<script lang="ts">
  import { Button } from 'bits-ui';
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import UiSwitch from './ui/UiSwitch.svelte';
  import {
    autoConfigureSttFast,
    getSttConfig,
    getTtsVoice,
    setSttConfig,
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
  let sttBinaryPath = '';
  let sttModelPath = '';
  let sttStatus = '';
  let lastResult = '';
  let verifying = false;
  let sttAutoConfiguring = false;
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
      sttBinaryPath = stt.sttBinaryPath || '';
      sttModelPath = stt.sttModelPath || '';
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

  async function saveSttSettings() {
    try {
      await setSttConfig(
        sttEnabled,
        sttBinaryPath.trim() || null,
        sttModelPath.trim() || null
      );
      sttStatus = 'Saved STT settings.';
      await runRuntimeVerification();
    } catch (error) {
      errorBannerStore.set('Saving STT settings failed: ' + String(error));
    }
  }

  async function autoSetupStt() {
    sttAutoConfiguring = true;
    try {
      const result = await autoConfigureSttFast();
      sttEnabled = result.sttEnabled;
      sttBinaryPath = result.sttBinaryPath || '';
      sttModelPath = result.sttModelPath || '';
      sttStatus = result.message || 'Auto-configure completed.';
      await runRuntimeVerification();
    } catch (error) {
      errorBannerStore.set('STT auto-configure failed: ' + String(error));
    } finally {
      sttAutoConfiguring = false;
    }
  }
</script>

<section class="card grid">
  <h3>Voice</h3>
  <small class="muted">Cloud neural TTS output + local STT status. Mic toggle is in Main Session Chat.</small>
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

  <div class="stt-grid">
    <div class="toggle">
      <UiSwitch bind:checked={sttEnabled} ariaLabel="Enable STT" />
      <span>Enable STT</span>
    </div>
    <label class="muted" for="stt-bin">STT binary path</label>
    <input id="stt-bin" bind:value={sttBinaryPath} placeholder="whisper-cli or full path" />
    <label class="muted" for="stt-model">STT model path</label>
    <input id="stt-model" bind:value={sttModelPath} placeholder="/path/to/ggml-*.bin" />
    <div class="stt-actions">
      <Button.Root class="p-btn btn" on:click={saveSttSettings}><Icon name="save" />Save STT</Button.Root>
      <Button.Root class="p-btn btn" on:click={autoSetupStt} disabled={sttAutoConfiguring}>
        <Icon name="wrench" />{sttAutoConfiguring ? 'Auto-configuring...' : 'Auto-configure STT'}
      </Button.Root>
    </div>
    {#if sttStatus}
      <small class="muted">{sttStatus}</small>
    {/if}
  </div>

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
</section>
