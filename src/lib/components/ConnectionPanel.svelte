<script lang="ts">
  import { connectChat, disconnectChat } from '../api/tauri';
  import { statusStore } from '../stores/app';
  import StatusIndicator from './StatusIndicator.svelte';

  $: status = $statusStore;
</script>

<section class="card grid">
  <h3>🛰️ Connection</h3>
  <StatusIndicator label="Twitch" state={status.twitchState} />
  <div class="actions">
    <button on:click={connectChat}>▶️ Join Channel</button>
    <button on:click={disconnectChat}>⏹️ Leave Channel</button>
  </div>
  <small>Channel: {status.channel || 'not configured'}</small>
</section>

<style>
  .actions {
    display: flex;
    gap: 0.6rem;
  }
</style>
