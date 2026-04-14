<script lang="ts">
  import { Button } from 'bits-ui';
  import { getPublicCallSettings, rotatePublicCallToken, setPublicCallSettings } from '../api/tauri';
  import type { PublicCallSettings } from '../types';
  import { errorBannerStore } from '../stores/app';
  import Icon from './ui/Icon.svelte';

  let settings: PublicCallSettings | null = null;
  let callUrl = '';

  async function refresh() {
    try {
      settings = await getPublicCallSettings();
      const base = typeof window !== 'undefined' ? window.location.origin + window.location.pathname : '';
      callUrl = settings?.token ? `${base}?call=${settings.token}` : '';
    } catch (error) {
      errorBannerStore.set('Public call settings load failed: ' + String(error));
    }
  }

  async function toggleEnabled() {
    if (!settings) return;
    try {
      settings = await setPublicCallSettings(!settings.enabled, settings.defaultCharacterSlug);
      await refresh();
    } catch (error) {
      errorBannerStore.set('Public call toggle failed: ' + String(error));
    }
  }

  async function rotate() {
    try {
      settings = await rotatePublicCallToken();
      await refresh();
    } catch (error) {
      errorBannerStore.set('Public call token rotate failed: ' + String(error));
    }
  }

  async function copyLink() {
    if (!callUrl) return;
    try {
      await navigator.clipboard.writeText(callUrl);
    } catch (error) {
      errorBannerStore.set('Public call link copy failed: ' + String(error));
    }
  }

  void refresh();
</script>

<section class="card grid">
  <h3>Public Call Link</h3>
  <small class="muted">This creates a stable call token and a lightweight browser call route. The page is designed to run separately from the owner dashboard. For true public internet access outside this machine, the route still needs hosting or proxy exposure.</small>

  {#if settings}
    <div class="row">
      <Button.Root class="p-btn" on:click={toggleEnabled}>
        <Icon name="switch" />{settings.enabled ? 'Disable Public Call' : 'Enable Public Call'}
      </Button.Root>
      <Button.Root class="p-btn" on:click={rotate}>
        <Icon name="reset" />Rotate Link
      </Button.Root>
      <Button.Root class="p-btn" on:click={copyLink} disabled={!callUrl}>
        <Icon name="copy" />Copy Link
      </Button.Root>
    </div>
    <small class="muted">Status: {settings.enabled ? 'enabled' : 'disabled'}</small>
    <small class="muted">Token: {settings.token}</small>
    <small class="muted">Link: {callUrl || 'unavailable'}</small>
  {/if}
</section>
