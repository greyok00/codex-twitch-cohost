<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { getSttConfig, loadAuthSessions, loadPersonality, loadStatus, registerEventListeners } from '../api/tauri';
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
  import WelcomeScreen from './WelcomeScreen.svelte';
  let sttReady = false;
  let sttTimer: number | null = null;
  let showWelcome = false;

  $: authReady = $authSessionsStore.botTokenPresent && $authSessionsStore.streamerTokenPresent;
  $: aiReady = $diagnosticsStore.providerState === 'connected';
  $: chatReady = $statusStore.twitchState === 'connected';
  $: voiceReady = !!$statusStore.voiceEnabled;
  $: settingsConfigured = authReady && aiReady && chatReady && sttReady;

  function summaryDotClass(state: string) {
    if (state === 'connected' || state === 'pass') return 'green';
    if (state === 'connecting' || state === 'warn') return 'amber';
    return 'red';
  }

  onMount(async () => {
    showWelcome = localStorage.getItem('cohost.onboarded.v1') !== 'true';
    document.documentElement.classList.add('dark');
    try {
      await registerEventListeners();
      await loadStatus();
      await loadAuthSessions();
      await loadPersonality();
      void refreshSttReady();
      sttTimer = window.setInterval(() => void refreshSttReady(), 3500);
    } catch (error) {
      errorBannerStore.set('Startup sync failed: ' + String(error));
    }
  });

  onDestroy(() => {
    if (sttTimer !== null) {
      window.clearInterval(sttTimer);
    }
  });

  async function refreshSttReady() {
    try {
      const cfg = await getSttConfig();
      sttReady = !!(cfg.sttBinaryPath && cfg.sttModelPath);
    } catch {
      sttReady = false;
    }
  }

  function closeWelcome() {
    showWelcome = false;
  }
</script>

<main>
  <section class="brand-strip card">
    <img src="/logo.png" alt="App logo" class="brand-logo" />
    <div class="brand-copy">
      <strong>GreyOK_'s Twitch Co-Host Bot</strong>
      <small class="muted">Cloud-first automation, voice, and avatar cohost controls</small>
    </div>
  </section>

  {#if $errorBannerStore}
    <section class="card error">{$errorBannerStore}</section>
  {/if}

  <section class="top">
    <SessionChatPanel />
  </section>

  <section class="sections">
    <details class="card fold" open={!settingsConfigured}>
      <summary><span class="sum">⚙️ Settings</span></summary>
      <div class="content one settings-group">
        <details class="card fold nested" open={!authReady}>
          <summary><span class="sum"><span class="dot {summaryDotClass(authReady ? 'connected' : 'error')}"></span>🔐 Auth & Channel</span></summary>
          <div class="content one">
            <TwitchLoginCard />
          </div>
        </details>

        <details class="card fold nested" open={!aiReady}>
          <summary><span class="sum"><span class="dot {summaryDotClass(aiReady ? 'connected' : 'error')}"></span>☁️ AI Setup & Personality</span></summary>
          <div class="content two">
            <CloudSetupCard />
            <PersonalityEditor />
          </div>
        </details>

        <details class="card fold nested" open={!aiReady || !chatReady || !sttReady}>
          <summary><span class="sum"><span class="dot {summaryDotClass(sttReady ? 'connected' : 'error')}"></span>🎤 Voice Input</span></summary>
          <div class="content one">
            <VoicePanel aiReady={aiReady} chatReady={chatReady} />
          </div>
        </details>

        <details class="card fold nested" open={!aiReady || !chatReady || !voiceReady}>
          <summary><span class="sum"><span class="dot {summaryDotClass(aiReady && chatReady && voiceReady ? 'connected' : 'error')}"></span>🧍 Avatar Popup</span></summary>
          <div class="content one">
            <AvatarPanel aiReady={aiReady} chatReady={chatReady} voiceReady={voiceReady} />
          </div>
        </details>

        <details class="card fold nested">
          <summary>🧾 Memory</summary>
          <div class="content one">
            <MemoryPanel />
          </div>
        </details>
      </div>
    </details>

    <details class="card fold">
      <summary>🩺 Diagnostics</summary>
      <div class="content one">
        <DiagnosticsPanel />
        <section class="self-test-wrap">
          <h3>✅ Self-Test</h3>
          <SelfTestPanel />
        </section>
      </div>
    </details>

    <details class="card fold">
      <summary>ℹ️ About</summary>
      <div class="content one about-content">
        <h3>🛠️ Built By GreyOK_</h3>
        <p class="about-text">Cross-platform Twitch co-host desktop app focused on low-latency chat automation, voice, and stream utility workflows.</p>
        <p class="about-text muted">Stack: Tauri + Rust backend, Svelte frontend, Vite, Skeleton UI styling.</p>
        <div class="social-links">
          <a href="https://twitch.tv/GreyOK__" target="_blank" rel="noreferrer" class="social-link">
            <span class="icon" aria-hidden="true">🟣</span>
            <span>twitch.tv/GreyOK__</span>
          </a>
          <a href="https://youtube.com/@GreyOK_0" target="_blank" rel="noreferrer" class="social-link">
            <span class="icon" aria-hidden="true">🔴</span>
            <span>youtube.com/@GreyOK_0</span>
          </a>
        </div>
      </div>
    </details>
  </section>

  {#if showWelcome}
    <WelcomeScreen on:complete={closeWelcome} />
  {/if}
</main>

<style>
  main {
    padding: 0.6rem;
    display: grid;
    gap: 0.5rem;
    max-width: 1480px;
    margin: 0 auto;
    min-height: 100%;
    align-content: start;
  }
  .top {
    min-height: 0;
  }
  .brand-strip {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 0.5rem 0.65rem;
  }
  .brand-logo {
    width: 48px;
    height: 48px;
    object-fit: contain;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.08);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.14) inset;
  }
  .brand-copy {
    display: grid;
    gap: 0.12rem;
    line-height: 1.2;
  }
  .sections {
    display: grid;
    gap: 0.55rem;
  }
  .fold {
    padding: 0.5rem 0.66rem;
    transition: transform 160ms ease, box-shadow 160ms ease;
  }
  .fold:hover {
    transform: translateY(-1px);
  }
  .fold :global(section.card) {
    border: 0;
    padding: 0.45rem 0 0;
    background: transparent;
  }
  .content {
    padding-top: 0.45rem;
  }
  .settings-group {
    display: grid;
    gap: 0.45rem;
  }
  .nested {
    background: color-mix(in srgb, var(--panel-strong) 80%, transparent);
  }
  .content.two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.45rem;
  }
  summary {
    cursor: pointer;
    font-weight: 700;
    font-size: 1.1rem;
    user-select: none;
    list-style: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  summary::after {
    content: "▾";
    color: var(--muted);
    font-size: 1.02rem;
    transition: transform 140ms ease;
  }
  .sum {
    display: inline-flex;
    align-items: center;
    gap: 0.42rem;
  }
  .dot {
    width: 1.08rem;
    height: 1.08rem;
    border: 1px solid rgba(0, 0, 0, 0.36);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.12) inset;
  }
  .dot.green {
    background: #2bd35f;
  }
  .dot.amber {
    background: #d8a34a;
  }
  .dot.red {
    background: #d74646;
  }
  details[open] summary::after {
    transform: rotate(180deg);
    color: var(--accent);
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .error {
    border-color: color-mix(in srgb, var(--danger), #000 30%);
    color: #ffd4d4;
  }
  .about-content {
    display: grid;
    gap: 0.5rem;
  }
  .self-test-wrap {
    display: grid;
    gap: 0.45rem;
    margin-top: 0.55rem;
  }
  .self-test-wrap h3 {
    margin: 0;
  }
  .about-content h3 {
    margin: 0;
    font-size: 1.24rem;
  }
  .about-text {
    margin: 0;
    font-size: 1.02rem;
    line-height: 1.42;
  }
  .social-links {
    display: grid;
    gap: 0.45rem;
  }
  .social-link {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    font-size: 1.04rem;
    font-weight: 700;
    width: fit-content;
    padding: 0.35rem 0.5rem;
    border-radius: 10px;
    background: color-mix(in srgb, var(--panel-strong) 86%, transparent);
    border: 1px solid color-mix(in srgb, var(--border) 82%, transparent);
  }
  .icon {
    font-size: 1.1rem;
    line-height: 1;
  }
  .about-content a {
    color: var(--accent);
    text-decoration: none;
  }
  .about-content a:hover {
    text-decoration: underline;
  }
  @media (max-width: 1100px) {
    .content.two {
      grid-template-columns: 1fr;
    }
  }
</style>
