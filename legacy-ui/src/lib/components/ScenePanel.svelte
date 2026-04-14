<script lang="ts">
  import { onMount } from 'svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import { getSceneSettings, setSceneSettings } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';
  import type { SceneSettings } from '../types';

  let scene: SceneSettings = {
    mode: 'solo',
    maxTurnsBeforePause: 2,
    allowExternalTopicChanges: true,
    secondaryCharacterSlug: ''
  };
  let ready = false;

  onMount(async () => {
    try {
      scene = await getSceneSettings();
    } catch (error) {
      errorBannerStore.set('Scene settings load failed: ' + String(error));
    } finally {
      ready = true;
    }
  });

  async function persist() {
    if (!ready) return;
    try {
      await setSceneSettings(
        scene.mode,
        scene.maxTurnsBeforePause,
        scene.allowExternalTopicChanges,
        scene.secondaryCharacterSlug
      );
    } catch (error) {
      errorBannerStore.set('Scene settings save failed: ' + String(error));
    }
  }

  $: if (ready) {
    void persist();
  }
</script>

<section class="card grid">
  <h3>Scene</h3>
  <small class="muted">Scene rules now save into the backend rules file instead of browser-local storage.</small>

  <div class="grid">
    <small class="muted">Scene mode</small>
    <UiSelect
      bind:value={scene.mode}
      options={[
        { value: 'solo', label: 'Solo character' },
        { value: 'dual_debate', label: 'Dual-character debate' },
        { value: 'chat_topic', label: 'Chat-driven topic' }
      ]}
      placeholder="Select scene mode"
    />
  </div>

  <div class="grid">
    <small class="muted">Turns before pause</small>
    <UiSlider bind:value={scene.maxTurnsBeforePause} min={1} max={6} step={1} ariaLabel="Turns before pause" />
    <small>{scene.maxTurnsBeforePause} turn(s)</small>
  </div>

  <div class="grid">
    <small class="muted">Secondary character slug</small>
    <input bind:value={scene.secondaryCharacterSlug} placeholder="Reserved for debate mode" />
  </div>

  <div class="grid">
    <label class="muted">
      <input type="checkbox" bind:checked={scene.allowExternalTopicChanges} />
      Allow chat or outside input to shift the topic
    </label>
  </div>
</section>
