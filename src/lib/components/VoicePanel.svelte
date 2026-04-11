<script lang="ts">
  import { onMount } from 'svelte';
  import { getSttConfig, getTtsVoice, setTtsVoice, setTtsVolume, setVoiceEnabled } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';
  export let aiReady = false;
  export let chatReady = false;

  let selectedVoice = 'auto';
  let ttsVolume = 100;
  let sttReady = false;
  let lastResult = '';

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
    void loadSettings();
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
      sttReady = !!(stt.sttEnabled && stt.sttBinaryPath && stt.sttModelPath);
    } catch {
      sttReady = false;
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
    } catch (error) {
      errorBannerStore.set('Saving TTS settings failed: ' + String(error));
    }
  }
</script>

<section class="card grid">
  <h3>🔊 Voice</h3>
  <small class="muted">Cloud neural TTS output + local STT status. Mic toggle is in Main Session Chat.</small>
  {#if activationBlocked}
    <small class="muted">{activationBlockedReason}</small>
  {/if}

  <div class="tts-row">
    <label class="muted" for="tts-voice">TTS voice</label>
    <select id="tts-voice" bind:value={selectedVoice}>
      {#each voiceOptions as voice}
        <option value={voice}>{voice === 'auto' ? 'Auto (Recommended)' : voice}</option>
      {/each}
    </select>
    <label class="muted" for="tts-volume">Volume</label>
    <input id="tts-volume" type="range" min="0" max="100" step="1" bind:value={ttsVolume} />
    <small>{ttsVolume}%</small>
    <button class="btn" on:click={saveVoiceSettings}>Apply Voice</button>
  </div>

  <small class="muted state">
    <span class="light {sttReady ? 'on' : 'off'}" aria-hidden="true"></span>
    STT: {sttReady ? 'ready' : 'missing'}
  </small>
  {#if lastResult}<small>{lastResult}</small>{/if}
</section>

<style>
  .tts-row {
    display: grid;
    grid-template-columns: auto minmax(240px, 1fr) auto minmax(140px, 1fr) auto auto;
    align-items: center;
    gap: 0.45rem;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 0.38rem;
    flex-wrap: wrap;
  }
  .light {
    width: 1rem;
    height: 1rem;
    border: 1px solid rgba(0, 0, 0, 0.35);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.12) inset;
  }
  .light.on {
    background: #2bd35f;
  }
  .light.off {
    background: #d74646;
  }
  @media (max-width: 900px) {
    .tts-row {
      grid-template-columns: 1fr;
    }
  }
</style>
