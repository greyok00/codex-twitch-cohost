<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Button } from 'flowbite-svelte';
  import { getBackendControlSnapshot, launchBackendTerminal, startBackendDaemon } from '../api/tauri';
  import type { BackendControlSnapshot, BackendModuleView } from '../types';

  let snapshot: BackendControlSnapshot | null = null;
  let error = '';
  let timer: number | null = null;
  let starting = false;

  async function refresh() {
    try {
      snapshot = await getBackendControlSnapshot();
      error = '';
    } catch (err) {
      error = String(err);
    }
  }

  async function start() {
    starting = true;
    try {
      snapshot = await startBackendDaemon();
      error = '';
    } catch (err) {
      error = String(err);
    } finally {
      starting = false;
    }
  }

  async function openTerminal() {
    try {
      await launchBackendTerminal();
    } catch (err) {
      error = String(err);
    }
  }

  function lightClasses(module: BackendModuleView): string {
    if (module.light === 'green') return 'bg-emerald-400 shadow-emerald-400/60';
    if (module.light === 'yellow') return 'bg-amber-300 shadow-amber-300/60';
    return 'bg-rose-400 shadow-rose-400/60';
  }

  onMount(() => {
    void refresh();
    timer = window.setInterval(() => void refresh(), 1500);
  });

  onDestroy(() => {
    if (timer !== null) window.clearInterval(timer);
  });
</script>

<main class="h-screen overflow-hidden bg-slate-950 text-slate-50">
  <div class="flex h-full w-full items-center gap-3 px-3 py-2">
    <div class="min-w-[110px]">
      <div class="text-[10px] font-semibold uppercase tracking-[0.22em] text-cyan-300">Utility</div>
      <div class="text-sm font-black text-white">Backend Control</div>
      {#if error}
        <div class="truncate text-[11px] text-rose-300">{error}</div>
      {/if}
    </div>

    <div class="flex flex-1 items-center gap-2 overflow-hidden rounded-2xl border border-slate-800 bg-slate-900/90 px-3 py-2">
      {#each snapshot?.modules ?? [] as module}
        <div class="flex min-w-0 items-center gap-2 rounded-xl border border-slate-800 bg-slate-950/80 px-2 py-1">
          <span class={`h-3 w-3 rounded-full shadow-lg ${lightClasses(module)}`}></span>
          <span class="truncate text-[11px] font-semibold uppercase tracking-[0.14em] text-slate-200">{module.name}</span>
        </div>
      {/each}
    </div>

    <div class="flex items-center gap-2">
      <Button color="light" size="xs" onclick={refresh}>Refresh</Button>
      <Button color="cyan" size="xs" onclick={start} disabled={starting}>{starting ? 'Starting…' : 'Start'}</Button>
      <Button color="dark" size="xs" onclick={openTerminal}>CLI</Button>
    </div>
  </div>
</main>
