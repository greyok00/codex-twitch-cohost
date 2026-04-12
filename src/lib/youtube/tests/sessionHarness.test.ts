import { beforeEach, describe, expect, it } from 'vitest';
import { CommentScheduler } from '../services/CommentScheduler';
import { YoutubeCohostSession, type PlayerControllerLike, type RemarkGeneratorLike, type TTSPlaybackQueueLike, type TranscriptContextServiceLike } from '../services/YoutubeCohostSession';
import type { RemarkGenerationRequest, RemarkResponse, TopicContextWindow, TranscriptSegment, YoutubeCohostSettings } from '../types';

const settings: YoutubeCohostSettings = {
  remarksPerMinute: 3.2,
  relevanceStrictness: 70,
  humorStyle: 'sarcastic',
  maxRemarkLengthSeconds: 8,
  interruptOnlyAtNaturalBreaks: true,
  captionsDebugOverlay: false,
  autoResumeAfterRemark: true,
  developerMode: true
};

function makeWindow(overrides: Partial<TopicContextWindow> = {}): TopicContextWindow {
  return {
    currentTime: 48,
    currentSegment: { startTime: 44, endTime: 52, text: 'The host makes a bold claim, then pauses to let it hang.', confidence: 0.86 },
    previousSegments: [],
    nextSegments: [],
    topicSummary: 'host makes a bold claim',
    entities: ['host'],
    tone: 'playful',
    pauseConfidence: 0.82,
    seriousnessScore: 0.08,
    humorOpportunityScore: 0.88,
    transcriptCoverageScore: 0.94,
    ...overrides
  };
}

async function flushAsyncWork(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

class MockPlayer extends EventTarget implements PlayerControllerLike {
  currentTime = 0;
  duration = 180;
  playerState = 2;
  pauseCount = 0;
  playCount = 0;

  async initialize(): Promise<void> {
    this.dispatchEvent(new CustomEvent('ready'));
  }

  destroy(): void {}

  loadFromInput(_input: string): boolean {
    return true;
  }

  playVideo(): void {
    this.playerState = 1;
    this.playCount += 1;
    this.dispatchEvent(new CustomEvent('play'));
  }

  pauseVideo(): void {
    this.playerState = 2;
    this.pauseCount += 1;
    this.dispatchEvent(new CustomEvent('pause'));
  }

  seekTo(seconds: number): void {
    const from = this.currentTime;
    this.currentTime = seconds;
    this.dispatchEvent(new CustomEvent('seek', { detail: { from, to: seconds } }));
  }

  getCurrentTime(): number {
    return this.currentTime;
  }

  getDuration(): number {
    return this.duration;
  }

  getPlayerState(): number {
    return this.playerState;
  }

  getVideoMetadata(): { title: string; videoId: string } {
    return { title: 'Mock video', videoId: 'abc123' };
  }

  tick(currentTime: number): void {
    this.currentTime = currentTime;
    this.dispatchEvent(new CustomEvent('tick', { detail: { currentTime, duration: this.duration } }));
  }
}

class MockContextService implements TranscriptContextServiceLike {
  window = makeWindow();

  setSegments(_segments: TranscriptSegment[], _duration: number): void {}

  getWindow(currentTime: number): TopicContextWindow {
    return {
      ...this.window,
      currentTime
    };
  }
}

class MockRemarkGenerator implements RemarkGeneratorLike {
  nextResponse: RemarkResponse = {
    shouldSpeak: true,
    remark: 'That claim walked in wearing borrowed confidence.',
    anchor: 'bold claim',
    topic: 'host makes a bold claim',
    confidence: 0.91,
    style: 'sarcastic',
    estimatedDurationSeconds: 4,
    skipReason: null
  };

  generateImpl?: (input: RemarkGenerationRequest) => Promise<RemarkResponse>;

  async generate(input: RemarkGenerationRequest): Promise<RemarkResponse> {
    if (this.generateImpl) return this.generateImpl(input);
    return this.nextResponse;
  }
}

class MockTtsQueue extends EventTarget implements TTSPlaybackQueueLike {
  spoken: string[] = [];
  speaking = false;

  async enqueue(player: PlayerControllerLike, input: { text: string; autoResumeAfterRemark: boolean }): Promise<void> {
    player.pauseVideo();
    this.speaking = true;
    this.dispatchEvent(new CustomEvent('tts_start', { detail: { text: input.text } }));
    this.spoken.push(input.text);
    await Promise.resolve();
    this.speaking = false;
    this.dispatchEvent(new CustomEvent('tts_end'));
    if (input.autoResumeAfterRemark) {
      player.playVideo();
    }
  }

  cancel(): void {
    this.speaking = false;
    this.dispatchEvent(new CustomEvent('tts_cancel'));
  }

  isSpeaking(): boolean {
    return this.speaking;
  }
}

class AlwaysFireScheduler extends CommentScheduler {
  override shouldInterrupt(context: TopicContextWindow) {
    return {
      shouldInterrupt: true,
      reason: 'forced fire for harness',
      minGapSeconds: 8,
      threshold: 0.2,
      components: {
        pauseConfidence: context.pauseConfidence,
        humorOpportunity: context.humorOpportunityScore,
        noveltyScore: 1,
        transcriptCoverage: context.transcriptCoverageScore,
        userSliderPressure: 1,
        repetitionPenalty: 0,
        seriousnessPenalty: 0,
        total: 0.91
      }
    };
  }
}

describe('YoutubeCohostSession harness', () => {
  beforeEach(() => {
    // no-op placeholder to keep test ordering explicit
  });

  it('pauses, speaks, and resumes when a contextual remark fires', async () => {
    const player = new MockPlayer();
    const contextService = new MockContextService();
    const generator = new MockRemarkGenerator();
    const ttsQueue = new MockTtsQueue();

    const session = new YoutubeCohostSession({} as HTMLElement, settings, {}, {
      player,
      scheduler: new AlwaysFireScheduler(7),
      contextService,
      remarkGenerator: generator,
      ttsQueue
    });

    await session.init();
    session.setTranscript(
      {
        mode: 'provider',
        segments: [makeWindow().currentSegment!],
        quality: 'high',
        coverageScore: 0.94,
        providerName: 'mock',
        message: 'Loaded transcript.'
      },
      180
    );

    player.playVideo();
    player.tick(48);
    await flushAsyncWork();

    const snapshot = session.state.getSnapshot();
    expect(ttsQueue.spoken).toEqual(['That claim walked in wearing borrowed confidence.']);
    expect(snapshot.lastRemark).toBe('That claim walked in wearing borrowed confidence.');
    expect(player.pauseCount).toBe(1);
    expect(player.playCount).toBeGreaterThanOrEqual(2);
    expect(snapshot.playbackState).toBe('playing');
  });

  it('drops stale remarks when the user seeks during generation', async () => {
    const player = new MockPlayer();
    const contextService = new MockContextService();
    const generator = new MockRemarkGenerator();
    const ttsQueue = new MockTtsQueue();

    let resolveRemark!: (value: RemarkResponse) => void;
    generator.generateImpl = () =>
      new Promise((resolve) => {
        resolveRemark = resolve;
      });

    const session = new YoutubeCohostSession({} as HTMLElement, settings, {}, {
      player,
      scheduler: new AlwaysFireScheduler(7),
      contextService,
      remarkGenerator: generator,
      ttsQueue
    });

    await session.init();
    session.setTranscript(
      {
        mode: 'provider',
        segments: [makeWindow().currentSegment!],
        quality: 'high',
        coverageScore: 0.94,
        providerName: 'mock',
        message: 'Loaded transcript.'
      },
      180
    );

    player.playVideo();
    player.tick(52);
    player.seekTo(97);
    resolveRemark({
      shouldSpeak: true,
      remark: 'Too stale to use now.',
      anchor: 'claim',
      topic: 'stale',
      confidence: 0.9,
      style: 'sarcastic',
      estimatedDurationSeconds: 4,
      skipReason: null
    });
    await flushAsyncWork();

    const snapshot = session.state.getSnapshot();
    expect(ttsQueue.spoken).toHaveLength(0);
    expect(snapshot.lastRemark).toBe('');
    expect(snapshot.lastDecisionReason).toContain('seek');
  });
});
