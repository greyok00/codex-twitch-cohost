import { onLiveSttEvent, setAssemblyAiLiveSttPaused, startAssemblyAiLiveStt, stopAssemblyAiLiveStt } from '../../../frontend-api';
import type { SpeechEngine, VoiceSessionCallbacks } from '../types';

export class AssemblyAiBackendSpeechEngine implements SpeechEngine {
  kind: SpeechEngine['kind'] = 'assemblyai-realtime';
  private readonly callbacks: VoiceSessionCallbacks;
  private unlisten: (() => void) | null = null;
  private active = false;
  private speechActive = false;

  constructor(callbacks: VoiceSessionCallbacks) {
    this.callbacks = callbacks;
  }

  async start(): Promise<void> {
    this.active = true;
    this.callbacks.onStatus('starting', 'Connecting AssemblyAI Live...');
    this.unlisten = await onLiveSttEvent((payload) => {
      if (!this.active) return;
      if (payload.kind === 'interim') {
        if (!this.speechActive) {
          this.speechActive = true;
          this.callbacks.onSpeechStart?.();
        }
        this.callbacks.onInterim((payload.text || '').trim());
        return;
      }
      if (payload.kind === 'final') {
        const text = (payload.text || '').trim();
        if (text) {
          if (!this.speechActive) {
            this.speechActive = true;
            this.callbacks.onSpeechStart?.();
          }
          void this.callbacks.onFinal(text);
        }
        if (this.speechActive) {
          this.speechActive = false;
          this.callbacks.onSpeechEnd?.();
        }
        return;
      }
      if (payload.kind === 'error') {
        const detail = (payload.detail || 'AssemblyAI live STT failed.').trim();
        if (this.speechActive) {
          this.speechActive = false;
          this.callbacks.onSpeechEnd?.();
        }
        this.callbacks.onError(detail);
        this.callbacks.onStatus('error', detail);
        return;
      }
      if (payload.kind === 'status') {
        if ((payload.detail || '').toLowerCase().includes('paused') && this.speechActive) {
          this.speechActive = false;
          this.callbacks.onSpeechEnd?.();
        }
        this.callbacks.onStatus('listening', (payload.detail || 'AssemblyAI Live active.').trim());
      }
    });
    await startAssemblyAiLiveStt();
  }

  async stop(): Promise<void> {
    this.active = false;
    if (this.speechActive) {
      this.speechActive = false;
      this.callbacks.onSpeechEnd?.();
    }
    if (this.unlisten) {
      this.unlisten();
      this.unlisten = null;
    }
    await stopAssemblyAiLiveStt().catch(() => undefined);
    this.callbacks.onStatus('idle', 'AssemblyAI Live stopped.');
  }

  async dispose(): Promise<void> {
    await this.stop();
  }

  static async setPaused(paused: boolean): Promise<void> {
    await setAssemblyAiLiveSttPaused(paused).catch(() => undefined);
  }
}
