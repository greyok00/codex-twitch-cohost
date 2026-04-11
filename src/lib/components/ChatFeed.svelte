<script lang="ts">
  import { chatStore } from '../stores/app';
  export let title = '📺 Live Chat Feed';
</script>

<section class="card">
  <h3>{title}</h3>
  <div class="feed">
    {#if $chatStore.length === 0}
      <small class="muted">No chat yet.</small>
    {:else}
      {#each $chatStore as line (line.id)}
        <div class="line">
          <strong>{line.user}</strong>
          <span>{line.content}</span>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .feed {
    max-height: 260px;
    overflow: auto;
    display: grid;
    gap: 0.45rem;
  }
  .line {
    display: flex;
    gap: 0.55rem;
    flex-wrap: wrap;
    font-size: 0.93rem;
  }
</style>
