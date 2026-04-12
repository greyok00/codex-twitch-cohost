<script lang="ts">
  import { Button } from 'bits-ui';
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import DiagnosticsPanel from './DiagnosticsPanel.svelte';
  import {
    autoConfigureSttFast,
    getSttConfig,
    getTtsVoice,
    setTtsVoice,
    setTtsVolume,
    setVoiceEnabled,
    verifyVoiceRuntime
  } from '../api/tauri';
  import type { VoiceRuntimeReport } from '../types';
  import { errorBannerStore } from '../stores/app';
  export let aiReady = false;
  export let chatReady = false;

  let selectedVoice = 'auto';
  let ttsVolume = 100;
  let sttReady = false;
  let ttsReady = false;
  let sttEnabled = false;
  let sttStatus = '';
  let lastResult = '';
  let verifying = false;
  let runtimeReport: VoiceRuntimeReport | null = null;

  $: activationBlockedReason = !aiReady
    ? 'Step order: connect AI first.'
    : !chatReady
      ? 'Step order: connect chat after AI.'
      : '';
  $: activationBlocked = activationBlockedReason.length > 0;

  const voiceOptions = [
    'auto',
    'en-US-JennyNeural',
    'en-US-AriaNeural',
    'en-US-GuyNeural',
    'en-US-AnaNeural',
    'en-GB-SoniaNeural'
  ];

  onMount(() => {
    void (async () => {
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
    if (activationBlocked) {
      errorBannerStore.set(activationBlockedReason);
      return;
    }
    try {
      await setVoiceEnabled(true);
      await setTtsVoice(selectedVoice === 'auto' ? null : selectedVoice);
      await setTtsVolume(ttsVolume);
      lastResult = `Saved voice ${selectedVoice} at ${ttsVolume}% volume.`;
      await runRuntimeVerification();
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
</script>

<section class="card grid">
  <h3>Settings</h3>
  <small class="muted">Voice output lives here. STT is auto-configured behind the scenes and no manual tuning is required.</small>
  {#if activationBlocked}
    <small class="muted">{activationBlockedReason}</small>
  {/if}

  <div class="tts-row">
    <label class="muted" for="tts-voice">TTS voice</label>
    <UiSelect
      bind:value={selectedVoice}
      options={voiceOptions.map((voice) => ({ value: voice, label: voice === 'auto' ? 'Auto (Recommended)' : voice }))}
      placeholder="Select TTS voice"
    />
    <label class="muted" for="tts-volume">Volume</label>
    <UiSlider bind:value={ttsVolume} min={0} max={100} step={1} ariaLabel="TTS volume" />
    <small>{ttsVolume}%</small>
    <Button.Root class="p-btn btn" on:click={saveVoiceSettings}><Icon name="voice" />Apply Voice</Button.Root>
    <Button.Root class="p-btn btn" on:click={runRuntimeVerification} disabled={verifying}>
      <Icon name="check" />{verifying ? 'Verifying...' : 'Verify STT/TTS'}
    </Button.Root>
  </div>
  {#if sttStatus}
    <small class="muted">{sttStatus}</small>
  {/if}

  <small class="muted state">
    <span class="light {sttReady ? 'on' : 'off'}" aria-hidden="true"></span>
    STT: {sttReady ? 'ready' : 'missing'}
    <span class="light {ttsReady ? 'on' : 'off'}" aria-hidden="true"></span>
    TTS: {ttsReady ? 'ready' : 'missing'}
  </small>
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
  {#if lastResult}<small>{lastResult}</small>{/if}

  <details class="settings-diagnostics">
    <summary>Diagnostics, Self-Test, and Debug Export</summary>
    <DiagnosticsPanel embedded={true} />
  </details>
</section>
