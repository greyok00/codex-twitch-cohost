import { synthesizeTtsCloud } from '../../api/tauri';
import type { PlayerController } from './PlayerController';

export interface QueuePlayInput {
  text: string;
  voiceName?: string | null;
  volumePercent?: number;
  autoResumeAfterRemark: boolean;
}

export class TTSPlaybackQueue extends EventTarget {
  private queue = Promise.resolve();
  private activeToken = 0;
  private speaking = false;

  enqueue(player: PlayerController, input: QueuePlayInput): Promise<void> {
    this.queue = this.queue.then(() => this.run(player, input)).catch(() => undefined);
    return this.queue;
  }

  cancel(): void {
    this.activeToken += 1;
    if (typeof window !== 'undefined' && 'speechSynthesis' in window) {
      window.speechSynthesis.cancel();
    }
    this.speaking = false;
    this.dispatchEvent(new CustomEvent('tts_cancel'));
  }

  isSpeaking(): boolean {
    return this.speaking;
  }

  private async run(player: PlayerController, input: QueuePlayInput): Promise<void> {
    const token = ++this.activeToken;
    const now = player.getCurrentTime();
    player.pauseVideo();
    this.dispatchEvent(new CustomEvent('tts_start', { detail: { text: input.text, at: now } }));
    this.speaking = true;

    try {
      const dataUrl = await synthesizeTtsCloud(input.text, input.voiceName || null);
      if (token !== this.activeToken) return;
      await this.playAudio(dataUrl, token, input.volumePercent ?? 100);
    } finally {
      if (token === this.activeToken) {
        this.speaking = false;
        this.dispatchEvent(new CustomEvent('tts_end'));
        if (input.autoResumeAfterRemark) {
          player.playVideo();
        }
      }
    }
  }

  private async playAudio(dataUrl: string, token: number, volumePercent: number): Promise<void> {
    await new Promise<void>((resolve) => {
      const audio = new Audio(dataUrl);
      audio.volume = Math.max(0, Math.min(1, volumePercent / 100));
      const done = () => resolve();
      audio.onended = done;
      audio.onerror = done;
      void audio.play().catch(done);
      const checkCancel = window.setInterval(() => {
        if (token !== this.activeToken) {
          audio.pause();
          window.clearInterval(checkCancel);
          resolve();
        }
      }, 150);
      audio.onended = () => {
        window.clearInterval(checkCancel);
        resolve();
      };
      audio.onerror = () => {
        window.clearInterval(checkCancel);
        resolve();
      };
    });
  }
}
