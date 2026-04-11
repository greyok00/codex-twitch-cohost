<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { listen } from '@tauri-apps/api/event';
  import { clearAuthSessions, connectChat, connectTwitch, disconnectChat, getTwitchOauthSettings, loadAuthSessions, loadStatus, openIsolatedTwitchWindow, setTwitchOauthSettings } from '../api/tauri';
  import { authSessionsStore, errorBannerStore, statusStore } from '../stores/app';

  let clientId = '';
  let botUsername = '';
  let channel = '';
  let redirectUrl = 'http://127.0.0.1:37219/callback';
  const botAuthProfile = 'bot-default';
  const streamerAuthProfile = 'streamer-default';
  let showAdvanced = false;
  let showTools = false;
  let autoJoinInFlight = false;
  let autoJoinRetryScheduled = false;
  $: canJoin = $authSessionsStore.botTokenPresent && $authSessionsStore.streamerTokenPresent;
  $: nextStep = !$authSessionsStore.botTokenPresent
    ? 'connect-bot'
    : !$authSessionsStore.streamerTokenPresent
      ? 'connect-streamer'
      : $statusStore.twitchState !== 'connected'
        ? 'connect-chat'
        : 'ready';

  async function loadSavedOAuthSettings() {
    const saved = await getTwitchOauthSettings();
    clientId = saved.clientId || '';
    botUsername = saved.botUsername || '';
    channel = saved.channel || '';
    redirectUrl = saved.redirectUrl || redirectUrl;
  }

  onMount(() => {
    let cleanup = () => {};

    void (async () => {
      try {
        await loadSavedOAuthSettings();
        await loadAuthSessions();
        await loadStatus();
        void maybeAutoJoin();
      } catch {
        // Keep defaults if settings are unavailable during first launch.
      }

      cleanup = await listen<{
        botUsername: string;
        channel: string;
        broadcasterLogin?: string | null;
      }>('oauth_profile_updated', (event) => {
        botUsername = event.payload.botUsername || botUsername;
        channel = event.payload.channel || channel;
      });
    })();

    return () => {
      cleanup();
    };
  });

  async function onSave() {
    try {
      await setTwitchOauthSettings({
        clientId,
        botUsername: null,
        channel: null,
        redirectUrl: redirectUrl.trim() ? redirectUrl : null
      });
      await loadSavedOAuthSettings();
      await loadAuthSessions();
    } catch (error) {
      errorBannerStore.set('Saving Twitch OAuth settings failed: ' + String(error));
    }
  }

  async function onLogin() {
    try {
      await connectTwitch(false, botAuthProfile, 'bot');
      setTimeout(() => {
        void loadSavedOAuthSettings();
        void loadAuthSessions();
        void maybeAutoJoin();
      }, 1200);
    } catch (error) {
      errorBannerStore.set('OAuth launch failed: ' + String(error));
    }
  }

  async function onSwitchBot() {
    try {
      await connectTwitch(true, botAuthProfile, 'bot');
      setTimeout(() => {
        void loadSavedOAuthSettings();
        void loadAuthSessions();
      }, 1200);
    } catch (error) {
      errorBannerStore.set('OAuth launch failed: ' + String(error));
    }
  }

  async function onConnectStreamer() {
    try {
      if (!$authSessionsStore.botTokenPresent) {
        errorBannerStore.set('Connect Bot first, then connect Streamer.');
        return;
      }
      await connectTwitch(false, streamerAuthProfile, 'streamer');
      setTimeout(() => {
        void loadSavedOAuthSettings();
        void loadAuthSessions();
        void maybeAutoJoin();
      }, 1200);
    } catch (error) {
      errorBannerStore.set('Streamer OAuth failed: ' + String(error));
    }
  }

  async function openBotLoginWindow() {
    try {
      await openIsolatedTwitchWindow(botAuthProfile, 'https://www.twitch.tv/login');
    } catch (error) {
      errorBannerStore.set('Failed to open Twitch login: ' + String(error));
    }
  }

  async function onResetAuth() {
    try {
      await clearAuthSessions();
      await loadSavedOAuthSettings();
      await loadAuthSessions();
      botUsername = '';
      channel = '';
    } catch (error) {
      errorBannerStore.set('Reset auth sessions failed: ' + String(error));
    }
  }

  async function joinNow() {
    try {
      if (!canJoin) {
        errorBannerStore.set('Activation order: Connect Bot -> Connect Streamer -> Connect Chat.');
        return;
      }
      await connectChat();
    } catch (error) {
      errorBannerStore.set('Join chat failed: ' + String(error));
    }
  }

  async function leaveNow() {
    try {
      await disconnectChat();
    } catch (error) {
      errorBannerStore.set('Leave chat failed: ' + String(error));
    }
  }

  async function maybeAutoJoin() {
    if (autoJoinInFlight) return;
    const sessions = get(authSessionsStore);
    const status = get(statusStore);
    if (!sessions.botTokenPresent || !sessions.streamerTokenPresent || status.twitchState === 'connected') {
      return;
    }
    autoJoinInFlight = true;
    try {
      await joinNow();
    } finally {
      autoJoinInFlight = false;
      const latest = get(statusStore);
      if (latest.twitchState !== 'connected' && !autoJoinRetryScheduled) {
        autoJoinRetryScheduled = true;
        window.setTimeout(() => {
          autoJoinRetryScheduled = false;
          void maybeAutoJoin();
        }, 1500);
      }
    }
  }

  async function activateNext() {
    if (nextStep === 'connect-bot') {
      await onLogin();
      return;
    }
    if (nextStep === 'connect-streamer') {
      await onConnectStreamer();
      return;
    }
    if (nextStep === 'connect-chat') {
      await joinNow();
    }
  }

  $: if (canJoin && $statusStore.twitchState !== 'connected') {
    void maybeAutoJoin();
  }
</script>

<section class="card">
  <h3>🔐 Twitch Login</h3>
  <p class="muted">Activation order is enforced: 1) connect bot, 2) connect streamer, 3) connect chat.</p>

  {#if nextStep !== 'ready'}
    <div class="next-step">
      <small class="muted">Next required step:
        {#if nextStep === 'connect-bot'} Connect Bot{/if}
        {#if nextStep === 'connect-streamer'} Connect Streamer{/if}
        {#if nextStep === 'connect-chat'} Connect Chat{/if}
      </small>
      <button class="btn" on:click={activateNext}>
        {#if nextStep === 'connect-bot'}Activate: Bot{/if}
        {#if nextStep === 'connect-streamer'}Activate: Streamer{/if}
        {#if nextStep === 'connect-chat'}Activate: Chat{/if}
      </button>
    </div>
  {/if}

  {#if botUsername || channel}
    <p class="muted">Signed-in bot: {botUsername || 'unknown'} | Target channel: {channel || 'unknown'}</p>
  {/if}
  <small class="muted">
    Bot: {$authSessionsStore.botTokenPresent ? 'connected' : 'not connected'} ({$authSessionsStore.botUsername || 'unknown'})
    |
    Streamer: {$authSessionsStore.streamerTokenPresent ? 'connected' : 'not connected'} ({$authSessionsStore.broadcasterLogin || 'not set'})
  </small>
  <div class="actions primary-actions">
    <button class="btn" on:click={onLogin}>Connect Bot</button>
    <button class="btn" on:click={onConnectStreamer} disabled={!$authSessionsStore.botTokenPresent}>Connect Streamer</button>
    <button class="btn" on:click={joinNow} disabled={!canJoin || $statusStore.twitchState === 'connected'}>Connect Chat</button>
    <button class="btn" on:click={leaveNow}>Disconnect Chat</button>
  </div>

  <button class="btn tertiary-toggle" on:click={() => (showTools = !showTools)}>
    {showTools ? 'Hide' : 'Show'} Extra Tools ▾
  </button>

  {#if showTools}
    <div class="actions">
      <button class="btn" on:click={openBotLoginWindow}>🔐 Open Bot Login Window</button>
      <button class="btn" on:click={onSwitchBot}>🔄 Switch Bot Account</button>
      <button class="btn" on:click={onResetAuth}>🧼 Reset Auth Sessions</button>
    </div>
  {/if}

  <button class="btn tertiary-toggle" on:click={() => (showAdvanced = !showAdvanced)}>
    {showAdvanced ? 'Hide' : 'Show'} Advanced OAuth Setup ▾
  </button>

  {#if showAdvanced}
    <input bind:value={clientId} placeholder="Twitch client ID" />
    <input bind:value={redirectUrl} placeholder="Redirect URL" />
    <button class="btn" on:click={onSave}>Save OAuth Settings</button>
  {/if}
</section>

<style>
  .actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .primary-actions .btn {
    min-width: 140px;
  }
  .tertiary-toggle {
    justify-content: flex-start;
    padding-inline: 0.7rem;
    opacity: 0.95;
  }
  .next-step {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    flex-wrap: wrap;
  }
</style>
