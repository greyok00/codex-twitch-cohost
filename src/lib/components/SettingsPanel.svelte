<script lang="ts">
  import { openExternal, searchWeb, setLurkMode, setProviderApiKey } from '../api/tauri';
  import { errorBannerStore, statusStore } from '../stores/app';

  let query = '';
  let searchOutput = '';
  let openUrl = 'https://www.twitch.tv';
  let providerName = 'ollama-cloud';
  let providerKey = '';

  async function doSearch() {
    try {
      searchOutput = await searchWeb(query);
    } catch (error) {
      errorBannerStore.set('Search failed: ' + String(error));
    }
  }

  async function open() {
    try {
      await openExternal(openUrl);
    } catch (error) {
      errorBannerStore.set('Open URL failed: ' + String(error));
    }
  }

  async function saveProviderKey() {
    if (!providerName.trim() || !providerKey.trim()) return;
    try {
      await setProviderApiKey(providerName, providerKey);
      searchOutput = `Saved API key for ${providerName} in local keychain.`;
      providerKey = '';
    } catch (error) {
      errorBannerStore.set('Saving provider key failed: ' + String(error));
    }
  }
</script>

<section class="card grid">
  <h3>Settings + Tools</h3>
  <button on:click={() => setLurkMode(!$statusStore.lurkMode)}>
    {$statusStore.lurkMode ? 'Disable' : 'Enable'} Lurk Mode
  </button>
  <input bind:value={query} placeholder="Search the web" />
  <button on:click={doSearch}>Run Search</button>
  {#if searchOutput}<p>{searchOutput}</p>{/if}
  <input bind:value={openUrl} placeholder="https://" />
  <button on:click={open}>Open URL (explicit)</button>
  <input bind:value={providerName} placeholder="Provider name (ex: ollama-cloud)" />
  <input bind:value={providerKey} placeholder="Provider API key (stored locally)" />
  <button on:click={saveProviderKey}>Save Provider Key</button>
</section>
