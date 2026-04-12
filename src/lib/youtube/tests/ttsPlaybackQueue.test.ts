import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TTSPlaybackQueue } from '../services/TTSPlaybackQueue';

vi.mock('../../api/tauri', () => ({
  synthesizeTtsCloud: vi.fn(async () => 'data:audio/wav;base64,AA==')
}));

class MockPlayer {
  paused = 0;
  played = 0;
  currentTime = 21;
  pauseVideo() {
    this.paused += 1;
  }
  playVideo() {
    this.played += 1;
  }
  getCurrentTime() {
    return this.currentTime;
  }
}

describe('TTSPlaybackQueue', () => {
  beforeEach(() => {
    class MockAudio {
      volume = 1;
      onended: null | (() => void) = null;
      onerror: null | (() => void) = null;
      constructor(_url: string) {}
      play() {
        queueMicrotask(() => this.onended?.());
        return Promise.resolve();
      }
      pause() {}
    }
    vi.stubGlobal('Audio', MockAudio);
  });

  it('pauses and resumes playback around tts', async () => {
    const queue = new TTSPlaybackQueue();
    const player = new MockPlayer();
    await queue.enqueue(player as any, {
      text: 'quick contextual roast',
      autoResumeAfterRemark: true,
      volumePercent: 100
    });
    expect(player.paused).toBe(1);
    expect(player.played).toBe(1);
  });
});
