<script lang="ts">
  import { onMount } from 'svelte';
  import { openExternal } from '../api/tauri';
  import { detectRuntimeCapabilities } from '../runtime/CapabilityService';
  import { runtimeCapabilityStore } from '../stores/runtime';
  import TwitchLoginCard from './TwitchLoginCard.svelte';
  import PublicCallPanel from './PublicCallPanel.svelte';

  const socialLinks = [
    { label: 'GitHub', handle: '@greyok00', url: 'https://github.com/greyok00', icon: '/social/github.svg' },
    { label: 'Twitch', handle: '@greyok__', url: 'https://twitch.tv/greyok__', icon: '/social/twitch.svg' },
    { label: 'YouTube', handle: '@GreyOK_0', url: 'https://www.youtube.com/@GreyOK_0', icon: '/social/youtube.svg' },
    { label: 'Discord', handle: "GreyOK_'s Discord", url: 'https://discord.gg/TJcr6ZxJ', icon: '/social/discord.svg' }
  ] as const;

  async function openLink(url: string) {
    try {
      await openExternal(url);
    } catch {
      // no-op
    }
  }

  onMount(() => {
    void detectRuntimeCapabilities();
  });
</script>

<section class="grid">
  <TwitchLoginCard />
  <PublicCallPanel />
  <section class="card grid">
    <h3>Runtime</h3>
    <small class="muted">This reports whether the browser runtime can actually support worker-based concurrency and future GPU-backed rendering or compute paths.</small>
    <div class="checks">
      <small class="check {$runtimeCapabilityStore.workerSupported ? 'pass' : 'warn'}">Workers: {$runtimeCapabilityStore.workerSupported ? 'available' : 'missing'}</small>
      <small class="check {$runtimeCapabilityStore.webGpuSupportedMain ? 'pass' : 'warn'}">WebGPU main thread: {$runtimeCapabilityStore.webGpuSupportedMain ? 'available' : 'missing'}</small>
      <small class="check {$runtimeCapabilityStore.webGpuInitializedMain ? 'pass' : 'warn'}">WebGPU init: {$runtimeCapabilityStore.webGpuInitializedMain ? 'adapter ready' : 'not initialized'}</small>
      <small class="check {$runtimeCapabilityStore.webGpuDeviceReadyMain ? 'pass' : 'warn'}">WebGPU device: {$runtimeCapabilityStore.webGpuDeviceReadyMain ? 'ready' : 'unavailable'}</small>
      <small class="check {$runtimeCapabilityStore.webGpuSupportedWorker ? 'pass' : 'warn'}">WebGPU worker: {$runtimeCapabilityStore.webGpuSupportedWorker ? 'available' : 'missing'}</small>
      <small class="check {$runtimeCapabilityStore.offscreenCanvasSupported ? 'pass' : 'warn'}">OffscreenCanvas: {$runtimeCapabilityStore.offscreenCanvasSupported ? 'available' : 'missing'}</small>
      <small class="check {$runtimeCapabilityStore.webGpuAdapterName ? 'pass' : 'warn'}">GPU adapter: {$runtimeCapabilityStore.webGpuAdapterName ?? 'unknown'}</small>
      <small class="check pass">Hardware threads: {$runtimeCapabilityStore.hardwareConcurrency}</small>
      <small class="check pass">Device memory: {$runtimeCapabilityStore.deviceMemoryGb ?? 'unknown'} GB</small>
      <small class="check {$runtimeCapabilityStore.secureContext ? 'pass' : 'warn'}">Secure context: {$runtimeCapabilityStore.secureContext ? 'yes' : 'no'}</small>
    </div>
  </section>
  <section class="card about-card">
    <div class="about-head">
      <img src="/top-logo.png" alt="GreyOK" class="about-logo" />
      <div>
        <h3>App</h3>
        <p class="muted">Accounts, public access, and app-wide links live here.</p>
      </div>
    </div>
    <div class="social-list">
      {#each socialLinks as link}
        <button type="button" class="social-link" on:click={() => openLink(link.url)}>
          <img src={link.icon} alt={link.label + ' logo'} class="social-icon" />
          <span class="social-meta">
            <strong>{link.label}</strong>
            <small>{link.handle}</small>
          </span>
        </button>
      {/each}
    </div>
  </section>
</section>
