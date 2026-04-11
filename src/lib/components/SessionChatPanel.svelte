<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { getSttConfig, setVoiceEnabled, submitStreamerPrompt, transcribeMicChunk } from '../api/tauri';
  import { authSessionsStore, botLogStore, chatStore, diagnosticsStore, errorBannerStore, eventStore, statusStore } from '../stores/app';

  let content = '';
  let sttReady = false;
  let sttTimer: number | null = null;
  let micLive = false;
  let micProcessing = false;
  let micLoopId = 0;
  let micStatus = 'Mic idle.';
  let micChunkMs = 2200;
  let lastMicTextNormalized = '';
  let lastMicTextAt = 0;

  $: combined = [
    ...$chatStore.map((m) => ({ ...m, source: 'viewer' as const })),
    ...$botLogStore.map((m) => ({ ...m, source: 'bot' as const })),
    ...$eventStore.map((m) => ({
      id: `event-${m.id}`,
      user: 'system',
      content: `${m.kind}: ${m.content}`,
      timestamp: m.timestamp,
      source: 'system' as const
    }))
  ]
    .sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp))
    .slice(0, 300);

  onMount(() => {
    void refreshSttReady();
    sttTimer = window.setInterval(() => void refreshSttReady(), 3500);
  });

  onDestroy(() => {
    (window as unknown as { __cohost_mic_live?: boolean }).__cohost_mic_live = false;
    micLive = false;
    micLoopId += 1;
    if (sttTimer !== null) {
      window.clearInterval(sttTimer);
    }
  });

  async function refreshSttReady() {
    try {
      const cfg = await getSttConfig();
      sttReady = !!(cfg.sttBinaryPath && cfg.sttModelPath && cfg.sttEnabled);
    } catch {
      sttReady = false;
    }
  }

  async function submit() {
    if (!content.trim()) return;
    const outgoing = content.trim();
    content = '';
    try {
      await submitStreamerPrompt(outgoing);
    } catch (error) {
      errorBannerStore.set('Local AI send failed: ' + String(error));
    }
  }

  $: activationBlockedReason = !$diagnosticsStore.providerState || $diagnosticsStore.providerState !== 'connected'
    ? 'Connect AI first, then connect chat.'
    : $statusStore.twitchState !== 'connected'
      ? 'Connect chat after AI.'
      : !$authSessionsStore.botTokenPresent || !$authSessionsStore.streamerTokenPresent
        ? 'Connect both Bot and Streamer auth first.'
        : '';
  $: activationBlocked = activationBlockedReason.length > 0;

  async function toggleMicInline() {
    if (activationBlocked) {
      errorBannerStore.set(activationBlockedReason);
      return;
    }
    if (!sttReady) {
      errorBannerStore.set('STT is not ready. Check Settings -> Voice to confirm STT status.');
      return;
    }

    if (micLive) {
      (window as unknown as { __cohost_mic_live?: boolean }).__cohost_mic_live = false;
      micLive = false;
      micLoopId += 1;
      micStatus = 'Mic stopped.';
      return;
    }

    await setVoiceEnabled(true);
    (window as unknown as { __cohost_mic_live?: boolean }).__cohost_mic_live = true;
    micLive = true;
    const thisLoop = ++micLoopId;
    micStatus = 'Mic live.';

    while (micLive && thisLoop === micLoopId) {
      micProcessing = true;
      try {
        const text = (await transcribeMicChunk(micChunkMs)).trim();
        if (text) {
          const normalized = text.toLowerCase().replace(/[^a-z0-9\s]/g, ' ').replace(/\s+/g, ' ').trim();
          const now = Date.now();
          const duplicate = normalized.length > 0 && normalized === lastMicTextNormalized && now - lastMicTextAt < 1800;
          if (!duplicate) {
            lastMicTextNormalized = normalized;
            lastMicTextAt = now;
            await submitStreamerPrompt(text);
            micStatus = `Heard: "${text}"`;
          }
        }
      } catch (error) {
        const msg = String(error);
        if (!msg.includes('busy')) {
          errorBannerStore.set('Mic transcription failed: ' + msg);
          (window as unknown as { __cohost_mic_live?: boolean }).__cohost_mic_live = false;
          micLive = false;
          micStatus = 'Mic stopped due to error.';
          break;
        }
      } finally {
        micProcessing = false;
      }
      await new Promise((resolve) => setTimeout(resolve, 120));
    }
  }

  async function openAvatarQuick() {
    if (activationBlocked) {
      errorBannerStore.set(activationBlockedReason);
      return;
    }
    const hasImage = !!localStorage.getItem('cohost_avatar_image');
    if (!hasImage) {
      errorBannerStore.set('No avatar loaded yet. Set it once in Settings -> Avatar Popup.');
      return;
    }
    try {
      const existing = await WebviewWindow.getByLabel('cohost-avatar');
      if (existing) {
        try {
          const raw = localStorage.getItem('cohost_avatar_size');
          if (raw) {
            const parsed = JSON.parse(raw) as { width?: number; height?: number };
            const w = Math.max(320, Math.min(1200, Number(parsed.width || 560) + 24));
            const h = Math.max(420, Math.min(1500, Number(parsed.height || 760) + 60));
            await existing.setSize(new LogicalSize(w, h));
          }
        } catch {
          // no-op
        }
        await existing.show();
        await existing.setFocus();
        return;
      }
      let popupW = 584;
      let popupH = 820;
      try {
        const raw = localStorage.getItem('cohost_avatar_size');
        if (raw) {
          const parsed = JSON.parse(raw) as { width?: number; height?: number };
          popupW = Math.max(320, Math.min(1200, Number(parsed.width || 560) + 24));
          popupH = Math.max(420, Math.min(1500, Number(parsed.height || 760) + 60));
        }
      } catch {
        // no-op
      }
      const win = new WebviewWindow('cohost-avatar', {
        url: '/avatar-popup.html',
        title: 'Cohost Avatar',
        width: popupW,
        height: popupH,
        minWidth: 420,
        minHeight: 520,
        resizable: true,
        alwaysOnTop: true
      });
      win.once('tauri://error', (e) => {
        errorBannerStore.set(`Failed to open avatar window: ${String((e as { payload?: unknown })?.payload ?? 'unknown error')}`);
      });
    } catch (error) {
      errorBannerStore.set('Avatar launch failed: ' + String(error));
    }
  }
</script>

<section class="card grid">
  <div class="head">
    <h3>💬 Main Session Chat Control</h3>
    <div class="quick-icons">
      <button
        class="btn avatar-icon"
        on:click={openAvatarQuick}
        disabled={activationBlocked}
        title="Open avatar popup"
        aria-label="Open avatar popup"
      >
        🧍
      </button>
      <button
        class="btn mic-icon {micLive ? 'live' : 'off'}"
        on:click={toggleMicInline}
        disabled={activationBlocked || !sttReady}
        aria-busy={micProcessing}
        title={micLive ? 'Stop mic' : 'Start mic'}
        aria-label={micLive ? 'Stop mic' : 'Start mic'}
      >
        🎤
      </button>
    </div>
  </div>
  <div class="health">
    <span class="chip {$authSessionsStore.botTokenPresent ? 'ok' : 'bad'}">Bot {$authSessionsStore.botTokenPresent ? 'ready' : 'missing'}</span>
    <span class="chip {$authSessionsStore.streamerTokenPresent ? 'ok' : 'bad'}">Streamer {$authSessionsStore.streamerTokenPresent ? 'ready' : 'missing'}</span>
    <span class="chip {$statusStore.twitchState === 'connected' ? 'ok' : 'bad'}">Chat {$statusStore.twitchState === 'connected' ? 'joined' : 'not joined'}</span>
    <span class="chip {$diagnosticsStore.providerState === 'connected' ? 'ok' : 'bad'}">AI {$diagnosticsStore.providerState === 'connected' ? 'online' : 'offline'}</span>
    <span class="chip {sttReady ? 'ok' : 'bad'}">STT {sttReady ? 'ready' : 'missing'}</span>
    <span class="chip {micLive ? 'ok' : 'bad'}">Mic {micLive ? 'live' : 'off'}</span>
  </div>
  <small class="muted state">
    <span class="light {$statusStore.twitchState === 'connected' ? 'on' : 'off'}" aria-hidden="true"></span>
    State: {$statusStore.twitchState} | Channel: {$statusStore.channel || 'not set'}
  </small>

  <div class="composer">
    <input bind:value={content} placeholder="Send local message to AI (not Twitch chat)..." on:keydown={(e) => e.key === 'Enter' && submit()} />
    <button class="btn" on:click={submit}>🧠 Send to AI</button>
  </div>

  <small class="muted">{micStatus}</small>

  <div class="feed">
    {#if combined.length === 0}
      <small class="muted">No chat or bot activity yet.</small>
    {:else}
      {#each combined as line (line.id)}
        <div class="line {line.source}">
          <span class="tag">
            {line.source === 'bot' ? '🤖 Bot' : line.source === 'system' ? '📣 System' : '👤 Chat'}
          </span>
          <strong>{line.user}</strong>
          <span>{line.content}</span>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.7rem;
  }
  .quick-icons {
    display: inline-flex;
    align-items: center;
    gap: 0.42rem;
  }
  .actions {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }
  .composer {
    display: grid;
    grid-template-columns: 1fr 132px;
    gap: 0.5rem;
  }
  .feed {
    max-height: 230px;
    overflow: auto;
    display: grid;
    gap: 0.5rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 0.6rem;
    background: linear-gradient(180deg, var(--panel-strong) 0%, color-mix(in srgb, var(--panel-strong) 86%, #000 14%) 100%);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
  }
  .mic-icon {
    width: 58px;
    height: 58px;
    min-width: 58px;
    min-height: 58px;
    padding: 0;
    display: inline-grid;
    place-items: center;
    font-size: 2rem;
    line-height: 1;
    border-width: 2px;
  }
  .mic-icon.live {
    border-color: color-mix(in srgb, var(--ok) 75%, var(--border) 25%) !important;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--ok) 30%, rgba(73, 84, 102, 0.76) 70%) 0%,
      color-mix(in srgb, var(--ok) 18%, rgba(50, 58, 72, 0.86) 82%) 100%
    ) !important;
    box-shadow:
      0 0 0 2px color-mix(in srgb, var(--ok) 42%, transparent 58%),
      0 0 28px color-mix(in srgb, var(--ok) 45%, transparent 55%);
  }
  .mic-icon.off {
    opacity: 1;
    background: linear-gradient(
      180deg,
      rgba(120, 129, 145, 0.88) 0%,
      rgba(87, 95, 110, 0.9) 100%
    ) !important;
    border-color: rgba(206, 214, 230, 0.5) !important;
  }
  .avatar-icon {
    width: 58px;
    height: 58px;
    min-width: 58px;
    min-height: 58px;
    padding: 0;
    display: inline-grid;
    place-items: center;
    font-size: 1.9rem;
    line-height: 1;
    border-width: 2px;
  }
  .health {
    display: flex;
    flex-wrap: wrap;
    gap: 0.42rem;
  }
  .chip {
    font-size: 0.92rem;
    font-weight: 700;
    padding: 0.26rem 0.6rem;
    border: 1px solid var(--border);
    background: rgba(255, 255, 255, 0.02);
  }
  .chip.ok {
    border-color: color-mix(in srgb, var(--ok) 68%, var(--border) 32%);
    background: color-mix(in srgb, var(--ok) 20%, transparent 80%);
    color: color-mix(in srgb, var(--text) 78%, #d4ffd8 22%);
  }
  .chip.bad {
    border-color: color-mix(in srgb, var(--danger) 62%, var(--border) 38%);
    background: color-mix(in srgb, var(--danger) 18%, transparent 82%);
    color: color-mix(in srgb, var(--text) 82%, #ffd4d4 18%);
  }
  .line {
    display: flex;
    gap: 0.45rem;
    align-items: baseline;
    flex-wrap: wrap;
    font-size: 1.03rem;
  }
  .tag {
    font-size: 1.05rem;
    font-weight: 700;
    color: var(--muted);
  }
  .line.bot {
    opacity: 0.95;
  }
  .line.system {
    opacity: 0.85;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 0.38rem;
  }
  .light {
    width: 1.08rem;
    height: 1.08rem;
    border: 1px solid rgba(0, 0, 0, 0.35);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.12) inset;
  }
  .light.on {
    background: #2bd35f;
  }
  .light.off {
    background: #d74646;
  }
</style>
