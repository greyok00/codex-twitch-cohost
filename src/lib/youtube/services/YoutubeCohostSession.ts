import type {
  DeveloperSnapshot,
  RemarkResponse,
  RemarkGenerationRequest,
  PlaylistInfo,
  TranscriptSegment,
  TopicContextWindow,
  TranscriptSourceResult,
  YoutubeCohostSettings
} from '../types';
import { parseYouTubeInput } from '../utils';
import { CommentScheduler } from './CommentScheduler';
import type { SchedulerRuntimeState } from './CommentScheduler';
import { PlayerController } from './PlayerController';
import { RemarkGenerator } from './RemarkGenerator';
import { SessionStateStore } from './SessionStateStore';
import { TTSPlaybackQueue } from './TTSPlaybackQueue';
import { TranscriptContextService } from './TranscriptContextService';

export interface SessionCallbacks {
  onError?: (message: string) => void;
}

export interface PlayerControllerLike extends EventTarget {
  initialize(): Promise<void>;
  destroy(): void;
  loadFromInput(input: string): boolean;
  playVideo(): void;
  pauseVideo(): void;
  seekTo(seconds: number): void;
  getCurrentTime(): number;
  getDuration(): number;
  getPlayerState(): number;
  getVideoMetadata(): { title: string; videoId: string };
}

export interface TranscriptContextServiceLike {
  setSegments(segments: TranscriptSegment[], duration: number): void;
  getWindow(currentTime: number, backSeconds: number, forwardSeconds: number): TopicContextWindow;
}

export interface RemarkGeneratorLike {
  generate(input: RemarkGenerationRequest): Promise<RemarkResponse>;
}

export interface TTSPlaybackQueueLike extends EventTarget {
  enqueue(player: PlayerControllerLike, input: { text: string; voiceName?: string | null; volumePercent?: number; autoResumeAfterRemark: boolean }): Promise<void>;
  cancel(): void;
  isSpeaking(): boolean;
}

export interface YoutubeCohostSessionDeps {
  player?: PlayerControllerLike;
  scheduler?: CommentScheduler;
  contextService?: TranscriptContextServiceLike;
  remarkGenerator?: RemarkGeneratorLike;
  ttsQueue?: TTSPlaybackQueueLike;
}

export class YoutubeCohostSession {
  readonly state = new SessionStateStore();
  readonly player: PlayerControllerLike;
  readonly scheduler: CommentScheduler;
  readonly contextService: TranscriptContextServiceLike;
  readonly remarkGenerator: RemarkGeneratorLike;
  readonly ttsQueue: TTSPlaybackQueueLike;

  private readonly callbacks: SessionCallbacks;
  private settings: YoutubeCohostSettings;
  private runtime: SchedulerRuntimeState = {
    remarksSpokenThisMinute: 0,
    secondsSinceLastRemark: 999,
    repetitionMemory: [],
    skippedOpportunities: 0,
    nowSecond: 0
  };
  private evaluationBusy = false;
  private transcriptVersion = 0;
  private minuteWindow: number[] = [];
  private personalityPrompt = '';
  private modelMode: RemarkGenerationRequest['modelMode'] = 'medium';
  private mounted = false;
  private topicHistory: string[] = [];
  private carryoverTopics: string[] = [];
  private currentPlaylist: string | null = null;

  constructor(
    hostEl: HTMLElement,
    settings: YoutubeCohostSettings,
    callbacks: SessionCallbacks = {},
    deps: YoutubeCohostSessionDeps = {}
  ) {
    this.player = deps.player ?? new PlayerController(hostEl, 1000);
    this.scheduler = deps.scheduler ?? new CommentScheduler(42);
    this.contextService = deps.contextService ?? new TranscriptContextService();
    this.remarkGenerator = deps.remarkGenerator ?? new RemarkGenerator();
    this.ttsQueue = deps.ttsQueue ?? new TTSPlaybackQueue();
    this.settings = { ...settings };
    this.callbacks = callbacks;
  }

  async init(): Promise<void> {
    if (this.mounted) return;
    this.mounted = true;
    this.state.setPlaybackState('loading_video');
    await this.player.initialize();
    this.bindEvents();
    this.state.setPlaybackState('ready');
  }

  destroy(): void {
    this.ttsQueue.cancel();
    this.player.destroy();
  }

  setSettings(next: YoutubeCohostSettings): void {
    this.settings = { ...next };
  }

  setPersonalityPrompt(prompt: string): void {
    this.personalityPrompt = prompt.trim();
  }

  setModelMode(mode: RemarkGenerationRequest['modelMode']): void {
    this.modelMode = mode || 'medium';
  }

  setTranscript(result: TranscriptSourceResult, duration: number): void {
    this.transcriptVersion += 1;
    this.contextService.setSegments(result.segments, duration);
    this.state.setTranscriptMode(result.mode);
    this.state.setTranscriptStatus(result.message, result.quality, result.coverageScore);
    this.state.clearOnSeek();
  }

  loadVideo(urlOrId: string): boolean {
    const parsed = parseYouTubeInput(urlOrId);
    if (!parsed) return false;
    this.handleVideoTransition(parsed);
    this.state.setPlaybackState('loading_video');
    const ok = this.player.loadFromInput(urlOrId);
    if (ok) this.state.setPlaybackState('ready');
    return ok;
  }

  play(): void {
    this.player.playVideo();
  }

  pause(): void {
    this.player.pauseVideo();
  }

  seekTo(seconds: number): void {
    this.player.seekTo(seconds);
  }

  private bindEvents(): void {
    this.player.addEventListener('play', () => {
      if (!this.ttsQueue.isSpeaking()) this.state.setPlaybackState('playing');
    });
    this.player.addEventListener('pause', () => {
      if (!this.ttsQueue.isSpeaking()) this.state.setPlaybackState('ready');
    });
    this.player.addEventListener('ended', () => this.state.setPlaybackState('ended'));
    this.player.addEventListener('seek', () => {
      this.transcriptVersion += 1;
      this.runtime.repetitionMemory = [];
      this.runtime.callbackSuppressedUntilSeconds = this.player.getCurrentTime() + 6;
      this.state.clearOnSeek();
      if (this.ttsQueue.isSpeaking()) this.ttsQueue.cancel();
    });
    this.player.addEventListener('tick', (event) => {
      const tick = (event as CustomEvent<{ currentTime: number; duration: number }>).detail;
      this.onTick(tick.currentTime, tick.duration);
    });
    this.ttsQueue.addEventListener('tts_start', () => this.state.setPlaybackState('speaking_remark'));
    this.ttsQueue.addEventListener('tts_end', () => {
      this.state.setPlaybackState(this.player.getPlayerState() === 1 ? 'playing' : 'ready');
    });
  }

  private async onTick(currentTime: number, duration: number): Promise<void> {
    this.state.setTimeline(currentTime, duration);
    this.runtime.nowSecond += 1;
    this.runtime.secondsSinceLastRemark += 1;
    this.trimMinuteWindow(currentTime);
    if (this.player.getPlayerState() !== 1) return;
    if (this.ttsQueue.isSpeaking()) return;
    if (this.evaluationBusy) return;

    this.evaluationBusy = true;
    const versionAtStart = this.transcriptVersion;
    try {
      this.state.setPlaybackState('evaluating_comment');
      const window = this.contextService.getWindow(currentTime, 42, 20);
      this.state.setCurrentSegment(window.currentSegment?.text || '');
      const decision = this.scheduler.shouldInterrupt(window, this.settings, this.runtime, currentTime);
      this.state.setLastDecision(decision.reason, Math.max(0, Math.min(1, decision.components.total)));
      this.state.setDebug(this.buildDebug(window, decision, false, decision.reason));

      if (!decision.shouldInterrupt) {
        this.runtime.skippedOpportunities += 1;
        this.state.markSkipped();
        this.state.setPlaybackState('playing');
        return;
      }

      const generationInput: RemarkGenerationRequest = {
        context: window,
        humorStyle: this.settings.humorStyle,
        maxRemarkLengthSeconds: this.settings.maxRemarkLengthSeconds,
        relevanceStrictness: this.settings.relevanceStrictness,
        modelMode: this.modelMode,
        repetitionMemory: this.runtime.repetitionMemory,
        topicHistory: this.topicHistory.slice(0, 8),
        recentRemarks: this.runtime.repetitionMemory.slice(0, 8),
        personalityPrompt: this.buildPersonalityPrompt()
      };
      const remark = await this.remarkGenerator.generate(generationInput);
      if (versionAtStart !== this.transcriptVersion) {
        this.state.setPlaybackState('playing');
        return;
      }

      if (!remark.shouldSpeak) {
        const reason = remark.skipReason || 'generator declined';
        this.state.setLastDecision(reason, Math.max(0, Math.min(1, decision.components.total)));
        this.state.setDebug(this.buildDebug(window, decision, false, reason));
        this.state.setPlaybackState('playing');
        return;
      }

      this.runtime.repetitionMemory = [remark.remark, ...this.runtime.repetitionMemory].slice(0, 20);
      this.topicHistory = [window.topicSummary, ...this.topicHistory].slice(0, 18);
      this.runtime.topicHistory = [...this.topicHistory];
      this.runtime.secondsSinceLastRemark = 0;
      this.runtime.remarksSpokenThisMinute += 1;
      this.minuteWindow.push(currentTime);
      this.runtime.callbackSuppressedUntilSeconds = currentTime + Math.max(6, remark.estimatedDurationSeconds + 2);
      this.state.markRemarkSpoken(remark.remark);
      this.state.setDebug(this.buildDebug(window, decision, true, 'remark fired'));
      await this.ttsQueue.enqueue(this.player, {
        text: remark.remark,
        autoResumeAfterRemark: this.settings.autoResumeAfterRemark,
        volumePercent: 100
      });
    } catch (error) {
      const message = String(error);
      this.state.setError(message);
      this.callbacks.onError?.(message);
    } finally {
      this.evaluationBusy = false;
    }
  }

  private trimMinuteWindow(currentTime: number): void {
    this.minuteWindow = this.minuteWindow.filter((t) => currentTime - t <= 60);
    this.runtime.remarksSpokenThisMinute = this.minuteWindow.length;
  }

  private handleVideoTransition(parsed: PlaylistInfo): void {
    const nextPlaylist = parsed.playlistId || null;
    if (this.currentPlaylist && nextPlaylist && this.currentPlaylist === nextPlaylist) {
      this.carryoverTopics = [...this.topicHistory].slice(0, 6);
    } else {
      this.carryoverTopics = [];
    }
    this.currentPlaylist = nextPlaylist;
    this.topicHistory = [...this.carryoverTopics];
    this.runtime.topicHistory = [...this.topicHistory];
    this.runtime.callbackSuppressedUntilSeconds = (parsed.startSeconds || 0) + 8;
  }

  private buildPersonalityPrompt(): string {
    const carryover = this.carryoverTopics.length > 0
      ? `Recent playlist carryover topics: ${this.carryoverTopics.join(' | ')}. Avoid repeating the same callback structure.`
      : '';
    const guidance = 'Prioritize context and comedy over speed. Every remark should clearly connect to the current segment, speaker behavior, wording, or tone.';
    return [this.personalityPrompt, carryover, guidance].filter(Boolean).join('\n');
  }

  private buildDebug(
    transcriptWindow: TopicContextWindow,
    commentDecision: ReturnType<CommentScheduler['shouldInterrupt']>,
    fired: boolean,
    reason: string
  ): DeveloperSnapshot {
    return {
      timestamp: Date.now(),
      transcriptWindow,
      commentDecision,
      fired,
      reason
    };
  }
}
