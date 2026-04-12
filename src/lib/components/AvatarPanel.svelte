<script lang="ts">
  import { Button } from 'bits-ui';
  import { onDestroy, onMount } from 'svelte';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import Icon from './ui/Icon.svelte';
  import { getSavedAvatarImage, saveAvatarImage } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';
  export let aiReady = false;
  export let chatReady = false;
  export let voiceReady = false;

  let selectedName = '';
  let ready = false;
  let imageWidth = 560;
  let imageHeight = 760;
  let avatarChannel: BroadcastChannel | null = null;
  $: activationBlockedReason = !aiReady
    ? 'Step order: connect AI first.'
    : !chatReady
      ? 'Step order: connect chat after AI.'
      : !voiceReady
        ? 'Step order: activate voice before avatar.'
        : '';
  $: activationBlocked = activationBlockedReason.length > 0;

  onMount(() => {
    if (typeof BroadcastChannel !== 'undefined') {
      avatarChannel = new BroadcastChannel('cohost-avatar-events');
      avatarChannel.onmessage = (event) => {
        const msg = event.data || {};
        if (msg.type === 'mouth_align' && msg.align) {
          localStorage.setItem('cohost_mouth_align_v1', JSON.stringify(msg.align));
        }
        if (msg.type === 'brow_align' && msg.align) {
          localStorage.setItem('cohost_brow_align_v1', JSON.stringify(msg.align));
        }
      };
    }
    void loadSavedAvatar();
  });

  onDestroy(() => {
    if (avatarChannel) {
      avatarChannel.close();
      avatarChannel = null;
    }
  });

  function syncAvatarAlignToPopup() {
    try {
      if (!avatarChannel) return;
      const mouthRaw = localStorage.getItem('cohost_mouth_align_v1');
      const browRaw = localStorage.getItem('cohost_brow_align_v1');
      if (mouthRaw) {
        avatarChannel.postMessage({ type: 'set_mouth_align', align: JSON.parse(mouthRaw) });
      }
      if (browRaw) {
        avatarChannel.postMessage({ type: 'set_brow_align', align: JSON.parse(browRaw) });
      }
    } catch {
      // no-op
    }
  }

  async function loadSavedAvatar() {
    try {
      const saved = await getSavedAvatarImage();
      const fallback = '/floating-head.png';
      const source = saved?.dataUrl || localStorage.getItem('cohost_avatar_image') || fallback;
      selectedName = saved?.fileName || (source === fallback ? 'floating head.png' : 'saved-avatar');
      localStorage.setItem('cohost_avatar_image', source);
      const probe = new Image();
      probe.onload = () => {
        imageWidth = Math.max(320, Math.min(1100, probe.naturalWidth));
        imageHeight = Math.max(420, Math.min(1400, probe.naturalHeight));
        localStorage.setItem(
          'cohost_avatar_size',
          JSON.stringify({ width: imageWidth, height: imageHeight })
        );
        ready = true;
      };
      probe.onerror = () => {
        ready = true;
      };
      probe.src = source;
    } catch (error) {
      errorBannerStore.set('Failed loading saved avatar image: ' + String(error));
    }
  }

  function onPhotoPick(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    selectedName = file.name;
    const reader = new FileReader();
    reader.onerror = () => {
      errorBannerStore.set('Failed reading image file.');
    };
    reader.onloadend = () => {
      const dataUrl = String(reader.result || '');
      if (!dataUrl) return;
      void (async () => {
        try {
          const saved = await saveAvatarImage(dataUrl, file.name);
          localStorage.setItem('cohost_avatar_image', saved.dataUrl);
          const probe = new Image();
          probe.onload = () => {
            imageWidth = Math.max(320, Math.min(1100, probe.naturalWidth));
            imageHeight = Math.max(420, Math.min(1400, probe.naturalHeight));
            localStorage.setItem(
              'cohost_avatar_size',
              JSON.stringify({ width: imageWidth, height: imageHeight })
            );
            ready = true;
          };
          probe.onerror = () => {
            ready = true;
          };
          probe.src = saved.dataUrl;
        } catch (error) {
          errorBannerStore.set('Failed saving avatar image: ' + String(error));
        }
      })();
    };
    reader.readAsDataURL(file);
  }

  async function openPopup() {
    if (activationBlocked) {
      errorBannerStore.set(activationBlockedReason);
      return;
    }
    try {
      const existing = await WebviewWindow.getByLabel('cohost-avatar');
      if (existing) {
        try {
          const raw = localStorage.getItem('cohost_avatar_size');
          if (raw) {
            const parsed = JSON.parse(raw) as { width?: number; height?: number };
            const w = Math.max(320, Math.min(1200, Number(parsed.width || imageWidth) + 24));
            const h = Math.max(420, Math.min(1500, Number(parsed.height || imageHeight) + 60));
            await existing.setSize(new LogicalSize(w, h));
          }
        } catch {
          // no-op
        }
        await existing.show();
        await existing.setFocus();
        window.setTimeout(() => syncAvatarAlignToPopup(), 120);
        return;
      }

      let popupW = imageWidth + 24;
      let popupH = imageHeight + 60;
      try {
        const raw = localStorage.getItem('cohost_avatar_size');
        if (raw) {
          const parsed = JSON.parse(raw) as { width?: number; height?: number };
          popupW = Number(parsed.width || popupW) + 24;
          popupH = Number(parsed.height || popupH) + 60;
        }
      } catch {
        // no-op
      }
      popupW = Math.max(320, Math.min(1200, popupW));
      popupH = Math.max(420, Math.min(1500, popupH));

      const win = new WebviewWindow('cohost-avatar', {
        url: '/avatar-popup.html',
        title: 'Cohost Avatar',
        width: popupW,
        height: popupH,
        minWidth: 420,
        minHeight: 520,
        resizable: true,
        alwaysOnTop: true,
        transparent: true,
        backgroundColor: '#00000000'
      });

      win.once('tauri://error', (e) => {
        errorBannerStore.set(`Failed to open avatar window: ${String((e as { payload?: unknown })?.payload ?? 'unknown error')}`);
      });
      window.setTimeout(() => syncAvatarAlignToPopup(), 260);
    } catch (error) {
      errorBannerStore.set(`Failed to open avatar window: ${String(error)}`);
    }
  }
</script>

<section class="card grid">
  <h3>Avatar Popup (OBS Layer)</h3>
  <small class="muted">Upload a PNG with transparency, or use default `floating head.png`. Bot speech drives mouth + brow animation in real time.</small>
  {#if activationBlocked}
    <small class="muted">{activationBlockedReason}</small>
  {/if}
  <input type="file" accept="image/*" on:change={onPhotoPick} />
  {#if selectedName}<small>Saved avatar: {selectedName}</small>{/if}
  <Button.Root class="p-btn" on:click={openPopup} disabled={!ready || activationBlocked}><Icon name="avatar" />Open Avatar Popup</Button.Root>
  <small class="muted">In OBS Browser Source (dev): `http://localhost:1420/avatar-popup.html`</small>
</section>
