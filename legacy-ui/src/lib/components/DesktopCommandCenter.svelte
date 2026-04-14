<script lang="ts">
  import { onMount } from 'svelte';
  import { Badge, Button, Card } from 'flowbite-svelte';
  import {
    getBehaviorSettings,
    getCharacterStudioSettings,
    getPublicCallSettings,
    getSceneSettings,
    getTtsVoice,
    launchBackendTerminal,
    setBehaviorSettings,
    setCharacterStudioSettings,
    setModel,
    setPublicCallSettings,
    setSceneSettings,
    setTtsVoice,
    setTtsVolume
  } from '../api/tauri';
  import { errorBannerStore } from '../stores/app';

  type ModelMode = 'fast' | 'medium' | 'long_context';

  let selectedPreset = 'basic-assistant';
  let warmth = 70;
  let humor = 35;
  let flirt = 0;
  let edge = 5;
  let energy = 40;
  let story = 35;
  let extraDirection = '';

  let modelMode: ModelMode = 'medium';
  let voiceName = 'auto';
  let volume = 100;

  let autoComments = false;
  let keepTalking = false;
  let postToTwitch = false;
  let pace = 0.6;

  let sceneMode = 'solo';
  let maxTurnsBeforePause = 2;
  let allowExternalTopicChanges = true;
  let secondaryCharacterSlug = '';
  let publicCallEnabled = false;

  const presetOptions = [
    ['basic-assistant', 'Basic Assistant'],
    ['midnight-host', 'Midnight Host'],
    ['sharp-bestie', 'Sharp Bestie'],
    ['studio-analyst', 'Studio Analyst'],
    ['story-weaver', 'Story Weaver'],
    ['velvet-flirt', 'Velvet Flirt'],
    ['dangerous-charmer', 'Dangerous Charmer'],
    ['sweet-trouble', 'Sweet Trouble'],
    ['after-dark', 'After Dark'],
    ['heat-check', 'Heat Check'],
    ['black-velvet-villain', 'Black Velvet Villain'],
    ['no-filter-menace', 'No Filter Menace']
  ] as const;

  const voiceOptions = [
    'auto',
    'en-US-JennyNeural',
    'en-US-AriaNeural',
    'en-US-GuyNeural',
    'en-US-ChristopherNeural',
    'en-US-EricNeural',
    'en-US-RogerNeural',
    'en-US-SteffanNeural',
    'en-US-TonyNeural',
    'en-US-AnaNeural',
    'en-GB-SoniaNeural',
    'en-GB-RyanNeural',
    'en-AU-WilliamNeural'
  ];

  function scheduledMinutesForPace(rate: number): number | null {
    if (rate <= 0) return null;
    if (rate >= 3.5) return 1;
    if (rate >= 2.0) return 2;
    if (rate >= 1.0) return 3;
    if (rate >= 0.6) return 5;
    if (rate >= 0.3) return 10;
    return 15;
  }

  function replyIntervalMsForPace(rate: number): number {
    if (rate <= 0) return 60_000;
    return Math.max(1200, Math.min(60_000, Math.round(60_000 / rate)));
  }

  function paceFromBehaviorSettings(behavior: Awaited<ReturnType<typeof getBehaviorSettings>>): number {
    if (!behavior.cohostMode) return 0.6;
    const interval = Number(behavior.minimumReplyIntervalMs ?? 0);
    if (interval > 0) return Math.max(0, Math.min(4, 60_000 / interval));
    const scheduled = Number(behavior.scheduledMessagesMinutes ?? 0);
    if (scheduled > 0) return Math.max(0, Math.min(4, 1 / scheduled));
    return 0.6;
  }

  function modelForMode(mode: ModelMode): string {
    if (mode === 'fast') return 'llama3.2:3b';
    if (mode === 'long_context') return 'gemma3:12b';
    return 'qwen3:8b';
  }

  async function hydrate() {
    try {
      const [character, voice, behavior, scene, publicCall] = await Promise.all([
        getCharacterStudioSettings(),
        getTtsVoice(),
        getBehaviorSettings(),
        getSceneSettings(),
        getPublicCallSettings()
      ]);
      selectedPreset = character.selectedPreset || selectedPreset;
      warmth = character.warmth;
      humor = character.humor;
      flirt = character.flirt;
      edge = character.edge;
      energy = character.energy;
      story = character.story;
      extraDirection = character.extraDirection || '';
      voiceName = voice.voiceName || 'auto';
      volume = voice.volumePercent ?? 100;
      autoComments = !!behavior.cohostMode;
      keepTalking = !!behavior.topicContinuationMode;
      postToTwitch = !!behavior.postBotMessagesToTwitch;
      pace = paceFromBehaviorSettings(behavior);
      sceneMode = scene.mode;
      maxTurnsBeforePause = scene.maxTurnsBeforePause;
      allowExternalTopicChanges = scene.allowExternalTopicChanges;
      secondaryCharacterSlug = scene.secondaryCharacterSlug || '';
      publicCallEnabled = !!publicCall.enabled;
    } catch (error) {
      errorBannerStore.set(`Command center hydrate failed: ${String(error)}`);
    }
  }

  async function saveCharacter() {
    try {
      await setCharacterStudioSettings({
        selectedPreset,
        warmth,
        humor,
        flirt,
        edge,
        energy,
        story,
        extraDirection
      });
    } catch (error) {
      errorBannerStore.set(`Character save failed: ${String(error)}`);
    }
  }

  async function saveVoiceAndModel() {
    try {
      await setModel(modelForMode(modelMode));
      await setTtsVoice(voiceName === 'auto' ? null : voiceName);
      await setTtsVolume(volume);
    } catch (error) {
      errorBannerStore.set(`Voice/AI save failed: ${String(error)}`);
    }
  }

  async function saveConversationRules() {
    try {
      await setBehaviorSettings(
        autoComments,
        autoComments ? scheduledMinutesForPace(pace) : null,
        replyIntervalMsForPace(pace),
        postToTwitch,
        keepTalking
      );
    } catch (error) {
      errorBannerStore.set(`Conversation rule save failed: ${String(error)}`);
    }
  }

  async function saveSceneAndApp() {
    try {
      await setSceneSettings(sceneMode as 'solo' | 'dual_debate' | 'chat_topic', maxTurnsBeforePause, allowExternalTopicChanges, secondaryCharacterSlug);
      await setPublicCallSettings(publicCallEnabled, selectedPreset);
    } catch (error) {
      errorBannerStore.set(`Scene/app save failed: ${String(error)}`);
    }
  }

  async function openCli() {
    try {
      await launchBackendTerminal();
    } catch (error) {
      errorBannerStore.set(`CLI launch failed: ${String(error)}`);
    }
  }

  onMount(() => {
    void hydrate();
  });
</script>

<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
  <Card class="border border-slate-800 bg-slate-900/90">
    <div class="space-y-4">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-xs font-semibold uppercase tracking-[0.22em] text-cyan-300">Character</p>
          <h3 class="text-lg font-black text-white">Preset + Personality Mix</h3>
        </div>
        <Badge color="cyan">{selectedPreset}</Badge>
      </div>
      <select bind:value={selectedPreset} class="w-full rounded-xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white">
        {#each presetOptions as [id, label]}
          <option value={id}>{label}</option>
        {/each}
      </select>
      <div class="grid grid-cols-2 gap-3 text-sm text-slate-300">
        <label>Warmth <input class="w-full accent-cyan-400" type="range" min="0" max="100" bind:value={warmth} /></label>
        <label>Humor <input class="w-full accent-cyan-400" type="range" min="0" max="100" bind:value={humor} /></label>
        <label>Flirt <input class="w-full accent-cyan-400" type="range" min="0" max="100" bind:value={flirt} /></label>
        <label>Edge <input class="w-full accent-cyan-400" type="range" min="0" max="100" bind:value={edge} /></label>
        <label>Energy <input class="w-full accent-cyan-400" type="range" min="0" max="100" bind:value={energy} /></label>
        <label>Story <input class="w-full accent-cyan-400" type="range" min="0" max="100" bind:value={story} /></label>
      </div>
      <textarea bind:value={extraDirection} rows="3" class="w-full rounded-xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white" placeholder="Extra direction for the active character..."></textarea>
      <Button color="cyan" onclick={saveCharacter}>Save Character</Button>
    </div>
  </Card>

  <Card class="border border-slate-800 bg-slate-900/90">
    <div class="space-y-4">
      <div>
        <p class="text-xs font-semibold uppercase tracking-[0.22em] text-cyan-300">Voice + AI</p>
        <h3 class="text-lg font-black text-white">Runtime Pairing</h3>
      </div>
      <div class="grid grid-cols-3 gap-2">
        <button type="button" class={`rounded-xl border px-3 py-2 text-sm ${modelMode === 'fast' ? 'border-cyan-400 bg-cyan-500/10 text-cyan-50' : 'border-slate-800 bg-slate-950 text-slate-200'}`} onclick={() => (modelMode = 'fast')}>Fast</button>
        <button type="button" class={`rounded-xl border px-3 py-2 text-sm ${modelMode === 'medium' ? 'border-cyan-400 bg-cyan-500/10 text-cyan-50' : 'border-slate-800 bg-slate-950 text-slate-200'}`} onclick={() => (modelMode = 'medium')}>Balanced</button>
        <button type="button" class={`rounded-xl border px-3 py-2 text-sm ${modelMode === 'long_context' ? 'border-cyan-400 bg-cyan-500/10 text-cyan-50' : 'border-slate-800 bg-slate-950 text-slate-200'}`} onclick={() => (modelMode = 'long_context')}>Long</button>
      </div>
      <select bind:value={voiceName} class="w-full rounded-xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white">
        {#each voiceOptions as voice}
          <option value={voice}>{voice}</option>
        {/each}
      </select>
      <label class="block text-sm text-slate-300">Volume <input class="mt-2 w-full accent-cyan-400" type="range" min="0" max="100" bind:value={volume} /></label>
      <Button color="cyan" onclick={saveVoiceAndModel}>Save Voice + AI</Button>
    </div>
  </Card>

  <Card class="border border-slate-800 bg-slate-900/90">
    <div class="space-y-4">
      <div>
        <p class="text-xs font-semibold uppercase tracking-[0.22em] text-cyan-300">Conversation Rules</p>
        <h3 class="text-lg font-black text-white">Behavior</h3>
      </div>
      <label class="flex items-center justify-between rounded-xl border border-slate-800 bg-slate-950 px-3 py-2 text-sm"><span>Auto comments</span><input type="checkbox" bind:checked={autoComments} /></label>
      <label class="flex items-center justify-between rounded-xl border border-slate-800 bg-slate-950 px-3 py-2 text-sm"><span>Keep talking</span><input type="checkbox" bind:checked={keepTalking} /></label>
      <label class="flex items-center justify-between rounded-xl border border-slate-800 bg-slate-950 px-3 py-2 text-sm"><span>Post to Twitch</span><input type="checkbox" bind:checked={postToTwitch} /></label>
      <label class="block text-sm text-slate-300">Pace <input class="mt-2 w-full accent-cyan-400" type="range" min="0" max="4" step="0.1" bind:value={pace} /></label>
      <Button color="cyan" onclick={saveConversationRules}>Save Behavior</Button>
    </div>
  </Card>

  <Card class="border border-slate-800 bg-slate-900/90">
    <div class="space-y-4">
      <div>
        <p class="text-xs font-semibold uppercase tracking-[0.22em] text-cyan-300">Scene + Backend</p>
        <h3 class="text-lg font-black text-white">Operations</h3>
      </div>
      <select bind:value={sceneMode} class="w-full rounded-xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white">
        <option value="solo">Solo</option>
        <option value="dual_debate">Dual Debate</option>
        <option value="chat_topic">Chat Topic</option>
      </select>
      <label class="block text-sm text-slate-300">Turns before pause <input class="mt-2 w-full accent-cyan-400" type="range" min="1" max="6" bind:value={maxTurnsBeforePause} /></label>
      <label class="flex items-center justify-between rounded-xl border border-slate-800 bg-slate-950 px-3 py-2 text-sm"><span>External topics</span><input type="checkbox" bind:checked={allowExternalTopicChanges} /></label>
      <label class="flex items-center justify-between rounded-xl border border-slate-800 bg-slate-950 px-3 py-2 text-sm"><span>Public call enabled</span><input type="checkbox" bind:checked={publicCallEnabled} /></label>
      <input bind:value={secondaryCharacterSlug} class="w-full rounded-xl border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-white" placeholder="Secondary character slug" />
      <div class="flex gap-2">
        <Button color="cyan" onclick={saveSceneAndApp}>Save Scene</Button>
        <Button color="light" onclick={openCli}>Open CLI</Button>
      </div>
    </div>
  </Card>
</div>
