<script lang="ts">
  import { Button } from 'bits-ui';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { listen } from '@tauri-apps/api/event';
  import Icon from './ui/Icon.svelte';
  import { clearAuthSessions, clearBotSession, clearStreamerSession, connectChat, connectTwitch, disconnectChat, getTwitchOauthSettings, loadAuthSessions, loadStatus, openExternal, openIsolatedTwitchWindow, setTwitchOauthSettings } from '../api/tauri';
  import { authSessionsStore, eventStore, statusStore } from '../stores/app';

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

  function logUiMessage(content: string) {
    const entry = {
      id: `ui-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      kind: 'ui',
      content,
      timestamp: new Date().toISOString()
    };
    eventStore.update((items) => [entry, ...items].slice(0, 300));
  }

  async function loadSavedOAuthSettings() {
    const saved = await getTwitchOauthSettings();
    clientId = saved.clientId || '';
    botUsername = saved.botUsername || '';
    channel = saved.broadcasterLogin || '';
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
        channel = event.payload.broadcasterLogin || event.payload.channel || '';
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
      logUiMessage('Saving Twitch OAuth settings failed: ' + String(error));
    }
  }

  async function openTwitchDevAppSetup() {
    try {
      await openExternal('https://dev.twitch.tv/console/apps/create');
    } catch (error) {
      logUiMessage('Failed to open Twitch Developer Console: ' + String(error));
    }
  }

  async function onLogin() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        await openTwitchDevAppSetup();
        logUiMessage('OAuth setup required first: log in at Twitch Developer Console, create an app, copy Client ID, save it here, then connect bot.');
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
      logUiMessage('OAuth launch failed: ' + String(error));
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
      logUiMessage('OAuth launch failed: ' + String(error));
    }
  }

  async function onSwitchStreamer() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        logUiMessage('Set Twitch client ID first in OAuth Setup.');
        return;
      }
      if (!$authSessionsStore.botTokenPresent) {
        logUiMessage('Connect Bot first, then switch Streamer account.');
        return;
      }
      await connectTwitch(true, streamerAuthProfile, 'streamer');
      setTimeout(() => {
        void loadSavedOAuthSettings();
        void loadAuthSessions();
      }, 1200);
    } catch (error) {
      logUiMessage('Streamer switch failed: ' + String(error));
    }
  }

  async function onConnectStreamer() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        await openTwitchDevAppSetup();
        logUiMessage('OAuth setup required first: create app in Twitch Developer Console, save Client ID, then connect streamer.');
        return;
      }
      if (!$authSessionsStore.botTokenPresent) {
        logUiMessage('Connect Bot first, then connect Streamer.');
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
      logUiMessage('Streamer OAuth failed: ' + String(error));
    }
  }

  async function openBotLoginWindow() {
    try {
      await openIsolatedTwitchWindow(botAuthProfile, 'https://www.twitch.tv/login');
    } catch (error) {
      logUiMessage('Failed to open Twitch login: ' + String(error));
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
      logUiMessage('Reset auth sessions failed: ' + String(error));
    }
  }

  async function onDisconnectBot() {
    try {
      await clearBotSession();
      await loadStatus();
      await loadSavedOAuthSettings();
    } catch (error) {
      logUiMessage('Disconnect bot session failed: ' + String(error));
    }
  }

  async function onDisconnectStreamer() {
    try {
      await clearStreamerSession();
      await loadStatus();
      await loadSavedOAuthSettings();
    } catch (error) {
      logUiMessage('Disconnect streamer session failed: ' + String(error));
    }
  }

  async function joinNow() {
    try {
      if (!oauthConfigured) {
        showAdvanced = true;
        logUiMessage('Set Twitch client ID first in OAuth Setup.');
        return;
      }
      if (!canJoin) {
        logUiMessage('Activation order: Connect Bot -> Connect Streamer -> Connect Chat.');
        return;
      }
      await connectChat();
    } catch (error) {
      logUiMessage('Join chat failed: ' + String(error));
    }
  }

  async function leaveNow() {
    try {
      await disconnectChat();
    } catch (error) {
      logUiMessage('Leave chat failed: ' + String(error));
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

  $: if (canJoin && $statusStore.twitchState !== 'connected') {
    void maybeAutoJoin();
  }
</script>

<section class="card">
  <h3 class="title-row">
    Twitch Login
    <span class="oauth-status">
      <span class="status-dot {oauthConfigured ? 'ok' : 'bad'}"></span>
      {oauthConfigured ? 'OAuth configured' : 'OAuth not configured'}
    </span>
  </h3>
  <p class="muted">Simple flow: Save Twitch Client ID once, then connect Bot, connect Streamer, then connect Chat.</p>
  <p class="muted">Bot and Streamer must be different Twitch accounts.</p>

  {#if !oauthConfigured}
    <div class="oauth-required">
      <small class="muted">One-time setup: Create a Twitch app, copy its Client ID, then save it here. Use redirect URL <code>http://127.0.0.1:37219/callback</code>.</small>
      <Button.Root class="p-btn btn" on:click={openTwitchDevAppSetup}><Icon name="external" />Open Twitch App Setup</Button.Root>
      <input bind:value={clientId} placeholder="Twitch client ID (required)" />
      <input bind:value={redirectUrl} placeholder="Redirect URL" />
      <Button.Root class="p-btn btn" on:click={onSave}><Icon name="save" />Save OAuth Settings</Button.Root>
    </div>
  {/if}

  {#if oauthCode}
    <div class="oauth-code-card">
      <small class="muted">Authorize {oauthCode.role === 'streamer' ? 'Streamer' : 'Bot'} account with this code:</small>
      <div class="oauth-code-value">{oauthCode.userCode}</div>
      <div class="actions">
        <Button.Root class="p-btn btn" on:click={() => oauthCode && openExternal(oauthCode.verificationUrl)}><Icon name="external" />Open Twitch Activation Page</Button.Root>
        <Button.Root class="p-btn btn" on:click={() => oauthCode && navigator.clipboard?.writeText(oauthCode.userCode)}><Icon name="copy" />Copy Code</Button.Root>
        <Button.Root class="p-btn btn" on:click={() => (oauthCode = null)}><Icon name="close" />Dismiss</Button.Root>
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
    <Button.Root class="p-btn btn" on:click={onLogin} disabled={!oauthConfigured}><Icon name="bot" />1) Connect Bot</Button.Root>
    <Button.Root class="p-btn btn" on:click={onConnectStreamer} disabled={!oauthConfigured || !$authSessionsStore.botTokenPresent}><Icon name="user" />2) Connect Streamer</Button.Root>
    <Button.Root class="p-btn btn" on:click={joinNow} disabled={!oauthConfigured || !canJoin || $statusStore.twitchState === 'connected'}><Icon name="plug" />3) Connect Chat</Button.Root>
    <Button.Root class="p-btn btn" on:click={leaveNow}><Icon name="unplug" />Disconnect Chat</Button.Root>
  </div>

  <Button.Root class="p-btn btn tertiary-toggle" on:click={() => (showTools = !showTools)}>
    <Icon name="wrench" />{showTools ? 'Hide' : 'Show'} Extra Tools <Icon name="chevron" />
  </Button.Root>

  {#if showTools}
    <div class="actions">
      <Button.Root class="p-btn btn" on:click={openBotLoginWindow}><Icon name="external" />Open Twitch Login Page</Button.Root>
      <Button.Root class="p-btn btn" on:click={onSwitchBot}><Icon name="switch" />Switch Bot Account</Button.Root>
      <Button.Root class="p-btn btn" on:click={onSwitchStreamer}><Icon name="switch" />Switch Streamer Account</Button.Root>
      <Button.Root class="p-btn btn" on:click={onDisconnectBot}><Icon name="unplug" />Disconnect Bot Account</Button.Root>
      <Button.Root class="p-btn btn" on:click={onDisconnectStreamer}><Icon name="unplug" />Disconnect Streamer Account</Button.Root>
      <Button.Root class="p-btn btn" on:click={onResetAuth}><Icon name="reset" />Reset Auth Sessions</Button.Root>
    </div>
  {/if}

  <Button.Root class="p-btn btn tertiary-toggle" on:click={() => (showAdvanced = !showAdvanced)}>
    <Icon name="key" />{showAdvanced ? 'Hide' : 'Show'} Advanced OAuth Setup <Icon name="chevron" />
  </Button.Root>

  {#if showAdvanced}
    <input bind:value={clientId} placeholder="Twitch client ID" />
    <input bind:value={redirectUrl} placeholder="Redirect URL" />
    <Button.Root class="p-btn btn" on:click={onSave}><Icon name="save" />Save OAuth Settings</Button.Root>
  {/if}
</section>
