<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from 'bits-ui';
  import { configureCloudOnlyMode, getProviderApiKey, getProviderModels, openExternal, setProviderApiKey } from '../api/tauri';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import { errorBannerStore } from '../stores/app';

  let apiKey = '';
  let selectedModel = 'gemma3:12b';
  let status = '';
  let loadingModels = false;

  const recommended = [
    { id: 'gemma3:12b', label: 'Funnier cohost default', style: 'Richer tone + more context', context: 'Medium-High', webSearch: 'Search supported' },
    { id: 'qwen3:8b', label: 'Fast conversation', style: 'Quick + stable', context: 'Medium', webSearch: 'Search supported' },
    { id: 'llama3.2:3b', label: 'Very light model', style: 'Fastest option', context: 'Low-Medium', webSearch: 'Search supported' },
    { id: 'phi4:14b', label: 'Compact reasoning', style: 'Clean replies', context: 'Medium', webSearch: 'Search supported' }
  ];
  let discoveredModels: string[] = [];
  let fastDiscovered: Array<{ id: string; label: string; style: string; context: string; webSearch: string }> = [];

  function modelSizeHint(model: string): number {
    const m = model.toLowerCase();
    const match = m.match(/:(\d+(?:\.\d+)?)b\b/);
    if (!match) return 9999;
    return Number(match[1]);
  }

  function modelFamily(model: string): string {
    return model.split(':')[0].toLowerCase();
  }

  function pickFastModels(models: string[]): Array<{ id: string; label: string; style: string; context: string; webSearch: string }> {
    const ranked = [...models].sort((a, b) => modelSizeHint(a) - modelSizeHint(b));
    const seen = new Set<string>();
    const out: Array<{ id: string; label: string; style: string; context: string; webSearch: string }> = [];
    for (const id of ranked) {
      const fam = modelFamily(id);
      if (seen.has(fam)) continue;
      seen.add(fam);
      out.push({
        id,
        label: 'Detected fast pick',
        style: 'Auto-ranked small model',
        context: modelSizeHint(id) <= 12 ? 'Low-Medium' : 'Medium',
        webSearch: 'Search supported'
      });
      if (out.length >= 5) break;
    }
    return out;
  }

  async function refreshModels() {
    loadingModels = true;
    try {
      const models = await getProviderModels('ollama-cloud');
      discoveredModels = models;
      fastDiscovered = pickFastModels(models);
      if (models.length > 0) {
        const picks = fastDiscovered.length > 0 ? fastDiscovered : models.map((id) => ({ id, label: 'Detected model', style: 'Live', context: '-', webSearch: 'Search supported' }));
        if (!picks.find((m) => m.id === selectedModel)) selectedModel = picks[0].id;
        status = `Connected to Ollama Cloud. Found ${models.length} model(s). Picked up to 5 fast presets.`;
      } else {
        status = 'Connected, but no cloud models were returned for this account.';
      }
    } catch (error) {
      status = 'Cloud model discovery failed.';
      errorBannerStore.set('Cloud model discovery failed: ' + String(error));
    } finally {
      loadingModels = false;
    }
  }

  onMount(async () => {
    try {
      const saved = await getProviderApiKey('ollama-cloud');
      if (saved && saved.trim()) {
        apiKey = saved.trim();
        status = 'Using saved Ollama API key.';
        await refreshModels();
      }
    } catch {
      // no-op
    }
  });

  async function saveKeyAndCloudMode() {
    try {
      if (!apiKey.trim()) {
        status = 'Paste your Ollama API key first.';
        return;
      }
      await setProviderApiKey('ollama-cloud', apiKey.trim());
      await refreshModels();
      await configureCloudOnlyMode(selectedModel);
      status = `Cloud-only mode enabled with ${selectedModel}.`;
    } catch (error) {
      const msg = String(error);
      if (msg.includes('model') && msg.includes('not found')) status = `Model "${selectedModel}" not found for your account.`;
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
  <div class="actions">
    <Button.Root class="p-btn btn" on:click={refreshModels} disabled={loadingModels || !apiKey.trim()}>
      <Icon name="check" />{loadingModels ? 'Checking models...' : 'Check Cloud Models'}
    </Button.Root>
  </div>

  <label class="muted" for="cloud-model-preset">Model preset</label>
  <small class="muted">Choose a model from your account. `Check Cloud Models` loads available models directly from Ollama Cloud.</small>
  <UiSelect
    bind:value={selectedModel}
    options={(fastDiscovered.length > 0 ? fastDiscovered : recommended).map((model) => ({ value: model.id, label: `${model.id} - ${model.label}` }))}
    placeholder="Select cloud model preset"
  />

  <Button.Root class="p-btn btn" on:click={saveKeyAndCloudMode}><Icon name="cloud" />3) Enable Cloud-Only Mode</Button.Root>

  <div class="cap-grid">
    <div class="cap-head">Model</div>
    <div class="cap-head">Best For</div>
    <div class="cap-head">Style</div>
    <div class="cap-head">Context</div>
    <div class="cap-head">Search</div>
    {#each (fastDiscovered.length > 0 ? fastDiscovered : recommended) as model}
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
