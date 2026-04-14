<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Badge, Button, Card } from 'flowbite-svelte';
  import {
    autoConfigureSttFast,
    getBackendControlSnapshot,
    getSttConfig,
    loadAuthSessions,
    loadPersonality,
    loadStatus,
    registerEventListeners,
    startBackendDaemon
  } from '../api/tauri';
  import { authSessionsStore, diagnosticsStore, errorBannerStore, statusStore } from '../stores/app';
  import type { BackendControlSnapshot, BackendModuleView } from '../types';
  import OwnerSessionConsole from './OwnerSessionConsole.svelte';
  import DesktopCommandCenter from './DesktopCommandCenter.svelte';

  let backendSnapshot: BackendControlSnapshot | null = null;
  let backendLoading = true;
  let backendStarting = false;
  let sttReady = false;
  let startupError = '';
  let sttTimer: number | null = null;
  let backendTimer: number | null = null;
  let sttAutoTried = false;

  $: authReady = $authSessionsStore.botTokenPresent && $authSessionsStore.streamerTokenPresent;
  $: aiReady = $diagnosticsStore.providerState === 'connected';
  $: chatReady = $statusStore.twitchState === 'connected';
  $: backendModules = backendSnapshot?.modules ?? [];

  onMount(async () => {
    try {
      document.documentElement.setAttribute('data-theme', 'dark');
      await registerEventListeners();
      await Promise.all([loadStatus(), loadAuthSessions(), loadPersonality()]);
      await maybeAutoConfigureStt();
      await Promise.all([refreshBackendSnapshot(), refreshSttReady()]);
      sttTimer = window.setInterval(() => void refreshSttReady(), 3500);
      backendTimer = window.setInterval(() => void refreshBackendSnapshot(), 2500);
    } catch (error) {
      startupError = String(error);
      errorBannerStore.set(`Startup sync failed: ${String(error)}`);
    }
  });

  onDestroy(() => {
    if (sttTimer !== null) window.clearInterval(sttTimer);
    if (backendTimer !== null) window.clearInterval(backendTimer);
  });

  async function refreshBackendSnapshot() {
    try {
      backendSnapshot = await getBackendControlSnapshot();
      startupError = '';
    } catch (error) {
      startupError = String(error);
    } finally {
      backendLoading = false;
    }
  }

  async function bootBackend() {
    backendStarting = true;
    try {
      backendSnapshot = await startBackendDaemon();
      startupError = '';
    } catch (error) {
      startupError = String(error);
      errorBannerStore.set(`Backend start failed: ${String(error)}`);
    } finally {
      backendStarting = false;
      backendLoading = false;
    }
  }

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
      if (cfg.sttEnabled && cfg.sttBinaryPath && cfg.sttModelPath) {
        sttReady = true;
        return;
      }
      await autoConfigureSttFast();
      await refreshSttReady();
    } catch {
      sttReady = false;
    }
  }

  function lightClass(module: BackendModuleView): string {
    if (module.light === 'green') return 'bg-emerald-400 shadow-emerald-400/50';
    if (module.light === 'yellow') return 'bg-amber-300 shadow-amber-300/50';
    return 'bg-rose-400 shadow-rose-400/50';
  }
</script>

<main class="h-screen overflow-hidden bg-slate-950 text-slate-50">
  <div class="mx-auto flex h-screen w-full max-w-[1500px] min-w-[1280px] flex-col gap-4 p-4">
    <Card class="border border-slate-800/80 bg-slate-900/90 shadow-2xl shadow-cyan-950/30">
      <div class="flex items-center gap-3">
        <div class="min-w-[180px]">
          <p class="text-[10px] font-semibold uppercase tracking-[0.28em] text-cyan-300">Desktop Command Center</p>
          <h1 class="text-xl font-black tracking-tight text-white">GreyOK Cohost Runtime</h1>
        </div>
        <div class="flex min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-2xl border border-slate-800 bg-slate-950/70 px-3 py-2">
          {#each backendModules as module}
            <div class="flex min-w-0 items-center gap-2 rounded-xl border border-slate-800 bg-slate-900/80 px-2 py-1">
              <span class={`h-3 w-3 rounded-full shadow-lg ${lightClass(module)}`}></span>
              <span class="truncate text-[11px] font-semibold uppercase tracking-[0.14em] text-slate-200">{module.name}</span>
            </div>
          {/each}
        </div>
        <div class="flex items-center gap-2">
          <Badge color={authReady ? 'green' : 'red'}>{authReady ? 'Auth' : 'No Auth'}</Badge>
          <Badge color={aiReady ? 'green' : 'red'}>{aiReady ? 'AI' : 'AI Off'}</Badge>
          <Badge color={chatReady ? 'green' : 'yellow'}>{chatReady ? 'Twitch' : 'Local'}</Badge>
          <Badge color={sttReady ? 'green' : 'yellow'}>{sttReady ? 'STT' : 'STT Wait'}</Badge>
          <Button color="light" onclick={refreshBackendSnapshot} disabled={backendLoading}>Refresh</Button>
          <Button color="cyan" onclick={bootBackend} disabled={backendStarting}>
            {backendStarting ? 'Starting…' : 'Start Backend'}
          </Button>
        </div>
      </div>
    </Card>

    <div class="grid min-h-0 flex-1 grid-cols-[minmax(0,980px)_480px] justify-between gap-4">
      <section class="flex min-h-0 flex-col gap-4">
        <Card class="border border-slate-800/80 bg-slate-900/90 shadow-xl shadow-slate-950/40">
          <div class="flex flex-wrap items-start justify-between gap-4">
            <div class="space-y-2">
              <div class="flex flex-wrap gap-2">
                <Badge color={authReady ? 'green' : 'red'}>{authReady ? 'Accounts Ready' : 'Accounts Missing'}</Badge>
                <Badge color={aiReady ? 'green' : 'red'}>{aiReady ? 'AI Connected' : 'AI Offline'}</Badge>
                <Badge color={chatReady ? 'green' : 'yellow'}>{chatReady ? 'Twitch Joined' : 'Local Only'}</Badge>
                <Badge color={sttReady ? 'green' : 'yellow'}>{sttReady ? 'STT Ready' : 'STT Pending'}</Badge>
                <Badge color={backendSnapshot?.connected ? 'green' : 'red'}>
                  {backendSnapshot?.connected ? 'Backend Live' : 'Backend Down'}
                </Badge>
              </div>
              <div class="space-y-1">
                <h2 class="text-2xl font-bold text-white">Unified Console</h2>
                <p class="text-sm text-slate-400">
                  This is the combined operator surface for local chat, Twitch-facing replies, events, and eventually embedded CLI output.
                </p>
              </div>
            </div>
            {#if startupError}
              <div class="max-w-md rounded-2xl border border-rose-500/30 bg-rose-950/50 px-4 py-3 text-sm text-rose-200">
                {startupError}
              </div>
            {/if}
          </div>
        </Card>

        <Card class="border border-slate-800/80 bg-slate-900/90 shadow-xl shadow-slate-950/40">
          <OwnerSessionConsole />
        </Card>
      </section>

      <aside class="flex min-h-0 flex-col gap-4">
        <DesktopCommandCenter />
      </aside>
    </div>
  </div>
</main>
