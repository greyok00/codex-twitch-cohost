<script lang="ts">
  import { clearMemory, summarizeChat } from '../api/tauri';
  import { errorBannerStore, eventStore } from '../stores/app';

  let summary = '';

  async function summarize() {
    try {
      summary = await summarizeChat();
    } catch (error) {
      errorBannerStore.set('Summary failed: ' + String(error));
    }
  }

  async function wipe() {
    try {
      await clearMemory();
      summary = 'Memory cleared.';
    } catch (error) {
      errorBannerStore.set('Memory clear failed: ' + String(error));
    }
  }
</script>

<section class="card grid">
  <h3>🧾 Memory</h3>
  <div class="row">
    <button on:click={summarize}>📝 Summarize Chat</button>
    <button on:click={wipe}>🧹 Reset Memory</button>
  </div>
  {#if summary}
    <p>{summary}</p>
  {/if}
  <small class="muted">Event cache: {$eventStore.length}</small>
</section>

<style>
  .row {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
</style>
