<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from 'bits-ui';
  import { configureCloudOnlyMode, getProviderApiKey, getProviderModels, openExternal, setProviderApiKey } from '../api/tauri';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import { errorBannerStore } from '../stores/app';

  let apiKey = '';
  let selectedModel = 'qwen3:8b';
  let status = '';
  let loadingModels = false;

  type ModelMeta = {
    id: string;
    label: string;
    style: string;
    context: string;
    webSearch: string;
    uncensored?: boolean;
    available?: boolean;
  };

  const recommended: ModelMeta[] = [
    { id: 'qwen3:8b', label: 'Conversational | Qwen 8B', style: 'Fast everyday conversation', context: '8B', webSearch: 'App-fed search' },
    { id: 'qwen3:14b', label: 'Conversational | Qwen 14B', style: 'Stronger follow-through', context: '14B', webSearch: 'App-fed search' },
    { id: 'gemma3:12b', label: 'Conversational | Gemma 12B', style: 'Cleaner longer replies', context: '12B', webSearch: 'App-fed search' },
    { id: 'gemma3:27b', label: 'Conversational | Gemma 27B', style: 'Best depth of the normal set', context: '27B', webSearch: 'App-fed search' },
    { id: 'wizard-vicuna-uncensored', label: 'UNCENSORED | Wizard Vicuna 7B', style: 'Loose general chat', context: '7B', webSearch: 'App-fed search', uncensored: true },
    { id: 'dolphin-mistral', label: 'UNCENSORED | Dolphin Mistral 7B', style: 'Edgier conversation', context: '7B', webSearch: 'App-fed search', uncensored: true },
    { id: 'dolphin-mixtral', label: 'UNCENSORED | Dolphin Mixtral 8x7B', style: 'Heavier uncensored option', context: '8x7B', webSearch: 'App-fed search', uncensored: true },
    { id: 'dolphin-phi', label: 'UNCENSORED | Dolphin Phi 3B', style: 'Small uncensored option', context: '3B', webSearch: 'App-fed search', uncensored: true }
  ];
  let discoveredModels: string[] = [];
  let catalogModels: ModelMeta[] = [];

  function normalizeFamily(model: string): string {
    return model.toLowerCase().replace(/:(latest|[\w.\-]+)$/i, '');
  }

  function enrichModel(id: string): ModelMeta {
    const lower = id.toLowerCase();
    const family = normalizeFamily(lower);
    const direct = recommended.find((entry) => lower === entry.id.toLowerCase());
    if (direct) return { ...direct, id };
    const familyMatch = recommended.find((entry) => family.startsWith(normalizeFamily(entry.id)));
    if (familyMatch) return { ...familyMatch, id };
    const uncensored = lower.includes('uncensored') || lower.startsWith('dolphin-');
    return {
      id,
      label: uncensored ? 'Uncensored discovered model' : 'Discovered cloud model',
      style: uncensored ? 'Looser-aligned output' : 'Live account model',
      context: '-',
      webSearch: 'App-fed search',
      uncensored,
      available: true
    };
  }

  function rebuildCatalog(models: string[]) {
    const availableFamilies = new Set(models.map((model) => normalizeFamily(model)));
    catalogModels = recommended
      .map((entry) => {
        const matched = models.find((model) => normalizeFamily(model).startsWith(normalizeFamily(entry.id)));
        const resolved = matched ? enrichModel(matched) : { ...entry };
        return {
          ...resolved,
          available: availableFamilies.has(normalizeFamily(entry.id)) || !!matched
        };
      })
      .sort((a, b) => {
        if (!!a.uncensored !== !!b.uncensored) return a.uncensored ? 1 : -1;
        return recommended.findIndex((entry) => entry.id === a.id || normalizeFamily(a.id).startsWith(normalizeFamily(entry.id)))
          - recommended.findIndex((entry) => entry.id === b.id || normalizeFamily(b.id).startsWith(normalizeFamily(entry.id)));
      });
  }

  async function refreshModels() {
    loadingModels = true;
    try {
      const models = await getProviderModels('ollama-cloud');
      discoveredModels = models;
      rebuildCatalog(models);
      if (models.length > 0) {
        if (!catalogModels.find((m) => m.id === selectedModel)) selectedModel = catalogModels[0].id;
        const uncensoredCount = catalogModels.filter((entry) => entry.uncensored).length;
        const availableCount = catalogModels.filter((entry) => entry.available).length;
        status = `Connected to Ollama Cloud. Showing ${catalogModels.length} curated picks, ${availableCount} available on this account, including ${uncensoredCount} uncensored option(s).`;
      } else {
        status = 'Connected, but account discovery returned no models. Curated picks are still listed below.';
      }
    } catch (error) {
      status = 'Cloud model discovery failed.';
      errorBannerStore.set('Cloud model discovery failed: ' + String(error));
    } finally {
      loadingModels = false;
    }
  }

  onMount(async () => {
    rebuildCatalog([]);
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
  <small class="muted">Four conversational picks and four uncensored picks. Uncensored models are labeled directly.</small>
  <UiSelect
    bind:value={selectedModel}
    options={catalogModels.map((model) => ({
      value: model.id,
      label: model.label
    }))}
    placeholder="Select cloud model preset"
  />

  <Button.Root class="p-btn btn" on:click={saveKeyAndCloudMode}><Icon name="cloud" />3) Enable Cloud-Only Mode</Button.Root>

  <div class="cap-grid">
    <div class="cap-head">Preset</div>
    <div class="cap-head">Model</div>
    <div class="cap-head">Style</div>
    <div class="cap-head">Size</div>
    <div class="cap-head">Notes</div>
    {#each catalogModels as model}
      <div>{model.label}</div>
      <div>{model.id}</div>
      <div>{model.style}</div>
      <div>{model.context}</div>
      <div>{model.uncensored ? 'Uncensored' : 'Conversational'}{model.available === false ? ' | Not detected on account' : ''}</div>
    {/each}
  </div>

  <small class="muted">Search is handled by the app and fed back into conversation context.</small>
  {#if status}
    <small>{status}</small>
  {/if}
</section>
