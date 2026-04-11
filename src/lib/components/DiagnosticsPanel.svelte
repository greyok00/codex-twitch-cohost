<script lang="ts">
  import { diagnosticsStore } from '../stores/app';
  import StatusIndicator from './StatusIndicator.svelte';
</script>

<section class="card grid">
  <h3>🩺 Diagnostics</h3>
  <StatusIndicator label="Twitch" state={$diagnosticsStore.twitchState} />
  <StatusIndicator label="Provider" state={$diagnosticsStore.providerState} />
  <small>Uptime: {$diagnosticsStore.uptimeSeconds}s</small>
  {#if $diagnosticsStore.lastError}
    <small class="muted">Last error: {$diagnosticsStore.lastError}</small>
  {/if}
</section>
