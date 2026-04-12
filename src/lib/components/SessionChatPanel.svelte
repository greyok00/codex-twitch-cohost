<script lang="ts">
  import { Button } from 'bits-ui';
  import { afterUpdate, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import { autoConfigureSttFast, getBehaviorSettings, getSttConfig, loadStatus, setBehaviorSettings, setModel, setVoiceEnabled, submitStreamerPrompt, transcribeMicChunk } from '../api/tauri';
  import { authSessionsStore, botLogStore, chatStore, diagnosticsStore, errorBannerStore, eventStore, statusStore } from '../stores/app';
  import { cohostControlsStore, type CohostModelMode } from '../stores/cohost';

  let content = '';
  let sttReady = false;
  let sttTimer: number | null = null;
  let micLive = false;
  let micProcessing = false;
  let micLoopId = 0;
  let micStatus = 'Mic idle.';
  let micChunkMs = 1000;
  let lastMicTextNormalized = '';
  let lastMicTextAt = 0;
  let sttStatusNote = 'STT not initialized.';
  let sttFixing = false;
  let lastAppliedModelMode: CohostModelMode | null = null;
  let selectedModelMode: CohostModelMode = 'medium';
  let videoRemarksPerMinute = 1.2;
  let autonomousReplies = true;
  let controlsReady = false;
  let feedEl: HTMLDivElement | null = null;

  const modelModeOptions = [
    { value: 'fast', label: 'Fast conversational' },
    { value: 'medium', label: 'Medium' },
    { value: 'long_context', label: 'Long context' }
  ];

  function modelForMode(mode: CohostModelMode): string {
    if (mode === 'fast') return 'qwen3:8b';
    if (mode === 'long_context') return 'phi4:14b';
    return 'gemma3:12b';
  }

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
    .sort((a, b) => Date.parse(a.timestamp) - Date.parse(b.timestamp))
    .slice(-300);

  afterUpdate(() => {
    if (!feedEl) return;
    requestAnimationFrame(() => {
      if (!feedEl) return;
      feedEl.scrollTop = feedEl.scrollHeight;
    });
  });

  onMount(() => {
    const initialControls = get(cohostControlsStore);
    selectedModelMode = initialControls.modelMode;
    videoRemarksPerMinute = initialControls.videoRemarksPerMinute;
    autonomousReplies = initialControls.autonomousReplies;
    controlsReady = true;
    void refreshSttReady();
    void hydrateBehaviorSettings();
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
      sttStatusNote = sttReady
        ? 'STT ready.'
        : `STT not ready (${cfg.sttEnabled ? 'missing binary/model' : 'disabled'}).`;
    } catch {
      sttReady = false;
      sttStatusNote = 'STT status unavailable.';
    }
  }

  async function hydrateBehaviorSettings() {
    try {
      const behavior = await getBehaviorSettings();
      cohostControlsStore.update((current) => {
        const next = {
          ...current,
          autonomousReplies: behavior.cohostMode
        };
        selectedModelMode = next.modelMode;
        videoRemarksPerMinute = next.videoRemarksPerMinute;
        autonomousReplies = next.autonomousReplies;
        return next;
      });
    } catch {
      // no-op
    }
  }

  async function ensureSttReady(): Promise<boolean> {
    await refreshSttReady();
    if (sttReady) return true;
    if (sttFixing) return false;
    sttFixing = true;
    try {
      const result = await autoConfigureSttFast();
      sttStatusNote = result.message || sttStatusNote;
    } catch (error) {
      sttStatusNote = `STT auto-fix failed: ${String(error)}`;
    } finally {
      sttFixing = false;
    }
    await refreshSttReady();
    return sttReady;
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

  async function applyModelMode(mode: CohostModelMode) {
    if (mode === lastAppliedModelMode) return;
    lastAppliedModelMode = mode;
    try {
      await setModel(modelForMode(mode));
      await loadStatus();
    } catch (error) {
      errorBannerStore.set('Model mode switch failed: ' + String(error));
    }
  }

  async function toggleAutonomousReplies(enabled: boolean) {
    try {
      await setBehaviorSettings(enabled, enabled ? 15 : null);
    } catch (error) {
      errorBannerStore.set('Autonomous chatter update failed: ' + String(error));
    }
  }

  function handleAutonomousToggle(event: Event) {
    const next = (event.currentTarget as HTMLInputElement).checked;
    autonomousReplies = next;
    void toggleAutonomousReplies(next);
  }

  $: if (controlsReady) {
    cohostControlsStore.set({
      modelMode: selectedModelMode,
      videoRemarksPerMinute,
      autonomousReplies
    });
  }

  $: void applyModelMode(selectedModelMode);

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
    if (!(await ensureSttReady())) {
      errorBannerStore.set(`STT is not ready. ${sttStatusNote} Go to Settings -> Voice if needed.`);
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

    let consecutiveErrors = 0;
    const isNonSpeechCaption = (value: string): boolean => {
      const t = value.trim().toLowerCase();
      if (!t) return true;
      if (/^\(?\s*(dramatic music|music|applause|laughter|laughing|silence|background noise|noise)\s*\)?[.!?]*$/.test(t)) {
        return true;
      }
      if (/^\[[^\]]{1,48}\]$/.test(t)) return true;
      if (/^\([^)]{1,48}\)$/.test(t)) return true;
      return false;
    };
    while (micLive && thisLoop === micLoopId) {
      micProcessing = true;
      try {
        const text = (await transcribeMicChunk(micChunkMs)).trim();
        consecutiveErrors = 0;
        if (text) {
          if (isNonSpeechCaption(text)) {
            micStatus = `Ignored non-speech transcript: "${text}"`;
            await new Promise((resolve) => setTimeout(resolve, 120));
            continue;
          }
          const normalized = text.toLowerCase().replace(/[^a-z0-9\s]/g, ' ').replace(/\s+/g, ' ').trim();
          const now = Date.now();
          const duplicate = normalized.length > 0 && normalized === lastMicTextNormalized && now - lastMicTextAt < 2600;
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
          consecutiveErrors += 1;
          micStatus = `Mic retrying (${consecutiveErrors}/3): ${msg}`;
          if (consecutiveErrors >= 3) {
            errorBannerStore.set('Mic transcription failed: ' + msg);
            (window as unknown as { __cohost_mic_live?: boolean }).__cohost_mic_live = false;
            micLive = false;
            micStatus = 'Mic stopped due to repeated STT errors.';
            break;
          }
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
        const visible = await existing.isVisible();
        if (visible) {
          await existing.hide();
          return;
        }
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
        alwaysOnTop: true,
        transparent: true,
        backgroundColor: '#00000000'
      });
      win.once('tauri://error', (e) => {
        errorBannerStore.set(`Failed to open avatar window: ${String((e as { payload?: unknown })?.payload ?? 'unknown error')}`);
      });
    } catch (error) {
      errorBannerStore.set('Avatar launch failed: ' + String(error));
    }
  }
</script>

<section class="card grid session-chat-panel">
  <div class="head">
    <h3>Main Session Chat Control</h3>
    <div class="quick-icons">
      <Button.Root
        class="p-btn btn avatar-icon {activationBlocked ? 'inactive' : ''}"
        on:click={openAvatarQuick}
        disabled={activationBlocked}
        title="Toggle avatar popup"
        aria-label="Toggle avatar popup"
      >
        <Icon name="avatar" />
        <span class="label">Avatar</span>
      </Button.Root>
      <Button.Root
        class="p-btn btn mic-icon {micLive ? 'live' : 'off'}"
        on:click={toggleMicInline}
        disabled={activationBlocked}
        aria-busy={micProcessing}
        title={micLive ? 'Stop mic' : 'Start mic'}
        aria-label={micLive ? 'Stop mic' : 'Start mic'}
      >
        <Icon name="mic" />
        <span class="label">{micLive ? 'Live' : 'Mic'}</span>
      </Button.Root>
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
  <small class="muted session-meta">
    <span class="light {$statusStore.twitchState === 'connected' ? 'on' : 'off'}" aria-hidden="true"></span>
    State: {$statusStore.twitchState} | Channel: {$statusStore.channel || 'not set'} | {micStatus}{sttFixing ? ' Auto-fixing STT…' : ''}
  </small>

  <div class="cohost-controls">
    <div class="cohost-cell label muted">Reply mode</div>
    <div class="cohost-cell">
      <UiSelect bind:value={selectedModelMode} options={modelModeOptions} placeholder="Response mode" />
    </div>
    <div class="cohost-cell label muted">Ambient chatter</div>
    <div class="cohost-cell">
      <label class="toggle-row cohost-toggle">
        <input
          type="checkbox"
          bind:checked={autonomousReplies}
          on:change={handleAutonomousToggle}
        />
        enabled
      </label>
    </div>
    <div class="cohost-cell label muted">Video pace</div>
    <div class="cohost-cell slider-wrap">
      <UiSlider bind:value={videoRemarksPerMinute} min={0} max={4} step={0.1} ariaLabel="Video comment speed" />
    </div>
    <div class="cohost-cell value muted">{videoRemarksPerMinute.toFixed(1)}/min</div>
  </div>

  <div class="feed" bind:this={feedEl}>
    {#if combined.length === 0}
      <small class="muted">No chat or bot activity yet.</small>
    {:else}
      {#each combined as line (line.id)}
        <div class="line {line.source}">
          <span class="tag">
            {line.source === 'bot' ? 'Bot' : line.source === 'system' ? 'System' : 'Chat'}
          </span>
          <strong>{line.user}</strong>
          <span>{line.content}</span>
        </div>
      {/each}
    {/if}
  </div>

  <div class="composer">
    <input bind:value={content} placeholder="Send local message to AI (not Twitch chat)..." on:keydown={(e) => e.key === 'Enter' && submit()} />
    <Button.Root class="p-btn btn" on:click={submit}><Icon name="send" />Send to AI</Button.Root>
  </div>
</section>
