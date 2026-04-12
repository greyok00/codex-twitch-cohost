<script lang="ts">
  import { diagnosticsStore } from '../stores/app';

  function stateTone(state: string): string {
    if (state === 'connected') return 'ok';
    if (state === 'connecting') return 'warn';
    return 'bad';
  }
</script>

<section class="card grid">
  <h3>Diagnostics</h3>
  <div class="state-row">
    <span class="pill {stateTone($diagnosticsStore.twitchState)}">Twitch: {$diagnosticsStore.twitchState}</span>
    <span class="pill {stateTone($diagnosticsStore.providerState)}">Provider: {$diagnosticsStore.providerState}</span>
  </div>
  <small>Uptime: {$diagnosticsStore.uptimeSeconds}s</small>
  {#if $diagnosticsStore.lastError}
    <small class="muted">Last error: {$diagnosticsStore.lastError}</small>
  {/if}
</section>
