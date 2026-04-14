<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { Button } from 'bits-ui';
  import { getPublicCallSettings, loadPersonality, registerEventListeners } from '../api/tauri';
  import { botLogStore, chatStore, errorBannerStore, personalityStore } from '../stores/app';
  import { VoiceSessionController } from '../voice-session/VoiceSessionController';
  import { voiceSessionStore } from '../voice-session/store';
  import Icon from './ui/Icon.svelte';

  let valid = false;
  let loading = true;
  let controller: VoiceSessionController | null = null;
  let callerName = 'guest';
  let callToken = '';
  let feed: Array<{ id: string; user: string; content: string; kind: 'chat' | 'bot' }> = [];

  $: session = $voiceSessionStore;
  $: phaseLabel = session.status === 'idle'
    ? 'idle'
    : session.status === 'starting'
      ? 'starting'
      : session.status === 'listening'
        ? 'listening'
        : session.status === 'processing'
          ? 'thinking'
          : session.status === 'replying'
            ? 'replying'
            : 'error';

  const unsubChat = chatStore.subscribe((items) => {
    feed = [
      ...items.slice(0, 8).map((item) => ({ id: item.id, user: item.user, content: item.content, kind: 'chat' as const })),
      ...get(botLogStore).slice(0, 8).map((item) => ({ id: `bot-${item.id}`, user: item.user, content: item.content, kind: 'bot' as const }))
    ]
      .sort((a, b) => a.id.localeCompare(b.id))
      .slice(-12);
  });

  const unsubBot = botLogStore.subscribe((items) => {
    feed = [
      ...get(chatStore).slice(0, 8).map((item) => ({ id: item.id, user: item.user, content: item.content, kind: 'chat' as const })),
      ...items.slice(0, 8).map((item) => ({ id: `bot-${item.id}`, user: item.user, content: item.content, kind: 'bot' as const }))
    ]
      .sort((a, b) => a.id.localeCompare(b.id))
      .slice(-12);
  });

  onMount(async () => {
    try {
      await registerEventListeners();
      await loadPersonality();
      const params = new URLSearchParams(window.location.search);
      callToken = params.get('call') || '';
      const settings = await getPublicCallSettings();
      valid = settings.enabled && !!callToken && callToken === settings.token;
      loading = false;
    } catch (error) {
      loading = false;
      errorBannerStore.set('Public call page failed to initialize: ' + String(error));
    }
  });

  onDestroy(() => {
    unsubChat();
    unsubBot();
    void controller?.stop();
  });

  async function toggleCall() {
    if (controller && session.micEnabled) {
      await controller.stop();
      return;
    }
    controller = new VoiceSessionController({ mode: 'public', callerName });
    try {
      await controller.start();
    } catch (error) {
      errorBannerStore.set('Public call start failed: ' + String(error));
    }
  }
</script>

<main class="public-call-shell">
  <section class="public-call-card">
    <img src="/floating-head.png" alt="Avatar" class="public-call-avatar" />
    <h1>{$personalityStore.name}</h1>
    <p class="muted">{$personalityStore.lore || $personalityStore.streamer_relationship}</p>

    {#if loading}
      <small class="muted">Loading call page...</small>
    {:else if !valid}
      <small class="muted">This public call link is unavailable or disabled.</small>
    {:else}
      <input bind:value={callerName} maxlength="30" placeholder="Your name" />
      <div class="row">
        <Button.Root class="p-btn btn" on:click={toggleCall}>
          <Icon name="mic" />{session.micEnabled ? 'End Call' : 'Start Call'}
        </Button.Root>
      </div>
      <small class="muted">Phase: {phaseLabel} | Engine: {session.engine}</small>
      <small class="muted">Live transcript: {session.interimText || 'Waiting for speech...'}</small>
      <small class="muted">First interim: {session.firstInterimLatencyMs ?? '-'} ms | Final: {session.finalLatencyMs ?? '-'} ms | AI: {session.aiLatencyMs ?? '-'} ms</small>

      <div class="feed public-feed">
        {#if feed.length === 0}
          <small class="muted">No conversation yet.</small>
        {:else}
          {#each feed as item (item.id)}
            <div class="line {item.kind === 'bot' ? 'bot' : 'viewer'}">
              <span class="tag">{item.kind === 'bot' ? 'Bot' : 'You'}</span>
              <strong>{item.user}</strong>
              <span>{item.content}</span>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  </section>
</main>
