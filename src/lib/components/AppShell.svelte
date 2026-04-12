<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Tabs } from 'bits-ui';
  import { autoConfigureSttFast, getSttConfig, loadAuthSessions, loadPersonality, loadStatus, registerEventListeners } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';
  import { authSessionsStore, diagnosticsStore, statusStore } from '../stores/app';
  import TwitchLoginCard from './TwitchLoginCard.svelte';
  import SessionChatPanel from './SessionChatPanel.svelte';
  import CloudSetupCard from './CloudSetupCard.svelte';
  import PersonalityEditor from './PersonalityEditor.svelte';
  import MemoryPanel from './MemoryPanel.svelte';
  import DiagnosticsPanel from './DiagnosticsPanel.svelte';
  import SelfTestPanel from './SelfTestPanel.svelte';
  import VoicePanel from './VoicePanel.svelte';
  import AvatarPanel from './AvatarPanel.svelte';

  type PaneId = 'auth' | 'ai' | 'voice' | 'memory' | 'diagnostics' | 'about';

  let sttReady = false;
  let sttTimer: number | null = null;
  let sttAutoTried = false;
  let activePane: PaneId = 'auth';

  $: authReady = $authSessionsStore.botTokenPresent && $authSessionsStore.streamerTokenPresent;
  $: aiReady = $diagnosticsStore.providerState === 'connected';
  $: chatReady = $statusStore.twitchState === 'connected';
  $: voiceReady = !!$statusStore.voiceEnabled;

  $: alertAuth = !authReady;
  $: alertAi = !aiReady;
  $: alertVoice = !sttReady;

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

  function onPaneChange(next: string | undefined) {
    if (!next) return;
    activePane = next as PaneId;
  }
</script>

<main class="app-shell">
  <header class="topbar topbar-logo-only">
    <img src="/top-logo.png" alt="App logo" class="top-logo" />
  </header>

  {#if $errorBannerStore}
    <section class="error-banner">{$errorBannerStore}</section>
  {/if}

  <section class="chat-wrap">
    <SessionChatPanel />
  </section>

  <Tabs.Root value={activePane} onValueChange={onPaneChange} orientation="vertical" class="layout">
    <aside class="sidebar">
      <h3>Control Center</h3>
      <Tabs.List class="tab-list">
        <Tabs.Trigger value="auth" class="tab-trigger {alertAuth ? 'alert' : ''}">Auth & Channel</Tabs.Trigger>
        <Tabs.Trigger value="ai" class="tab-trigger {alertAi ? 'alert' : ''}">AI Setup</Tabs.Trigger>
        <Tabs.Trigger value="voice" class="tab-trigger {alertVoice ? 'alert' : ''}">Voice Input</Tabs.Trigger>
        <Tabs.Trigger value="memory" class="tab-trigger">Memory</Tabs.Trigger>
        <Tabs.Trigger value="diagnostics" class="tab-trigger">Diagnostics</Tabs.Trigger>
        <Tabs.Trigger value="about" class="tab-trigger">About</Tabs.Trigger>
      </Tabs.List>

      <div class="status-list">
        <small>Bot: {authReady ? 'ready' : 'missing'}</small>
        <small>AI: {aiReady ? 'online' : 'offline'}</small>
        <small>Chat: {chatReady ? 'joined' : 'not joined'}</small>
        <small>STT: {sttReady ? 'ready' : 'missing'}</small>
      </div>
    </aside>

    <section class="content">
      <Tabs.Content value="auth" class="tab-panel">
        <TwitchLoginCard />
      </Tabs.Content>

      <Tabs.Content value="ai" class="tab-panel">
        <div class="ai-stack">
          <div class="two-col">
            <CloudSetupCard />
            <PersonalityEditor />
          </div>
          <AvatarPanel aiReady={aiReady} chatReady={chatReady} voiceReady={voiceReady} />
        </div>
      </Tabs.Content>

      <Tabs.Content value="voice" class="tab-panel">
        <VoicePanel aiReady={aiReady} chatReady={chatReady} />
      </Tabs.Content>

      <Tabs.Content value="memory" class="tab-panel">
        <MemoryPanel />
      </Tabs.Content>

      <Tabs.Content value="diagnostics" class="tab-panel">
        <DiagnosticsPanel />
        <section class="card-lite">
          <h3>Self Test</h3>
          <SelfTestPanel />
        </section>
      </Tabs.Content>

      <Tabs.Content value="about" class="tab-panel">
        <h3>About</h3>
        <p class="muted">Cross-platform Twitch co-host app built with Tauri + Svelte.</p>
      </Tabs.Content>
    </section>
  </Tabs.Root>
</main>
