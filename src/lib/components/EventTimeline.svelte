<script lang="ts">
  import { eventStore } from '../stores/app';
</script>

<section class="card">
  <h3>📌 Event Timeline</h3>
  <div class="feed">
    {#if $eventStore.length === 0}
      <small class="muted">No events yet.</small>
    {:else}
      {#each $eventStore as item (item.id)}
        <div class="event">
          <strong>{item.kind}</strong>
          <span>{item.content}</span>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .feed {
    max-height: 210px;
    overflow: auto;
    display: grid;
    gap: 0.45rem;
  }
  .event {
    display: grid;
    gap: 0.2rem;
  }
</style>
