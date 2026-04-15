import { transcribeMicChunkLocal } from '../../api/tauri';
import type { SpeechEngine, VoiceSessionCallbacks } from '../types';

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function getRuntimeFlags(): {
  __cohost_tts_speaking?: boolean;
  __cohost_recording_active?: boolean;
  __cohost_tts_suppressed_until?: number;
} {
  return window as Window & {
    __cohost_tts_speaking?: boolean;
    __cohost_recording_active?: boolean;
    __cohost_tts_suppressed_until?: number;
  };
}

export class LocalFallbackSpeechEngine implements SpeechEngine {
  kind: SpeechEngine['kind'];
  private readonly callbacks: VoiceSessionCallbacks;
  private readonly startingMessage: string;
  private readonly activeMessage: string;
  private active = false;
  private loopId = 0;

  constructor(
    callbacks: VoiceSessionCallbacks,
    kind: SpeechEngine['kind'] = 'local-fallback',
    messages: { starting: string; active: string } = {
      starting: 'Starting local Vosk STT...',
      active: 'Local Vosk STT active.'
    }
  ) {
    this.callbacks = callbacks;
    this.kind = kind;
    this.startingMessage = messages.starting;
    this.activeMessage = messages.active;
  }

  async start(): Promise<void> {
    this.active = true;
    const thisLoop = ++this.loopId;
    this.callbacks.onStatus('starting', this.startingMessage);
    void this.run(thisLoop);
  }

  private async run(loopId: number) {
    this.callbacks.onStatus('listening', this.activeMessage);
    while (this.active && loopId === this.loopId) {
      const runtime = getRuntimeFlags();
      if (
        runtime.__cohost_tts_speaking
        || runtime.__cohost_recording_active
        || (runtime.__cohost_tts_suppressed_until ?? 0) > Date.now()
      ) {
        await sleep(90);
        continue;
      }
      try {
        this.callbacks.onSpeechStart?.();
        const text = (await transcribeMicChunkLocal(900)).trim();
        this.callbacks.onSpeechEnd?.();
        if (!this.active || loopId !== this.loopId) break;
        if (text) {
          await this.callbacks.onFinal(text);
        }
      } catch (error) {
        this.callbacks.onSpeechEnd?.();
        this.callbacks.onError(String(error));
        this.callbacks.onStatus('error', 'Local Vosk STT failed.');
        break;
      }
      await sleep(25);
    }
  }

  async stop(): Promise<void> {
    this.active = false;
    this.loopId += 1;
    this.callbacks.onStatus('idle', 'Local Vosk STT stopped.');
  }

  async dispose(): Promise<void> {
    this.active = false;
    this.loopId += 1;
  }
}
