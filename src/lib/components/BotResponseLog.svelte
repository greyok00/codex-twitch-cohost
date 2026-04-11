<script lang="ts">
  import { botLogStore } from '../stores/app';
</script>

<section class="card">
  <h3>🤖 Bot Response Log (Live)</h3>
  <div class="feed">
    {#if $botLogStore.length === 0}
      <small class="muted">No bot responses yet.</small>
    {:else}
      {#each $botLogStore as line (line.id)}
        <div class="line">
          <span>{line.content}</span>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .feed {
    max-height: 120px;
    overflow: auto;
    display: grid;
    gap: 0.4rem;
  }
  .line {
    font-size: 0.92rem;
  }
</style>
