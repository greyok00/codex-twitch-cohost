<script lang="ts">
  import { setModel } from '../api/tauri';
  import { errorBannerStore, statusStore } from '../stores/app';

  const models = [
    { id: 'llama3.1:8b-instruct', label: 'Balanced chat speed' },
    { id: 'qwen2.5:7b-instruct', label: 'Good personality control' },
    { id: 'llama3.1:70b-instruct', label: 'Richer responses, slower' }
  ];

  async function choose(model: string) {
    try {
      await setModel(model);
    } catch (error) {
      errorBannerStore.set('Failed to switch model: ' + String(error));
    }
  }
</script>

<section class="card">
  <h3>🧠 Model Picker</h3>
  <small class="muted">Current: {$statusStore.model}</small>
  <div class="grid">
    {#each models as model}
      <button on:click={() => choose(model.id)}>{model.id} - {model.label}</button>
    {/each}
  </div>
</section>
