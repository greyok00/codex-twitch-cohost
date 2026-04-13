<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { getSavedAvatarImage, saveAvatarImage } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';

  let selectedName = '';
  let ready = false;
  let imageWidth = 560;
  let imageHeight = 760;
  let avatarChannel: BroadcastChannel | null = null;

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
      const stored = localStorage.getItem('cohost_avatar_image')?.trim() || '';
      const source = saved?.dataUrl || stored || fallback;
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
        localStorage.setItem('cohost_avatar_image', fallback);
        selectedName = 'floating head.png';
        ready = true;
      };
      probe.src = source;
    } catch (error) {
      errorBannerStore.set('Failed loading saved avatar image: ' + String(error));
      localStorage.setItem('cohost_avatar_image', '/floating-head.png');
      selectedName = 'floating head.png';
      ready = true;
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

  $: if (ready) {
    window.setTimeout(() => syncAvatarAlignToPopup(), 80);
  }

</script>

<section class="card grid">
  <h3>Avatar Popup (OBS Layer)</h3>
  <small class="muted">Upload a PNG with transparency, or use default `floating head.png`. Avatar is docked to the right of chat in the main window.</small>
  <small class="muted">Rig controls are on the main avatar image (top-left panel). Use the on-image `Rig controls` button.</small>
  <div class="avatar-upload">
    <label class="muted" for="avatar-upload-input">Avatar image</label>
    <input id="avatar-upload-input" type="file" accept="image/*" on:change={onPhotoPick} />
  </div>
  {#if selectedName}<small>Saved avatar: {selectedName}</small>{/if}
  <small class="muted">Changes apply immediately to the docked avatar.</small>
</section>
