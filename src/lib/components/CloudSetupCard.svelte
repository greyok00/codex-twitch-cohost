<script lang="ts">
  import { Button } from 'bits-ui';
  import { configureCloudOnlyMode, openExternal, setProviderApiKey } from '../api/tauri';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import { errorBannerStore } from '../stores/app';

  let apiKey = '';
  let selectedModel = 'llama3.3:70b-instruct';
  let status = '';

  const recommended = [
    { id: 'llama3.3:70b-instruct', label: 'Best overall conversation quality', style: 'Natural, witty, stable', context: 'High', webSearch: 'App search supported' },
    { id: 'qwen2.5:32b-instruct', label: 'Balanced chat + reasoning', style: 'Fast, coherent, versatile', context: 'High', webSearch: 'App search supported' },
    { id: 'mistral-small:24b-instruct', label: 'Low-latency banter', style: 'Quick and concise', context: 'Medium', webSearch: 'App search supported' },
    { id: 'llama3.1:8b-instruct', label: 'Lightweight fallback', style: 'Fast local-friendly fallback', context: 'Medium', webSearch: 'App search supported' }
  ];

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
        status = `Model "${selectedModel}" not found. Try llama3.1:8b-instruct as fallback.`;
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

<section class="card grid compact-cloud">
  <h3>Cloud AI Setup</h3>
  <small class="muted">Focused on conversational cohost models. Use your Ollama account and key.</small>

  <div class="actions link-actions">
    <Button.Root class="p-btn btn" on:click={openOllama}><Icon name="external" />1) Open Ollama (create account)</Button.Root>
    <Button.Root class="p-btn btn" on:click={openKeys}><Icon name="key" />2) Open API Keys (free key)</Button.Root>
    <Button.Root class="p-btn btn" on:click={openDocs}><Icon name="book" />Docs</Button.Root>
  </div>

  <input type="password" autocomplete="off" bind:value={apiKey} placeholder="Paste Ollama API key" />

  <label class="muted" for="cloud-model-preset">Model preset</label>
  <UiSelect
    bind:value={selectedModel}
    options={recommended.map((model) => ({ value: model.id, label: `${model.id} - ${model.label}` }))}
    placeholder="Select cloud model preset"
  />

  <Button.Root class="p-btn btn" on:click={saveKeyAndCloudMode}><Icon name="cloud" />3) Enable Cloud-Only Mode</Button.Root>

  <div class="cap-grid">
    <div class="cap-head">Model</div>
    <div class="cap-head">Best For</div>
    <div class="cap-head">Style</div>
    <div class="cap-head">Context</div>
    <div class="cap-head">Search</div>
    {#each recommended as model}
      <div>{model.id}</div>
      <div>{model.label}</div>
      <div>{model.style}</div>
      <div>{model.context}</div>
      <div>{model.webSearch}</div>
    {/each}
  </div>

  <small class="muted">Search is handled by the app and fed back into conversation context.</small>
  {#if status}
    <small>{status}</small>
  {/if}
</section>
