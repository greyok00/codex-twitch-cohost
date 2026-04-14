import { get } from 'svelte/store';
import { setRecordingSpeechBlock, stopBotSpeech, submitVoiceSessionFrame } from '../api/tauri';
import { botLogStore } from '../stores/app';
import { voiceSessionStore } from './store';
import { BrowserSpeechEngine, browserSpeechSupported } from './engines/browserSpeech';
import { LocalFallbackSpeechEngine } from './engines/localFallback';
import { WorkerBackedTranscriptService } from './WorkerBackedTranscriptService';
import { buildVoiceInputFrame } from './VoiceFrameBuilder';
import type { SpeechEngine, VoiceSessionCallbacks, VoiceSessionStartOptions, VoiceSessionStatus } from './types';

export class VoiceSessionController {
  private engine: SpeechEngine | null = null;
  private transcriptService = new WorkerBackedTranscriptService();
  private aiStartedAt = 0;
  private startedAt = 0;
  private readonly mode: VoiceSessionStartOptions['mode'];
  private readonly callerName?: string;

  constructor(options: VoiceSessionStartOptions) {
    this.mode = options.mode;
    this.callerName = options.callerName;
  }

  private setStatus(status: VoiceSessionStatus, detail?: string) {
    voiceSessionStore.update((state) => ({
      ...state,
      mode: this.mode,
      status,
      engine: this.engine?.kind ?? 'none',
      micEnabled: status !== 'idle',
      lastError: status === 'error' ? detail || state.lastError : state.lastError
    }));
  }

  private bindCallbacks(): VoiceSessionCallbacks {
    return {
      onInterim: (text) => {
        void this.transcriptService.pushInterim(text).then(({ interim, firstInterimLatencyMs }) => {
          voiceSessionStore.update((state) => ({
            ...state,
            interimText: interim,
            status: 'listening',
            engine: this.engine?.kind ?? state.engine,
            firstInterimLatencyMs: state.firstInterimLatencyMs ?? firstInterimLatencyMs
          }));
        });
      },
      onFinal: async (text) => {
        const normalized = this.engine?.kind === 'browser-speech'
          ? await this.transcriptService.pushFinal(text)
          : { committed: text.trim() || null, finalLatencyMs: this.startedAt ? Date.now() - this.startedAt : null };
        const committed = normalized.committed;
        if (!committed) {
          voiceSessionStore.update((state) => ({
            ...state,
            droppedCount: state.droppedCount + 1,
            interimText: ''
          }));
          return;
        }
        this.aiStartedAt = Date.now();
        voiceSessionStore.update((state) => ({
          ...state,
          interimText: '',
          lastFinalText: committed,
          finalLatencyMs: normalized.finalLatencyMs,
          status: 'processing'
        }));
        const current = get(voiceSessionStore);
        const frame = await buildVoiceInputFrame({
          sessionId: current.sessionId,
          mode: this.mode,
          engine: this.engine?.kind ?? 'none',
          transcript: committed,
          finalLatencyMs: normalized.finalLatencyMs
        });
        await submitVoiceSessionFrame(frame, this.callerName ?? (this.mode === 'public' ? 'guest' : null));
      },
      onStatus: (status, detail) => {
        this.setStatus(status, detail);
        if (status === 'listening') {
          voiceSessionStore.update((state) => ({
            ...state,
            engine: this.engine?.kind ?? state.engine
          }));
        }
      },
      onError: (message) => {
        voiceSessionStore.update((state) => ({
          ...state,
          lastError: message,
          status: 'error'
        }));
      },
      onSpeechStart: () => {
        stopBotSpeech();
        setRecordingSpeechBlock(true);
        voiceSessionStore.update((state) => ({ ...state, speakingBlocked: true, status: 'listening' }));
      },
      onSpeechEnd: () => {
        setRecordingSpeechBlock(false);
        voiceSessionStore.update((state) => ({ ...state, speakingBlocked: false }));
      }
    };
  }

  async start() {
    await this.stop();
    await this.transcriptService.reset();
    this.startedAt = Date.now();
    this.aiStartedAt = 0;
    this.transcriptService.setStartedAt(this.startedAt);
    voiceSessionStore.update((state) => ({
      ...state,
      mode: this.mode,
      micEnabled: true,
      status: 'starting',
      engine: 'none',
      interimText: '',
      lastFinalText: '',
      firstInterimLatencyMs: null,
      finalLatencyMs: null,
      aiLatencyMs: null,
      lastError: null
    }));
    const callbacks = this.bindCallbacks();
    this.engine = browserSpeechSupported()
      ? new BrowserSpeechEngine(callbacks)
      : new LocalFallbackSpeechEngine(callbacks);
    voiceSessionStore.update((state) => ({
      ...state,
      engine: this.engine?.kind ?? 'none',
      restartCount: state.restartCount + (state.micEnabled ? 1 : 0)
    }));
    await this.engine.start();
  }

  async stop() {
    setRecordingSpeechBlock(false);
    if (this.engine) {
      try {
        await this.engine.stop();
        await this.engine.dispose();
      } catch {
        // no-op
      }
    }
    this.engine = null;
    await this.transcriptService.reset();
    voiceSessionStore.update((state) => ({
      ...state,
      mode: this.mode,
      engine: 'none',
      status: 'idle',
      interimText: '',
      speakingBlocked: false,
      micEnabled: false
    }));
  }

  markReplying() {
    voiceSessionStore.update((state) => ({
      ...state,
      status: 'replying',
      aiLatencyMs: this.aiStartedAt ? Date.now() - this.aiStartedAt : state.aiLatencyMs
    }));
  }

  markTtsStart() {
    voiceSessionStore.update((state) => ({
      ...state,
      ttsLatencyMs: this.aiStartedAt ? Date.now() - this.aiStartedAt : state.ttsLatencyMs
    }));
  }
}

let ownerController: VoiceSessionController | null = null;

export function getOwnerVoiceSessionController() {
  if (!ownerController) {
    ownerController = new VoiceSessionController({ mode: 'owner' });
  }
  return ownerController;
}

export function syncVoiceSessionWithBotReplies() {
  let lastSeen = '';
  return botLogStore.subscribe((items) => {
    const latest = items[0];
    if (!latest || latest.id === lastSeen) return;
    lastSeen = latest.id;
    const controller = getOwnerVoiceSessionController();
    controller.markReplying();
  });
}

export function currentVoiceSessionState() {
  return get(voiceSessionStore);
}
