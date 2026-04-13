<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { autoConfigureSttFast, getSttConfig, loadAuthSessions, loadPersonality, loadStatus, openExternal, registerEventListeners } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';
  import { authSessionsStore, diagnosticsStore, statusStore } from '../stores/app';
  import TwitchLoginCard from './TwitchLoginCard.svelte';
  import SessionChatPanel from './SessionChatPanel.svelte';
  import CloudSetupCard from './CloudSetupCard.svelte';
  import PersonalityEditor from './PersonalityEditor.svelte';
  import MemoryPanel from './MemoryPanel.svelte';
  import VoicePanel from './VoicePanel.svelte';
  import AvatarPanel from './AvatarPanel.svelte';

  type PaneId = 'auth' | 'cloud' | 'personality' | 'voice' | 'avatar' | 'memory' | 'about';

  let sttReady = false;
  let sttTimer: number | null = null;
  let sttAutoTried = false;
  let activePane: PaneId = 'auth';

  $: authReady = $authSessionsStore.botTokenPresent && $authSessionsStore.streamerTokenPresent;
  $: aiReady = $diagnosticsStore.providerState === 'connected';
  $: chatReady = $statusStore.twitchState === 'connected';

  $: alertAuth = !authReady;
  $: alertAi = !aiReady;

  onMount(async () => {
    try {
      document.documentElement.setAttribute('data-theme', 'dark');
      await registerEventListeners();
      await loadStatus();
      await loadAuthSessions();
      await loadPersonality();
      await maybeAutoConfigureStt();
      void refreshSttReady();
      sttTimer = window.setInterval(() => void refreshSttReady(), 3500);
    } catch (error) {
      errorBannerStore.set('Startup sync failed: ' + String(error));
    }
  });

  onDestroy(() => {
    if (sttTimer !== null) window.clearInterval(sttTimer);
  });

  async function refreshSttReady() {
    try {
      const cfg = await getSttConfig();
      sttReady = !!(cfg.sttEnabled && cfg.sttBinaryPath && cfg.sttModelPath);
    } catch {
      sttReady = false;
    }
  }

  async function maybeAutoConfigureStt() {
    if (sttAutoTried) return;
    sttAutoTried = true;
    try {
      const cfg = await getSttConfig();
      const ready = !!(cfg.sttEnabled && cfg.sttBinaryPath && cfg.sttModelPath);
      if (ready) {
        sttReady = true;
        return;
      }
      await autoConfigureSttFast();
      await refreshSttReady();
    } catch {
      // no-op
    }
  }

  const socialLinks = [
    { label: 'GitHub', handle: '@greyok00', url: 'https://github.com/greyok00', icon: '/social/github.svg' },
    { label: 'Twitch', handle: '@greyok__', url: 'https://twitch.tv/greyok__', icon: '/social/twitch.svg' },
    { label: 'YouTube', handle: '@GreyOK_0', url: 'https://www.youtube.com/@GreyOK_0', icon: '/social/youtube.svg' },
    { label: 'Discord', handle: "GreyOK_'s Discord", url: 'https://discord.gg/TJcr6ZxJ', icon: '/social/discord.svg' }
  ] as const;

  async function openLink(url: string) {
    try {
      await openExternal(url);
    } catch {
      // no-op
    }
  }
</script>

<main class="app-shell">
  <section class="chat-wrap">
    <div class="session-surface">
      <SessionChatPanel />
    </div>
  </section>

  <section class="layout">
    <aside class="sidebar">
      <h3>Control Center</h3>
      <div class="tab-list">
        <button type="button" class="tab-trigger {activePane === 'auth' ? 'active' : ''} {alertAuth ? 'alert' : ''}" on:click={() => (activePane = 'auth')}>Auth & Channel</button>
        <button type="button" class="tab-trigger {activePane === 'cloud' ? 'active' : ''} {alertAi ? 'alert' : ''}" on:click={() => (activePane = 'cloud')}>Cloud AI</button>
        <button type="button" class="tab-trigger {activePane === 'personality' ? 'active' : ''}" on:click={() => (activePane = 'personality')}>Personality</button>
        <button type="button" class="tab-trigger {activePane === 'voice' ? 'active' : ''}" on:click={() => (activePane = 'voice')}>Settings</button>
        <button type="button" class="tab-trigger {activePane === 'avatar' ? 'active' : ''}" on:click={() => (activePane = 'avatar')}>Avatar</button>
        <button type="button" class="tab-trigger {activePane === 'memory' ? 'active' : ''}" on:click={() => (activePane = 'memory')}>Memory</button>
        <button type="button" class="tab-trigger {activePane === 'about' ? 'active' : ''}" on:click={() => (activePane = 'about')}>About</button>
      </div>

      <div class="status-list">
        <small>Bot: {authReady ? 'ready' : 'missing'}</small>
        <small>AI: {aiReady ? 'online' : 'offline'}</small>
        <small>Chat: {chatReady ? 'joined' : 'not joined'}</small>
        <small>STT: {sttReady ? 'ready' : 'missing'}</small>
      </div>
    </aside>

    <section class="content">
      {#if activePane === 'auth'}
        <TwitchLoginCard />
      {:else if activePane === 'cloud'}
        <CloudSetupCard />
      {:else if activePane === 'personality'}
        <PersonalityEditor />
      {:else if activePane === 'voice'}
        <VoicePanel />
      {:else if activePane === 'avatar'}
        <AvatarPanel />
      {:else if activePane === 'memory'}
        <MemoryPanel />
      {:else}
        <section class="card about-card">
          <div class="about-head">
            <img src="/top-logo.png" alt="GreyOK" class="about-logo" />
            <div>
              <h3>About GreyOK Co-Host</h3>
              <p class="muted">Cross-platform Twitch co-host app built with Tauri + Svelte.</p>
            </div>
          </div>
          <div class="social-list">
            {#each socialLinks as link}
              <button type="button" class="social-link" on:click={() => openLink(link.url)}>
                <img src={link.icon} alt={link.label + ' logo'} class="social-icon" />
                <span class="social-meta">
                  <strong>{link.label}</strong>
                  <small>{link.handle}</small>
                </span>
              </button>
            {/each}
          </div>
        </section>
      {/if}
    </section>
  </section>
</main>
