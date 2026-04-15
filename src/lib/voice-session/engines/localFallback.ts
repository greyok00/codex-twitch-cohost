import { transcribeMicChunk } from '../../api/tauri';
import type { SpeechEngine, VoiceSessionCallbacks } from '../types';

export class LocalFallbackSpeechEngine implements SpeechEngine {
  kind: SpeechEngine['kind'] = 'local-fallback';
  private readonly callbacks: VoiceSessionCallbacks;
  private active = false;
  private loopId = 0;

  constructor(callbacks: VoiceSessionCallbacks) {
    this.callbacks = callbacks;
  }

  async start(): Promise<void> {
    this.active = true;
    const thisLoop = ++this.loopId;
    this.callbacks.onStatus('starting', 'Starting local Vosk STT...');
    void this.run(thisLoop);
  }

  private async run(loopId: number) {
    this.callbacks.onStatus('listening', 'Local Vosk STT active.');
    while (this.active && loopId === this.loopId) {
      try {
        const text = (await transcribeMicChunk(1100)).trim();
        if (!this.active || loopId !== this.loopId) break;
        if (text) {
          await this.callbacks.onFinal(text);
        }
      } catch (error) {
        this.callbacks.onError(String(error));
        this.callbacks.onStatus('error', 'Local Vosk STT failed.');
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 80));
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
