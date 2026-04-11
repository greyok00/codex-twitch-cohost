<script lang="ts">
  import { sendChat } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';

  let content = '';

  async function submit() {
    if (!content.trim()) return;
    const outgoing = content;
    content = '';
    try {
      await sendChat(outgoing);
    } catch (error) {
      errorBannerStore.set('Send failed: ' + String(error));
    }
  }
</script>

<section class="card">
  <h3>💬 Chat Composer</h3>
  <div class="row">
    <input bind:value={content} placeholder="Type message for Twitch chat" on:keydown={(e) => e.key === 'Enter' && submit()} />
    <button on:click={submit}>📤 Send</button>
  </div>
</section>

<style>
  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.6rem;
  }
</style>
