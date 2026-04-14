<script lang="ts">
  import { onMount } from 'svelte';
  let ActiveComponent: any = null;

  onMount(async () => {
    const params = new URLSearchParams(window.location.search);
    const isPublicCall = !!params.get('call');
    const isAvatarPopup = !!params.get('avatar');
    const isUtility = !!params.get('utility');
    if (isAvatarPopup) {
      const mod = await import('./lib/components/AvatarPopupPage.svelte');
      ActiveComponent = mod.default;
      return;
    }
    if (isUtility) {
      const mod = await import('./lib/components/BackendUtilityPage.svelte');
      ActiveComponent = mod.default;
      return;
    }
    if (isPublicCall) {
      const mod = await import('./lib/components/PublicCallPage.svelte');
      ActiveComponent = mod.default;
      return;
    }
    const mod = await import('./lib/components/AppShell.svelte');
    ActiveComponent = mod.default;
  });
</script>

<div class="app-theme">
  {#if ActiveComponent}
    <svelte:component this={ActiveComponent} />
  {/if}
</div>
