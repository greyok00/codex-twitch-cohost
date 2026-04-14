<script lang="ts">
  import { afterUpdate, onDestroy, onMount } from 'svelte';
  import { Badge, Button, Card } from 'flowbite-svelte';
  import {
    autoConfigureSttFast,
    getSttConfig,
    submitVoiceSessionPrompt
  } from '../api/tauri';
  import { botLogStore, chatStore, errorBannerStore, eventStore } from '../stores/app';
  import { browserSpeechSupported } from '../voice-session/engines/browserSpeech';
  import { getOwnerVoiceSessionController, syncVoiceSessionWithBotReplies } from '../voice-session/VoiceSessionController';
  import { voiceSessionStore } from '../voice-session/store';

  let content = '';
  let sttReady = false;
  let sttFixing = false;
  let feedEl: HTMLDivElement | null = null;
  let unsubscribeVoiceReplies: (() => void) | null = null;

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
    .slice(-120);

  $: sessionState = $voiceSessionStore;
  $: browserSpeechReady = browserSpeechSupported();
  $: phaseLabel = sessionState.status === 'idle'
    ? 'Idle'
    : sessionState.status === 'starting'
      ? 'Starting'
      : sessionState.status === 'listening'
        ? 'Listening'
        : sessionState.status === 'processing'
          ? 'Processing'
          : sessionState.status === 'replying'
            ? 'Replying'
            : 'Error';

  afterUpdate(() => {
    if (!feedEl) return;
    requestAnimationFrame(() => {
      if (feedEl) feedEl.scrollTop = feedEl.scrollHeight;
    });
  });

  onMount(() => {
    void refreshSttReady();
    unsubscribeVoiceReplies = syncVoiceSessionWithBotReplies();
  });

  onDestroy(() => {
    if (unsubscribeVoiceReplies) unsubscribeVoiceReplies();
    void getOwnerVoiceSessionController().stop();
  });

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
      await refreshSttReady();
    } catch (error) {
      errorBannerStore.set(`STT auto-configure failed: ${String(error)}`);
    } finally {
      sttFixing = false;
    }
    return sttReady;
  }

  async function submit() {
    const outgoing = content.trim();
    if (!outgoing) return;
    content = '';
    try {
      await submitVoiceSessionPrompt(outgoing);
    } catch (error) {
      errorBannerStore.set(`Local AI send failed: ${String(error)}`);
    }
  }

  async function toggleMic() {
    const controller = getOwnerVoiceSessionController();
    if (sessionState.micEnabled) {
      await controller.stop();
      return;
    }
    const ready = await ensureVoiceReady();
    if (!ready) {
      errorBannerStore.set('Voice input is not ready.');
      return;
    }
    try {
      await controller.start();
    } catch (error) {
      errorBannerStore.set(`Mic start failed: ${String(error)}`);
    }
  }
</script>

<div class="flex h-full min-h-[700px] flex-col gap-4">
  <Card class="border border-slate-800 bg-slate-900/90">
    <div class="grid grid-cols-5 gap-3">
      <div class="rounded-2xl border border-slate-800 bg-slate-950/70 px-4 py-3">
        <div class="text-[10px] uppercase tracking-[0.18em] text-cyan-300">Phase</div>
        <div class="mt-1 text-sm font-semibold text-white">{phaseLabel}</div>
      </div>
      <div class="rounded-2xl border border-slate-800 bg-slate-950/70 px-4 py-3">
        <div class="text-[10px] uppercase tracking-[0.18em] text-cyan-300">Engine</div>
        <div class="mt-1 text-sm font-semibold text-white">{sessionState.engine}</div>
      </div>
      <div class="rounded-2xl border border-slate-800 bg-slate-950/70 px-4 py-3">
        <div class="text-[10px] uppercase tracking-[0.18em] text-cyan-300">Interim</div>
        <div class="mt-1 truncate text-sm text-slate-300">{sessionState.interimText || 'Waiting for speech...'}</div>
      </div>
      <div class="rounded-2xl border border-slate-800 bg-slate-950/70 px-4 py-3">
        <div class="text-[10px] uppercase tracking-[0.18em] text-cyan-300">Final</div>
        <div class="mt-1 text-sm text-slate-300">{sessionState.finalLatencyMs ?? '-'} ms</div>
      </div>
      <div class="rounded-2xl border border-slate-800 bg-slate-950/70 px-4 py-3">
        <div class="text-[10px] uppercase tracking-[0.18em] text-cyan-300">AI</div>
        <div class="mt-1 text-sm text-slate-300">{sessionState.aiLatencyMs ?? '-'} ms</div>
      </div>
    </div>
  </Card>

  <div class="flex min-h-0 flex-1 flex-col gap-4">
    <div bind:this={feedEl} class="min-h-0 flex-1 overflow-y-auto rounded-2xl border border-slate-800 bg-slate-950/70 p-4">
      <div class="space-y-3">
        {#each combined as item}
          <div class={`rounded-2xl px-4 py-3 text-sm ${item.source === 'bot' ? 'ml-6 bg-cyan-500/10 text-cyan-50' : item.source === 'system' ? 'border border-slate-800 bg-slate-900/70 text-slate-300' : 'mr-6 bg-slate-900 text-slate-100'}`}>
            <div class="mb-1 flex items-center justify-between gap-3 text-[11px] uppercase tracking-[0.18em] text-slate-400">
              <span>{item.user}</span>
              <span>{new Date(item.timestamp).toLocaleTimeString()}</span>
            </div>
            <p class="whitespace-pre-wrap leading-relaxed">{item.content}</p>
          </div>
        {/each}
        {#if !combined.length}
          <div class="rounded-2xl border border-dashed border-slate-800 bg-slate-950/60 p-6 text-sm text-slate-400">
            No conversation yet. Type locally or start the mic session.
          </div>
        {/if}
      </div>
    </div>

    <Card class="border border-slate-800 bg-slate-900/90">
      <div class="space-y-3">
        <textarea
          bind:value={content}
          rows="4"
          class="w-full rounded-2xl border border-slate-700 bg-slate-950 px-4 py-3 text-sm text-white outline-none ring-0 placeholder:text-slate-500 focus:border-cyan-400"
          placeholder="Type a local prompt for the cohost..."
        ></textarea>
        <div class="flex flex-wrap gap-3">
          <Button color="cyan" onclick={submit}>Send to AI</Button>
          <Button color={sessionState.micEnabled ? 'red' : 'light'} onclick={toggleMic}>
            {sessionState.micEnabled ? 'Stop Mic' : 'Start Mic'}
          </Button>
          <Badge color={sessionState.micEnabled ? 'green' : 'yellow'}>
            {sessionState.micEnabled ? 'Mic Live' : 'Mic Idle'}
          </Badge>
          <Badge color={sttReady ? 'green' : 'yellow'}>
            {browserSpeechReady ? 'Browser STT' : sttReady ? 'Local STT Ready' : 'STT Pending'}
          </Badge>
        </div>
      </div>
    </Card>
  </div>
</div>
