<script lang="ts">
  import { onMount } from 'svelte';
  import { configureCloudOnlyMode, getProviderApiKey, openExternal, setProviderApiKey } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';

  let apiKey = '';
  let selectedModel = 'deepseek-v3.1:671b-cloud';
  let status = '';

  const recommended = [
    { id: 'deepseek-v3.1:671b-cloud', label: 'High quality general', tags: 'natural conversation, long-context', webSearch: 'Yes (via app tools)', vision: 'No' },
    { id: 'qwen3-coder:480b-cloud', label: 'Latest coding + tools', tags: 'tools, fast reasoning', webSearch: 'Yes (via app tools)', vision: 'No' },
    { id: 'gpt-oss:120b-cloud', label: 'Open weights assistant', tags: 'agentic, tool-ready', webSearch: 'Yes (via app tools)', vision: 'No' },
    { id: 'llava:latest', label: 'Vision fallback (if available)', tags: 'multimodal', webSearch: 'Yes (via app tools)', vision: 'Yes (image input)' }
  ];

  onMount(() => {
    void (async () => {
      try {
        const saved = await getProviderApiKey('ollama-cloud');
        if (saved) apiKey = saved;
      } catch {
        // Non-fatal; field stays empty.
      }
    })();
  });

  async function saveKeyAndCloudMode() {
    try {
      if (!apiKey.trim()) {
        status = 'Paste your Ollama API key first.';
        return;
      }
      await setProviderApiKey('ollama-cloud', apiKey.trim());
      await configureCloudOnlyMode(selectedModel);
      status = `Cloud-only mode enabled with ${selectedModel}.`;
    } catch (error) {
      const msg = String(error);
      if (msg.includes('model') && msg.includes('not found')) {
        status = `Model "${selectedModel}" not found. Choose another preset (recommended: qwen3-coder:480b-cloud).`;
      }
      errorBannerStore.set('Cloud setup failed: ' + msg);
    }
  }

  async function openOllama() {
    try {
      await openExternal('https://ollama.com');
    } catch (error) {
      errorBannerStore.set('Open ollama.com failed: ' + String(error));
    }
  }

  async function openKeys() {
    try {
      await openExternal('https://ollama.com/settings/keys');
    } catch (error) {
      errorBannerStore.set('Open Ollama API key page failed: ' + String(error));
    }
  }

  async function openDocs() {
    try {
      await openExternal('https://ollama.com/blog');
    } catch (error) {
      errorBannerStore.set('Open Ollama docs failed: ' + String(error));
    }
  }
</script>

<section class="card grid">
  <h3>☁️ Cloud AI Setup</h3>
  <small class="muted">Cloud-only mode: sign in to Ollama, create key, paste once, then choose model.</small>

  <div class="actions link-actions">
    <button class="btn" on:click={openOllama}>1) Open Ollama.com</button>
    <button class="btn" on:click={openKeys}>2) Open API Keys</button>
    <button class="btn" on:click={openDocs}>Docs</button>
  </div>

  <input bind:value={apiKey} placeholder="Paste Ollama API key" />

  <label class="muted" for="cloud-model-preset">Model preset</label>
  <select id="cloud-model-preset" bind:value={selectedModel}>
    {#each recommended as model}
      <option value={model.id}>{model.id} - {model.label} ({model.tags})</option>
    {/each}
  </select>

  <button class="btn" on:click={saveKeyAndCloudMode}>3) Enable Cloud-Only Mode</button>

  <div class="cap-grid">
    <div class="cap-head">Model</div>
    <div class="cap-head">Web Search</div>
    <div class="cap-head">Vision</div>
    {#each recommended as model}
      <div>{model.id}</div>
      <div>{model.webSearch}</div>
      <div>{model.vision}</div>
    {/each}
  </div>

  <small class="muted">Web search is app-driven (commands/tools), not auto-browsing by the model itself.</small>
  <small class="muted">Vision models can analyze images, but desktop screen-capture-to-model is not wired yet.</small>
  {#if status}
    <small>{status}</small>
  {/if}
</section>

<style>
  .actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .link-actions .btn {
    min-width: 150px;
  }
  .cap-grid {
    display: grid;
    grid-template-columns: 1.8fr 1fr 0.8fr;
    gap: 0.35rem 0.6rem;
    font-size: 0.85rem;
  }
  .cap-head {
    color: var(--muted);
    font-weight: 700;
  }
</style>
