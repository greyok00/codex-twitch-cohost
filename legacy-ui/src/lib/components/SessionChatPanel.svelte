<script lang="ts">
  import { Button } from 'bits-ui';
  import { afterUpdate, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import Icon from './ui/Icon.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import AvatarRuntime from './AvatarRuntime.svelte';
  import {
    autoConfigureSttFast,
    connectChat,
    disconnectChat,
    getBehaviorSettings,
    getSttConfig,
    loadStatus,
    setBehaviorSettings,
    setModel,
    stopBotSpeech,
    submitVoiceSessionPrompt
  } from '../api/tauri';
  import { authSessionsStore, botLogStore, chatStore, diagnosticsStore, errorBannerStore, eventStore, statusStore } from '../stores/app';
  import { cohostControlsStore, type CohostModelMode } from '../stores/cohost';
  import { browserSpeechSupported } from '../voice-session/engines/browserSpeech';
  import { getOwnerVoiceSessionController, syncVoiceSessionWithBotReplies } from '../voice-session/VoiceSessionController';
  import { voiceSessionStore } from '../voice-session/store';

  let content = '';
  let sttReady = false;
  let sttTimer: number | null = null;
  let selectedModelMode: CohostModelMode = 'medium';
  let cohostPace = 0.6;
  let autonomousReplies = false;
  let postBotRepliesToTwitch = false;
  let topicContinuationMode = false;
  let controlsReady = false;
  let feedEl: HTMLDivElement | null = null;
  let twitchModeBusy = false;
  let sttFixing = false;
  let lastAppliedModelMode: CohostModelMode | null = null;
  let lastAppliedBehaviorSignature = '';
  let unsubscribeVoiceReplies: (() => void) | null = null;
  let showAvatarInline = false;

  const modeMeta: Record<CohostModelMode, { label: string; detail: string }> = {
    fast: { label: 'Fast', detail: 'Lowest latency, smallest model.' },
    medium: { label: 'Medium', detail: 'Balanced speed and context.' },
    long_context: { label: 'Long', detail: 'More context, slower replies.' }
  };
  const modeOrder: CohostModelMode[] = ['fast', 'medium', 'long_context'];

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

  $: browserSpeechReady = browserSpeechSupported();
  $: sessionState = $voiceSessionStore;
  $: micLive = sessionState.micEnabled;
  $: micBusy = sessionState.status === 'starting' || sessionState.status === 'processing';
  $: conversationPhase = sessionState.status;
  $: liveTranscriptDraft = sessionState.interimText;
  $: micStatus = sessionState.lastError
    ? `Voice error: ${sessionState.lastError}`
    : sessionState.status === 'idle'
      ? 'Mic idle.'
      : sessionState.status === 'starting'
        ? 'Starting voice session...'
        : sessionState.status === 'listening'
          ? `Listening with ${sessionState.engine === 'browser-speech' ? 'browser speech' : 'local fallback'}...`
          : sessionState.status === 'processing'
            ? 'Speech committed. Sending to AI...'
            : sessionState.status === 'replying'
              ? 'Bot replying...'
              : 'Voice session error.';
  $: sessionRuntimeLabel = browserSpeechReady ? 'Browser speech primary' : 'Local fallback STT';
  $: controlStatus = `Mode ${modeMeta[selectedModelMode].label}. Auto comments ${autonomousReplies ? 'on' : 'off'}. Keep talking ${topicContinuationMode ? 'on' : 'off'}. ${sessionRuntimeLabel}.`;

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
    cohostPace = initialControls.videoRemarksPerMinute;
    autonomousReplies = initialControls.autonomousReplies;

    void refreshSttReady();
    void hydrateBehaviorSettings().finally(() => {
      controlsReady = true;
    });
    sttTimer = window.setInterval(() => void refreshSttReady(), 3500);
    unsubscribeVoiceReplies = syncVoiceSessionWithBotReplies();
  });

  onDestroy(() => {
    if (sttTimer !== null) window.clearInterval(sttTimer);
    if (unsubscribeVoiceReplies) unsubscribeVoiceReplies();
    void getOwnerVoiceSessionController().stop();
  });

  function scheduledMinutesForPace(rate: number): number | null {
    if (rate <= 0) return null;
    if (rate >= 3.5) return 1;
    if (rate >= 2.0) return 2;
    if (rate >= 1.0) return 3;
    if (rate >= 0.6) return 5;
    if (rate >= 0.3) return 10;
    return 15;
  }

  function replyIntervalMsForPace(rate: number): number {
    if (rate <= 0) return 60_000;
    return Math.max(1200, Math.min(60_000, Math.round(60_000 / rate)));
  }

  function paceFromBehaviorSettings(behavior: Awaited<ReturnType<typeof getBehaviorSettings>>): number {
    if (!behavior.cohostMode) return 0.6;
    const interval = Number(behavior.minimumReplyIntervalMs ?? 0);
    if (interval > 0) return Math.max(0, Math.min(4, 60_000 / interval));
    const scheduled = Number(behavior.scheduledMessagesMinutes ?? 0);
    if (scheduled > 0) return Math.max(0, Math.min(4, 1 / scheduled));
    return 0.6;
  }

  function modelForMode(mode: CohostModelMode): string {
    if (mode === 'fast') return 'llama3.2:3b';
    if (mode === 'long_context') return 'gemma3:12b';
    return 'qwen3:8b';
  }

  async function refreshSttReady() {
    if (browserSpeechReady) {
      sttReady = true;
      return;
    }
    try {
      const cfg = await getSttConfig();
      sttReady = !!(cfg.sttEnabled && cfg.sttBinaryPath && cfg.sttModelPath);
    } catch {
      sttReady = false;
    }
  }

  async function ensureVoiceReady(): Promise<boolean> {
    if (browserSpeechReady) return true;
    await refreshSttReady();
    if (sttReady) return true;
    if (sttFixing) return false;
    sttFixing = true;
    try {
      await autoConfigureSttFast();
    } catch (error) {
      errorBannerStore.set(`STT auto-configure failed: ${String(error)}`);
    } finally {
      sttFixing = false;
    }
    await refreshSttReady();
    return sttReady;
  }

  async function hydrateBehaviorSettings() {
    try {
      const behavior = await getBehaviorSettings();
      autonomousReplies = !!behavior.cohostMode;
      cohostPace = paceFromBehaviorSettings(behavior);
      postBotRepliesToTwitch = !!behavior.postBotMessagesToTwitch;
      topicContinuationMode = !!behavior.topicContinuationMode;
      cohostControlsStore.set({
        modelMode: selectedModelMode,
        videoRemarksPerMinute: cohostPace,
        autonomousReplies
      });
    } catch {
      // no-op
    }
  }

  async function submit() {
    if (!content.trim()) return;
    const outgoing = content.trim();
    content = '';
    try {
      await submitVoiceSessionPrompt(outgoing);
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

  async function syncBehavior() {
    try {
      await setBehaviorSettings(
        autonomousReplies,
        autonomousReplies ? scheduledMinutesForPace(cohostPace) : null,
        replyIntervalMsForPace(cohostPace),
        postBotRepliesToTwitch,
        topicContinuationMode
      );
    } catch (error) {
      errorBannerStore.set('Behavior update failed: ' + String(error));
    }
  }

  function setMode(mode: CohostModelMode) {
    selectedModelMode = mode;
  }

  function setAutonomousReplies(next: boolean) {
    autonomousReplies = next;
    void syncBehavior();
  }

  function setTopicContinuationMode(next: boolean) {
    topicContinuationMode = next;
    void syncBehavior();
  }

  function setBotTwitchPosting(next: boolean) {
    postBotRepliesToTwitch = next;
    void syncBehavior();
  }

  $: if (controlsReady) {
    cohostControlsStore.set({
      modelMode: selectedModelMode,
      videoRemarksPerMinute: cohostPace,
      autonomousReplies
    });
  }

  $: if (controlsReady) {
    const signature = [
      autonomousReplies ? '1' : '0',
      postBotRepliesToTwitch ? '1' : '0',
      topicContinuationMode ? '1' : '0',
      String(autonomousReplies ? scheduledMinutesForPace(cohostPace) ?? 0 : 0),
      String(replyIntervalMsForPace(cohostPace))
    ].join(':');
    if (signature !== lastAppliedBehaviorSignature) {
      lastAppliedBehaviorSignature = signature;
      void syncBehavior();
    }
  }

  $: void applyModelMode(selectedModelMode);

  async function toggleMicInline() {
    const controller = getOwnerVoiceSessionController();
    if (sessionState.micEnabled) {
      await controller.stop();
      return;
    }
    if (!(await ensureVoiceReady())) {
      errorBannerStore.set('Voice input is not ready. Browser speech is unavailable and local STT could not be prepared.');
      return;
    }
    stopBotSpeech();
    try {
      await controller.start();
    } catch (error) {
      errorBannerStore.set('Mic start failed: ' + String(error));
    }
  }

  async function toggleTwitchMode() {
    if (twitchModeBusy) return;
    twitchModeBusy = true;
    try {
      if ($statusStore.twitchState === 'connected') {
        await disconnectChat();
        await loadStatus();
        return;
      }
      if (!$authSessionsStore.botTokenPresent || !$authSessionsStore.streamerTokenPresent) {
        errorBannerStore.set('Connect bot and streamer first to enable Twitch mode.');
        return;
      }
      await connectChat();
      await loadStatus();
    } catch (error) {
      errorBannerStore.set('Twitch mode switch failed: ' + String(error));
    } finally {
      twitchModeBusy = false;
    }
  }

  function openAvatarQuick() {
    showAvatarInline = !showAvatarInline;
  }
</script>

<section class="card grid session-chat-panel">
  <div class="head">
    <h3>Main Session Chat Control</h3>
    <div class="health">
      <span class="chip {$authSessionsStore.botTokenPresent ? 'ok' : 'bad'}">Bot {$authSessionsStore.botTokenPresent ? 'ready' : 'missing'}</span>
      <span class="chip {$authSessionsStore.streamerTokenPresent ? 'ok' : 'bad'}">Streamer {$authSessionsStore.streamerTokenPresent ? 'ready' : 'missing'}</span>
      <span class="chip {$statusStore.twitchState === 'connected' ? 'ok' : 'bad'}">Chat {$statusStore.twitchState === 'connected' ? 'joined' : 'not joined'}</span>
      <span class="chip {$diagnosticsStore.providerState === 'connected' ? 'ok' : 'bad'}">AI {$diagnosticsStore.providerState === 'connected' ? 'online' : 'offline'}</span>
      <span class="chip {sttReady ? 'ok' : 'bad'}">Voice {sttReady ? 'ready' : 'missing'}</span>
      <span class="chip {micLive ? 'ok' : 'bad'}">Mic {micLive ? 'live' : 'off'}</span>
      <div class="session-toggle-group">
        <button
          type="button"
          class="session-mode-toggle inline {$statusStore.twitchState === 'connected' ? 'online' : 'offline'}"
          on:click={toggleTwitchMode}
          disabled={twitchModeBusy}
          aria-busy={twitchModeBusy}
          title={$statusStore.twitchState === 'connected' ? 'Switch to local-only mode' : 'Join Twitch chat'}
        >
          <span class="mode-light" aria-hidden="true"></span>
          <span class="mode-copy">
            <strong>{$statusStore.twitchState === 'connected' ? 'Twitch Online' : 'Local Only'}</strong>
          </span>
        </button>
        <button
          type="button"
          class="session-mode-toggle inline small {postBotRepliesToTwitch ? 'online' : 'offline'}"
          on:click={() => setBotTwitchPosting(!postBotRepliesToTwitch)}
          title={postBotRepliesToTwitch ? 'Bot replies can post to Twitch chat' : 'Bot replies stay local in the app'}
        >
          <span class="mode-light" aria-hidden="true"></span>
          <span class="mode-copy">
            <strong>{postBotRepliesToTwitch ? 'Bot Twitch' : 'Bot Local'}</strong>
          </span>
        </button>
      </div>
    </div>
  </div>

  <div class="session-main-grid">
    <div class="session-feed-column">
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
        <button
          type="button"
          class="btn composer-action-btn mic-icon {micLive ? 'live' : 'off'}"
          on:click={toggleMicInline}
          disabled={micBusy}
          aria-busy={micBusy}
          title={micLive ? 'Turn mic off' : 'Turn mic on'}
          aria-label={micLive ? 'Turn mic off' : 'Turn mic on'}
        >
          <Icon name="mic" />
          <span class="label">{micLive ? 'Mic Off' : 'Mic On'}</span>
        </button>
        <button
          type="button"
          class="btn composer-action-btn avatar-icon"
          on:click={openAvatarQuick}
          title="Toggle embedded avatar"
          aria-label="Toggle embedded avatar"
        >
          <Icon name="avatar" />
          <span class="label">{showAvatarInline ? 'Hide Avatar' : 'Avatar'}</span>
        </button>
      </div>
    </div>

    <aside class="cohost-controls-panel">
      <div class="cohost-runtime-card">
        <small class="muted session-meta">
          <span class="light {$statusStore.twitchState === 'connected' ? 'on' : 'off'}" aria-hidden="true"></span>
          State: {$statusStore.twitchState} | Channel: {$statusStore.channel || 'not set'} | {micStatus}{sttFixing ? ' Auto-fixing STT…' : ''}
        </small>
        <small class="muted session-meta">{controlStatus} Pace {cohostPace.toFixed(1)}/min.</small>
        <small class="muted session-meta">Conversation phase: {conversationPhase === 'idle' ? 'idle' : conversationPhase === 'starting' ? 'starting' : conversationPhase === 'listening' ? 'capturing' : conversationPhase === 'processing' ? 'thinking' : conversationPhase === 'replying' ? 'replying' : 'error'}.</small>
        <small class="muted session-meta">Session engine: {sessionState.engine}. First interim: {sessionState.firstInterimLatencyMs ?? '-'} ms. Final: {sessionState.finalLatencyMs ?? '-'} ms. AI: {sessionState.aiLatencyMs ?? '-'} ms.</small>
        <div class="transcript-status {conversationPhase === 'listening' ? 'recording' : ''}">
          <span class="transcript-light" aria-hidden="true"></span>
          <small class="muted session-meta">
            Live transcript: {liveTranscriptDraft || (conversationPhase === 'processing' ? 'Speech captured. Sending to AI...' : 'Waiting for speech...')}
          </small>
        </div>
      </div>
      <div class="cohost-controls-table">
        <div class="cohost-control-row">
          <div class="cohost-control-label">
            <strong>Reply mode</strong>
            <small>{modeMeta[selectedModelMode].detail}</small>
          </div>
          <div class="cohost-control-value">
            <div class="mode-button-group">
              {#each modeOrder as mode}
                <Button.Root
                  class="p-btn btn mode-btn {selectedModelMode === mode ? 'active' : 'ghost'}"
                  on:click={() => setMode(mode)}
                >
                  {modeMeta[mode].label}
                </Button.Root>
              {/each}
            </div>
          </div>
        </div>

        <div class="cohost-control-row compact">
          <div class="cohost-control-label">
            <strong>Auto comments</strong>
            <small>{autonomousReplies ? 'On' : 'Off'}</small>
          </div>
          <div class="cohost-control-value">
            <div class="mode-button-group two-way">
              <Button.Root class="p-btn btn mode-btn {autonomousReplies ? 'ghost' : 'active'}" on:click={() => setAutonomousReplies(false)}>
                Off
              </Button.Root>
              <Button.Root class="p-btn btn mode-btn {autonomousReplies ? 'active' : 'ghost'}" on:click={() => setAutonomousReplies(true)}>
                On
              </Button.Root>
            </div>
          </div>
        </div>

        <div class="cohost-control-row compact">
          <div class="cohost-control-label">
            <strong>Keep talking</strong>
            <small>{topicContinuationMode ? 'Stay on the current topic' : 'Normal reply flow'}</small>
          </div>
          <div class="cohost-control-value">
            <div class="mode-button-group two-way">
              <Button.Root class="p-btn btn mode-btn {topicContinuationMode ? 'ghost' : 'active'}" on:click={() => setTopicContinuationMode(false)}>
                Off
              </Button.Root>
              <Button.Root class="p-btn btn mode-btn {topicContinuationMode ? 'active' : 'ghost'}" on:click={() => setTopicContinuationMode(true)}>
                On
              </Button.Root>
            </div>
          </div>
        </div>

        <div class="cohost-control-row">
          <div class="cohost-control-label">
            <strong>Cohost pace</strong>
            <small>{cohostPace.toFixed(1)} / min</small>
          </div>
          <div class="cohost-control-value">
            <div class="pace-control">
              <UiSlider bind:value={cohostPace} min={0} max={4} step={0.1} ariaLabel="Cohost comment speed" />
            </div>
          </div>
        </div>
      </div>
    </aside>
  </div>

  {#if showAvatarInline}
    <section class="card-lite">
      <AvatarRuntime embedded={true} />
    </section>
  {/if}
</section>
