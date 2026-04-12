import type { PlayerSeekEvent, PlayerTickEvent } from '../types';
import { parseYouTubeInput } from '../utils';

type YTPlayer = {
  loadVideoById: (opts: { videoId: string; startSeconds?: number }) => void;
  cueVideoById: (opts: { videoId: string; startSeconds?: number }) => void;
  loadPlaylist?: (opts: { listType?: 'playlist'; list: string; index?: number; startSeconds?: number }) => void;
  playVideo: () => void;
  pauseVideo: () => void;
  seekTo: (seconds: number, allowSeekAhead?: boolean) => void;
  getCurrentTime: () => number;
  getDuration: () => number;
  getPlayerState: () => number;
  getVideoData: () => { title?: string; video_id?: string };
  destroy: () => void;
};

declare global {
  interface Window {
    YT: any;
    onYouTubeIframeAPIReady?: () => void;
  }
}

export class PlayerController extends EventTarget {
  private player: YTPlayer | null = null;
  private hostEl: HTMLElement;
  private tickTimer: number | null = null;
  private lastTime = 0;
  private readonly tickMs: number;

  constructor(hostEl: HTMLElement, tickMs = 1000) {
    super();
    this.hostEl = hostEl;
    this.tickMs = tickMs;
  }

  async initialize(): Promise<void> {
    await this.ensureIframeApi();
    if (!window.YT?.Player) throw new Error('YouTube Player API unavailable');

    await new Promise<void>((resolve, reject) => {
      try {
        this.player = new window.YT.Player(this.hostEl, {
          width: '100%',
          height: '100%',
          playerVars: {
            autoplay: 0,
            controls: 1,
            rel: 0,
            modestbranding: 1,
            iv_load_policy: 3,
            playsinline: 1
          },
          events: {
            onReady: () => {
              this.dispatchEvent(new CustomEvent('ready'));
              this.startTicker();
              resolve();
            },
            onStateChange: (ev: { data: number }) => this.handleStateChange(ev.data),
            onError: (ev: { data: number }) => reject(new Error(`YouTube player error ${ev.data}`))
          }
        }) as YTPlayer;
      } catch (error) {
        reject(error as Error);
      }
    });
  }

  loadFromInput(input: string): boolean {
    const parsed = parseYouTubeInput(input);
    if (!parsed || !this.player) return false;
    if (parsed.playlistId && this.player.loadPlaylist) {
      this.player.loadPlaylist({
        listType: 'playlist',
        list: parsed.playlistId,
        startSeconds: parsed.startSeconds
      });
      if (parsed.videoId) {
        // Seek to specific video in playlist by loading requested id as a starting anchor.
        this.player.loadVideoById({ videoId: parsed.videoId, startSeconds: parsed.startSeconds });
      }
    } else {
      this.player.loadVideoById({ videoId: parsed.videoId, startSeconds: parsed.startSeconds });
    }
    this.dispatchEvent(new CustomEvent('video_loaded', { detail: parsed }));
    return true;
  }

  playVideo(): void {
    this.player?.playVideo();
  }

  pauseVideo(): void {
    this.player?.pauseVideo();
  }

  seekTo(seconds: number): void {
    const from = this.getCurrentTime();
    this.player?.seekTo(seconds, true);
    const event: PlayerSeekEvent = { from, to: seconds };
    this.dispatchEvent(new CustomEvent<PlayerSeekEvent>('seek', { detail: event }));
  }

  getCurrentTime(): number {
    return this.player?.getCurrentTime?.() || 0;
  }

  getDuration(): number {
    return this.player?.getDuration?.() || 0;
  }

  getPlayerState(): number {
    return this.player?.getPlayerState?.() ?? -1;
  }

  getVideoMetadata(): { title: string; videoId: string } {
    const data = this.player?.getVideoData?.() || {};
    return { title: data.title || '', videoId: data.video_id || '' };
  }

  destroy(): void {
    if (this.tickTimer !== null) {
      window.clearInterval(this.tickTimer);
      this.tickTimer = null;
    }
    this.player?.destroy();
    this.player = null;
  }

  private handleStateChange(nextState: number): void {
    // -1 unstarted, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued
    if (nextState === 1) this.dispatchEvent(new CustomEvent('play'));
    if (nextState === 2) this.dispatchEvent(new CustomEvent('pause'));
    if (nextState === 0) this.dispatchEvent(new CustomEvent('ended'));
    this.dispatchEvent(new CustomEvent('state_change', { detail: nextState }));
  }

  private startTicker(): void {
    if (this.tickTimer !== null) return;
    this.tickTimer = window.setInterval(() => {
      const currentTime = this.getCurrentTime();
      const duration = this.getDuration();
      const tick: PlayerTickEvent = { currentTime, duration };
      this.dispatchEvent(new CustomEvent<PlayerTickEvent>('tick', { detail: tick }));
      if (Math.abs(currentTime - this.lastTime) > 4) {
        const seekEv: PlayerSeekEvent = { from: this.lastTime, to: currentTime };
        this.dispatchEvent(new CustomEvent<PlayerSeekEvent>('seek', { detail: seekEv }));
      }
      this.lastTime = currentTime;
    }, this.tickMs);
  }

  private async ensureIframeApi(): Promise<void> {
    if (window.YT?.Player) return;

    await new Promise<void>((resolve, reject) => {
      const scriptId = 'youtube-iframe-api';
      const existing = document.getElementById(scriptId) as HTMLScriptElement | null;

      const onReady = () => resolve();
      const onTimeout = window.setTimeout(() => reject(new Error('Timed out loading YouTube API')), 10000);
      const prev = window.onYouTubeIframeAPIReady;
      window.onYouTubeIframeAPIReady = () => {
        window.clearTimeout(onTimeout);
        prev?.();
        onReady();
      };

      if (!existing) {
        const script = document.createElement('script');
        script.id = scriptId;
        script.src = 'https://www.youtube.com/iframe_api';
        script.async = true;
        script.onerror = () => {
          window.clearTimeout(onTimeout);
          reject(new Error('Failed loading YouTube IFrame API'));
        };
        document.head.appendChild(script);
      }
    });
  }
}
