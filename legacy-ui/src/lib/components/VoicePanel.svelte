<script lang="ts">
  import { Button } from 'bits-ui';
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import DiagnosticsPanel from './DiagnosticsPanel.svelte';
  import {
    autoConfigureSttFast,
    captureMicDebug,
    getSttConfig,
    getTtsVoice,
    playTtsReaction,
    setRecordingSpeechBlock,
    setTtsVoice,
    setTtsVolume,
    setVoiceEnabled,
    stopBotSpeech,
    verifyVoiceRuntime
  } from '../api/tauri';
  import type { MicDebugView, VoiceRuntimeReport } from '../types';
  import { errorBannerStore } from '../stores/app';
  let selectedVoice = 'auto';
  let ttsVolume = 100;
  let sttReady = false;
  let ttsReady = false;
  let sttEnabled = false;
  let sttStatus = '';
  let lastResult = '';
  let verifying = false;
  let micDebugRunning = false;
  let micDebug: MicDebugView | null = null;
  let runtimeReport: VoiceRuntimeReport | null = null;
  let browserTtsAvailable = false;
  let reactionPreviewing = false;
  let customReaction = 'mmm...';

  const voiceOptions = [
    { value: 'auto', label: 'Auto (Recommended)' as const },
    { value: 'en-US-JennyNeural', label: 'Jenny' as const },
    { value: 'en-US-AriaNeural', label: 'Aria' as const },
    { value: 'en-US-GuyNeural', label: 'Guy' as const },
    { value: 'en-US-ChristopherNeural', label: 'Christopher' as const },
    { value: 'en-US-EricNeural', label: 'Eric' as const },
    { value: 'en-US-RogerNeural', label: 'Roger' as const },
    { value: 'en-US-SteffanNeural', label: 'Steffan' as const },
    { value: 'en-US-TonyNeural', label: 'Tony' as const },
    { value: 'en-US-AnaNeural', label: 'Ana' as const },
    { value: 'en-GB-SoniaNeural', label: 'Sonia' as const },
    { value: 'en-GB-RyanNeural', label: 'Ryan' as const },
    { value: 'en-AU-WilliamNeural', label: 'William' as const }
  ];

  onMount(() => {
    void (async () => {
      browserTtsAvailable = typeof window !== 'undefined' && 'speechSynthesis' in window;
      await loadSettings();
      await ensureSttDefaults();
      await runRuntimeVerification();
    })();
  });

  async function loadSettings() {
    try {
      const tts = await getTtsVoice();
      selectedVoice = tts.voiceName || 'auto';
      ttsVolume = tts.volumePercent ?? 100;
    } catch (error) {
      errorBannerStore.set('Failed loading TTS settings: ' + String(error));
    }
    try {
      const stt = await getSttConfig();
      sttEnabled = !!stt.sttEnabled;
      sttReady = !!(stt.sttEnabled && stt.sttBinaryPath && stt.sttModelPath);
      sttStatus = sttReady ? 'STT configured.' : 'STT needs enable + binary + model.';
    } catch {
      sttReady = false;
      sttStatus = 'Failed reading STT settings.';
    }
  }

  async function saveVoiceSettings() {
    try {
      await setVoiceEnabled(true);
      await setTtsVoice(selectedVoice === 'auto' ? null : selectedVoice);
      await setTtsVolume(ttsVolume);
      ttsReady = true;
      lastResult = `Saved voice ${selectedVoice} at ${ttsVolume}% volume.`;
    } catch (error) {
      errorBannerStore.set('Saving TTS settings failed: ' + String(error));
    }
  }

  async function runRuntimeVerification() {
    verifying = true;
    try {
      runtimeReport = await verifyVoiceRuntime();
      sttReady = runtimeReport.sttReady;
      ttsReady = runtimeReport.ttsReady;
      const sttCheck = runtimeReport.checks.find((c) => c.name.toLowerCase().includes('stt process'))
        || runtimeReport.checks.find((c) => c.name.toLowerCase().startsWith('stt'));
      if (sttCheck) sttStatus = sttCheck.details;
    } catch (error) {
      runtimeReport = null;
      errorBannerStore.set('Voice runtime verification failed: ' + String(error));
      ttsReady = browserTtsAvailable;
    } finally {
      verifying = false;
    }
  }

  async function ensureSttDefaults() {
    if (sttReady) return;
    try {
      const result = await autoConfigureSttFast();
      sttEnabled = result.sttEnabled;
      sttStatus = result.message || 'Auto-configure completed.';
    } catch (error) {
      sttStatus = 'STT auto-configure failed. Restart app and retry.';
      errorBannerStore.set('STT auto-configure failed: ' + String(error));
    }
  }

  async function runMicDebugCapture() {
    micDebugRunning = true;
    try {
      stopBotSpeech();
      setRecordingSpeechBlock(true);
      micDebug = await captureMicDebug(1800);
      sttStatus = `Mic debug captured via ${micDebug.backend}.`;
    } catch (error) {
      errorBannerStore.set('Mic debug capture failed: ' + String(error));
    } finally {
      setRecordingSpeechBlock(false);
      micDebugRunning = false;
    }
  }

  async function previewReaction(reaction: string) {
    reactionPreviewing = true;
    try {
      await playTtsReaction(reaction, selectedVoice === 'auto' ? null : selectedVoice, ttsVolume);
      lastResult = `Played reaction: ${reaction}.`;
    } catch (error) {
      errorBannerStore.set('TTS reaction failed: ' + String(error));
    } finally {
      reactionPreviewing = false;
    }
  }

  $: effectiveTtsReady = ttsReady || browserTtsAvailable;
</script>

<section class="card grid">
  <h3>Settings</h3>
  <small class="muted">Voice output lives here. STT is auto-configured behind the scenes and no manual tuning is required.</small>

  <div class="tts-row">
    <label class="muted" for="tts-voice">TTS voice</label>
    <UiSelect
      bind:value={selectedVoice}
      options={voiceOptions}
      placeholder="Select TTS voice"
    />
    <label class="muted" for="tts-volume">Volume</label>
    <UiSlider bind:value={ttsVolume} min={0} max={100} step={1} ariaLabel="TTS volume" />
    <small>{ttsVolume}%</small>
    <Button.Root class="p-btn btn" on:click={saveVoiceSettings}><Icon name="voice" />Apply Voice</Button.Root>
    <Button.Root class="p-btn btn" on:click={runRuntimeVerification} disabled={verifying}>
      <Icon name="check" />{verifying ? 'Verifying...' : 'Verify STT/TTS'}
    </Button.Root>
    <Button.Root class="p-btn btn" on:click={runMicDebugCapture} disabled={micDebugRunning}>
      <Icon name="mic" />{micDebugRunning ? 'Capturing...' : 'Mic Debug Capture'}
    </Button.Root>
  </div>
  {#if sttStatus}
    <small class="muted">{sttStatus}</small>
  {/if}
  <div class="grid">
    <small class="muted">Non-word TTS reactions</small>
    <div class="row">
      <Button.Root class="p-btn btn" on:click={() => previewReaction('soft hum')} disabled={reactionPreviewing}>
        <Icon name="play" />Soft hum
      </Button.Root>
      <Button.Root class="p-btn btn" on:click={() => previewReaction('thinking hum')} disabled={reactionPreviewing}>
        <Icon name="play" />Thinking hum
      </Button.Root>
      <Button.Root class="p-btn btn" on:click={() => previewReaction('surprised')} disabled={reactionPreviewing}>
        <Icon name="play" />Surprised
      </Button.Root>
      <Button.Root class="p-btn btn" on:click={() => previewReaction('excited')} disabled={reactionPreviewing}>
        <Icon name="play" />Excited
      </Button.Root>
    </div>
    <div class="row">
      <input bind:value={customReaction} maxlength="32" placeholder="Custom sound like mmm... or oh!" />
      <Button.Root class="p-btn btn" on:click={() => previewReaction(customReaction)} disabled={reactionPreviewing}>
        <Icon name="voice" />Play Custom
      </Button.Root>
    </div>
  </div>

  <small class="muted state">
    <span class="light {sttReady ? 'on' : 'off'}" aria-hidden="true"></span>
    STT: {sttReady ? 'ready' : 'missing'}
    <span class="light {effectiveTtsReady ? 'on' : 'off'}" aria-hidden="true"></span>
    TTS: {effectiveTtsReady ? 'ready' : 'missing'}
  </small>
  {#if !ttsReady && browserTtsAvailable}
    <small class="muted">Using browser speech fallback if native TTS verification is unavailable.</small>
  {/if}
  {#if runtimeReport}
    <small class="muted">Voice verification: {runtimeReport.overall.toUpperCase()} ({runtimeReport.generatedAt})</small>
    <div class="checks">
      {#each runtimeReport.checks as check}
        <small class="check {check.status}">
          [{check.status.toUpperCase()}] {check.name}: {check.details}
        </small>
      {/each}
    </div>
  {/if}
  {#if micDebug}
    <div class="checks">
      <small class="check pass">[DEBUG] Backend: {micDebug.backend}</small>
      <small class="check pass">[DEBUG] Duration: {micDebug.durationMs} ms</small>
      <small class="check pass">[DEBUG] Transcript: {micDebug.transcript || '<empty>'}</small>
      <small class="check pass">[DEBUG] WAV: {micDebug.wavPath}</small>
    </div>
  {/if}
  {#if lastResult}<small>{lastResult}</small>{/if}

  <details class="settings-diagnostics">
    <summary>Diagnostics, Self-Test, and Debug Export</summary>
    <DiagnosticsPanel embedded={true} />
  </details>
</section>
