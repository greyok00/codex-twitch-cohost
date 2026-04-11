<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { listen } from '@tauri-apps/api/event';
  import { clearAuthSessions, connectChat, connectTwitch, disconnectChat, getTwitchOauthSettings, loadAuthSessions, loadStatus, openExternal, openIsolatedTwitchWindow, setTwitchOauthSettings } from '../api/tauri';
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
  let oauthCode: { userCode: string; verificationUri: string; verificationUrl: string; role: string } | null = null;
  $: canJoin = $authSessionsStore.botTokenPresent && $authSessionsStore.streamerTokenPresent;
  $: oauthConfigured = Boolean(clientId && clientId.trim() && clientId !== 'your_twitch_client_id' && clientId !== 'replace_client_id');
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
    showAdvanced = !oauthConfigured;
  }

  onMount(() => {
    const cleanups: Array<() => void> = [];

    void (async () => {
      try {
        await loadSavedOAuthSettings();
        await loadAuthSessions();
        await loadStatus();
        void maybeAutoJoin();
      } catch {
        // Keep defaults if settings are unavailable during first launch.
      }

      const unlistenProfile = await listen<{
        botUsername: string;
        channel: string;
        broadcasterLogin?: string | null;
      }>('oauth_profile_updated', (event) => {
        botUsername = event.payload.botUsername || botUsername;
        channel = event.payload.channel || channel;
        oauthCode = null;
      });
      cleanups.push(unlistenProfile);

      const unlistenCode = await listen<{
        userCode: string;
        verificationUri: string;
        verificationUrl: string;
        role: string;
      }>('oauth_device_code', (event) => {
        oauthCode = event.payload;
      });
      cleanups.push(unlistenCode);
    })();

    return () => {
      for (const cleanup of cleanups) cleanup();
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

  async function openTwitchDevAppSetup() {
    try {
      await openExternal('https://dev.twitch.tv/console/apps/create');
    } catch (error) {
      errorBannerStore.set('Failed to open Twitch Developer Console: ' + String(error));
    }
  }

  async function onLogin() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        await openTwitchDevAppSetup();
        errorBannerStore.set('OAuth setup required first: log in at Twitch Developer Console, create an app, copy Client ID, save it here, then connect bot.');
        return;
      }
      // Reuse saved session by default; backend only opens auth flow when needed.
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

  async function onSwitchStreamer() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        errorBannerStore.set('Set Twitch client ID first in OAuth Setup.');
        return;
      }
      if (!$authSessionsStore.botTokenPresent) {
        errorBannerStore.set('Connect Bot first, then switch Streamer account.');
        return;
      }
      await connectTwitch(true, streamerAuthProfile, 'streamer');
      setTimeout(() => {
        void loadSavedOAuthSettings();
        void loadAuthSessions();
      }, 1200);
    } catch (error) {
      errorBannerStore.set('Streamer switch failed: ' + String(error));
    }
  }

  async function onConnectStreamer() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        await openTwitchDevAppSetup();
        errorBannerStore.set('OAuth setup required first: create app in Twitch Developer Console, save Client ID, then connect streamer.');
        return;
      }
      if (!$authSessionsStore.botTokenPresent) {
        errorBannerStore.set('Connect Bot first, then connect Streamer.');
        return;
      }
      // Reuse saved streamer session by default.
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
      oauthCode = null;
    } catch (error) {
      errorBannerStore.set('Reset auth sessions failed: ' + String(error));
    }
  }

  async function joinNow() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        errorBannerStore.set('Set Twitch client ID first in OAuth Setup.');
        return;
      }
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
  <h3 class="title-row">
    🔐 Twitch Login
    <span class="oauth-status">
      <span class="status-dot {oauthConfigured ? 'ok' : 'bad'}"></span>
      {oauthConfigured ? 'OAuth configured' : 'OAuth not configured'}
    </span>
  </h3>
  <p class="muted">Simple flow: Save Twitch Client ID once, then connect Bot, connect Streamer, then connect Chat.</p>

  {#if !oauthConfigured}
    <div class="oauth-required">
      <small class="muted">One-time setup: Create a Twitch app, copy its Client ID, then save it here. Use redirect URL <code>http://127.0.0.1:37219/callback</code>.</small>
      <button class="btn" on:click={openTwitchDevAppSetup}>Open Twitch App Setup</button>
      <input bind:value={clientId} placeholder="Twitch client ID (required)" />
      <input bind:value={redirectUrl} placeholder="Redirect URL" />
      <button class="btn" on:click={onSave}>Save OAuth Settings</button>
    </div>
  {/if}

  {#if nextStep !== 'ready'}
    <div class="next-step">
      <small class="muted">Next step:
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

  {#if oauthCode}
    <div class="oauth-code-card">
      <small class="muted">Authorize {oauthCode.role === 'streamer' ? 'Streamer' : 'Bot'} account with this code:</small>
      <div class="oauth-code-value">{oauthCode.userCode}</div>
      <div class="actions">
        <button class="btn" on:click={() => openExternal(oauthCode.verificationUrl)}>Open Twitch Activation Page</button>
        <button class="btn" on:click={() => navigator.clipboard?.writeText(oauthCode.userCode)}>Copy Code</button>
        <button class="btn" on:click={() => (oauthCode = null)}>Dismiss</button>
      </div>
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
    <button class="btn" on:click={onLogin} disabled={!oauthConfigured}>1) Connect Bot</button>
    <button class="btn" on:click={onConnectStreamer} disabled={!oauthConfigured || !$authSessionsStore.botTokenPresent}>2) Connect Streamer</button>
    <button class="btn" on:click={joinNow} disabled={!oauthConfigured || !canJoin || $statusStore.twitchState === 'connected'}>3) Connect Chat</button>
    <button class="btn" on:click={leaveNow}>Disconnect Chat</button>
  </div>

  <button class="btn tertiary-toggle" on:click={() => (showTools = !showTools)}>
    {showTools ? 'Hide' : 'Show'} Extra Tools ▾
  </button>

  {#if showTools}
    <div class="actions">
      <button class="btn" on:click={openBotLoginWindow}>🔐 Open Twitch Login Page</button>
      <button class="btn" on:click={onSwitchBot}>🔄 Switch Bot Account</button>
      <button class="btn" on:click={onSwitchStreamer}>🔄 Switch Streamer Account</button>
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
  .oauth-required {
    display: grid;
    gap: 0.45rem;
    margin-bottom: 0.75rem;
    padding: 0.6rem;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: color-mix(in srgb, var(--panel), white 3%);
  }
  .oauth-code-card {
    display: grid;
    gap: 0.45rem;
    margin-bottom: 0.75rem;
    padding: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--line), #22c55e 38%);
    border-radius: 10px;
    background: color-mix(in srgb, var(--panel), #22c55e 6%);
  }
  .oauth-code-value {
    font-weight: 800;
    font-size: 1.15rem;
    letter-spacing: 0.1em;
    color: var(--text);
  }
  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.7rem;
    flex-wrap: wrap;
  }
  .oauth-status {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.82rem;
    color: var(--muted);
  }
  .status-dot {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 999px;
    display: inline-block;
  }
  .status-dot.ok {
    background: #22c55e;
    box-shadow: 0 0 0.45rem rgba(34, 197, 94, 0.55);
  }
  .status-dot.bad {
    background: #ef4444;
    box-shadow: 0 0 0.45rem rgba(239, 68, 68, 0.5);
  }
</style>
