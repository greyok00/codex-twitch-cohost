<script lang="ts">
  import { Button } from 'bits-ui';
  import { clearMemory, summarizeChat } from '../api/tauri';
  import Icon from './ui/Icon.svelte';
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
  <h3>Memory</h3>
  <div class="row">
    <Button.Root class="p-btn" on:click={summarize}><Icon name="summary" />Summarize Chat</Button.Root>
    <Button.Root class="p-btn" on:click={wipe}><Icon name="trash" />Reset Memory</Button.Root>
  </div>
  {#if summary}
    <p>{summary}</p>
  {/if}
  <small class="muted">Event cache: {$eventStore.length}</small>
</section>
