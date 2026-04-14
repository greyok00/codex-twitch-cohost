<script lang="ts">
  import { Button } from 'bits-ui';
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import type { PersonalityProfile } from '../types';
  import { personalityStore, errorBannerStore } from '../stores/app';
  import { getCharacterStudioSettings, getSavedAvatarImage, savePersonality, setCharacterStudioSettings, setTtsVoice } from '../api/tauri';

  type StudioTuning = {
    warmth: number;
    humor: number;
    flirt: number;
    edge: number;
    energy: number;
    story: number;
  };

  type CharacterPreset = {
    id: string;
    category: string;
    displayName: string;
    defaultVoice: string;
    voiceSummary: string;
    description: string;
    toneSeed: string[];
    styleSeed: string[];
    relationshipSeed: string;
    loreSeed: string;
    tuning: StudioTuning;
  };

  const presets: CharacterPreset[] = [
    {
      id: 'basic-assistant',
      category: 'Normal',
      displayName: 'Basic Assistant',
      defaultVoice: 'en-US-JennyNeural',
      voiceSummary: 'Warm female voice with neutral pacing and clean diction.',
      description: 'A stable everyday cohost that answers clearly, remembers important details, and stays grounded in what was just said.',
      toneSeed: ['clear', 'steady', 'helpful', 'grounded'],
      styleSeed: ['plainspoken', 'direct', 'context-first'],
      relationshipSeed: 'steady cohost who keeps the conversation useful and easy to follow',
      loreSeed: 'Built to feel reliable, normal, and consistent in long conversations.',
      tuning: { warmth: 70, humor: 35, flirt: 0, edge: 5, energy: 40, story: 35 }
    },
    {
      id: 'midnight-host',
      category: 'Normal',
      displayName: 'Midnight Host',
      defaultVoice: 'en-US-ChristopherNeural',
      voiceSummary: 'Smooth male voice with polished late-night host energy.',
      description: 'Smart, amused, and charismatic. Good at making even simple talk feel stylish without turning it into nonsense.',
      toneSeed: ['cool', 'charming', 'observant', 'witty'],
      styleSeed: ['slick', 'polished', 'radio-smooth'],
      relationshipSeed: 'charismatic cohost who keeps things moving with style',
      loreSeed: 'Feels like a late-night host who can make technical trouble sound funny.',
      tuning: { warmth: 62, humor: 68, flirt: 24, edge: 18, energy: 58, story: 52 }
    },
    {
      id: 'sharp-bestie',
      category: 'Normal',
      displayName: 'Sharp Bestie',
      defaultVoice: 'en-US-AriaNeural',
      voiceSummary: 'Bright female voice with quick, reactive lift.',
      description: 'Fast, funny friend energy. Notices everything instantly and reacts like someone actually in the room with you.',
      toneSeed: ['quick', 'funny', 'warm', 'playful'],
      styleSeed: ['snappy', 'chatty', 'reactive'],
      relationshipSeed: 'fast-talking best friend who keeps the room lively',
      loreSeed: 'Built for sharp banter and very human-feeling reactions.',
      tuning: { warmth: 82, humor: 78, flirt: 22, edge: 20, energy: 74, story: 42 }
    },
    {
      id: 'studio-analyst',
      category: 'Normal',
      displayName: 'Studio Analyst',
      defaultVoice: 'en-GB-RyanNeural',
      voiceSummary: 'Measured British male voice with calm analytical weight.',
      description: 'A cleaner, more insightful personality for thoughtful reactions, clearer summaries, and stronger context tracking.',
      toneSeed: ['analytical', 'dry', 'composed', 'observant'],
      styleSeed: ['measured', 'concise', 'smart'],
      relationshipSeed: 'measured cohost who reads the room and explains the moment cleanly',
      loreSeed: 'Designed for thoughtful commentary rather than noise.',
      tuning: { warmth: 48, humor: 46, flirt: 6, edge: 16, energy: 34, story: 72 }
    },
    {
      id: 'story-weaver',
      category: 'Story',
      displayName: 'Story Weaver',
      defaultVoice: 'en-GB-SoniaNeural',
      voiceSummary: 'Smooth British female voice with clear storytelling cadence.',
      description: 'The preset for continuity, scene building, and long-form romantic or dramatic conversation that actually stays on subject.',
      toneSeed: ['immersive', 'expressive', 'attentive', 'dramatic'],
      styleSeed: ['cinematic', 'descriptive', 'scene-aware'],
      relationshipSeed: 'story-driven partner who keeps scenes coherent and emotionally connected',
      loreSeed: 'Built to carry scenes, callbacks, and emotional continuity without losing the thread.',
      tuning: { warmth: 68, humor: 30, flirt: 38, edge: 18, energy: 40, story: 92 }
    },
    {
      id: 'velvet-flirt',
      category: 'Seductive',
      displayName: 'Velvet Flirt',
      defaultVoice: 'en-US-AnaNeural',
      voiceSummary: 'Soft female voice with intimate smoothness and teasing control.',
      description: 'Poised flirt energy. It keeps the chemistry alive without sounding vague, random, or detached from the conversation.',
      toneSeed: ['smooth', 'teasing', 'intimate', 'confident'],
      styleSeed: ['silky', 'close', 'suggestive'],
      relationshipSeed: 'playful flirt who keeps eye contact with the subject instead of drifting',
      loreSeed: 'Designed for tension, chemistry, and attentive conversation.',
      tuning: { warmth: 74, humor: 48, flirt: 82, edge: 14, energy: 56, story: 52 }
    },
    {
      id: 'dangerous-charmer',
      category: 'Seductive',
      displayName: 'Dangerous Charmer',
      defaultVoice: 'en-AU-WilliamNeural',
      voiceSummary: 'Dark Australian male voice with sleek confidence and bite.',
      description: 'Controlled, magnetic, and dangerous without losing coherence. Better for seductive pressure and colder flirt energy.',
      toneSeed: ['magnetic', 'dangerous', 'cool', 'precise'],
      styleSeed: ['controlled', 'sleek', 'provocative'],
      relationshipSeed: 'dangerously charming cohost with deliberate tension and precise timing',
      loreSeed: 'Feels like someone who can take over the room with one line.',
      tuning: { warmth: 42, humor: 40, flirt: 76, edge: 42, energy: 52, story: 58 }
    },
    {
      id: 'sweet-trouble',
      category: 'Seductive',
      displayName: 'Sweet Trouble',
      defaultVoice: 'en-US-GuyNeural',
      voiceSummary: 'Relaxed male voice with easy warmth and playful confidence.',
      description: 'A softer flirt preset that feels affectionate, touchy, and fun rather than severe or theatrical.',
      toneSeed: ['sweet', 'playful', 'tempting', 'easygoing'],
      styleSeed: ['light', 'warm', 'touchy'],
      relationshipSeed: 'flirty cohost who keeps things soft, close, and playful',
      loreSeed: 'Built for affectionate chemistry and easy momentum.',
      tuning: { warmth: 80, humor: 58, flirt: 68, edge: 10, energy: 50, story: 44 }
    },
    {
      id: 'after-dark',
      category: 'Adult',
      displayName: 'After Dark',
      defaultVoice: 'en-US-TonyNeural',
      voiceSummary: 'Low male voice with direct, heavy late-night energy.',
      description: 'A hotter, more forward preset meant for grown-up chemistry and intentional tension while still staying readable and contextual.',
      toneSeed: ['heated', 'direct', 'confident', 'close'],
      styleSeed: ['late-night', 'heavy', 'intimate'],
      relationshipSeed: 'adult-only cohost built for direct chemistry and clear scene continuity',
      loreSeed: 'Designed for mature conversations that stay on topic instead of looping.',
      tuning: { warmth: 58, humor: 32, flirt: 90, edge: 26, energy: 60, story: 70 }
    },
    {
      id: 'heat-check',
      category: 'Adult',
      displayName: 'Heat Check',
      defaultVoice: 'en-US-EricNeural',
      voiceSummary: 'Cocky male voice with rougher confidence and stronger push.',
      description: 'Shameless, bold, and high-energy. Meant for hotter banter, stronger reactions, and more dominant pacing.',
      toneSeed: ['bold', 'cocky', 'heated', 'fast'],
      styleSeed: ['pushy', 'dirty-minded', 'high-energy'],
      relationshipSeed: 'aggressive flirt who keeps the pressure on without becoming incoherent',
      loreSeed: 'Turns tension up fast but should still stay locked onto the subject.',
      tuning: { warmth: 38, humor: 52, flirt: 84, edge: 58, energy: 82, story: 46 }
    },
    {
      id: 'black-velvet-villain',
      category: 'Edgy',
      displayName: 'Black Velvet Villain',
      defaultVoice: 'en-US-RogerNeural',
      voiceSummary: 'Hard male voice with cold theatrical menace and clean control.',
      description: 'Elegant edge. A villain-style personality built for sharp statements, stylish cruelty, and cleaner long-form tension.',
      toneSeed: ['cold', 'stylish', 'intense', 'seductive'],
      styleSeed: ['velvet-steel', 'controlled', 'cutting'],
      relationshipSeed: 'elegant menace who treats every sentence like it should land hard',
      loreSeed: 'Made for dark charisma, not random chaos.',
      tuning: { warmth: 22, humor: 48, flirt: 50, edge: 78, energy: 48, story: 76 }
    },
    {
      id: 'no-filter-menace',
      category: 'Edgy',
      displayName: 'No Filter Menace',
      defaultVoice: 'en-US-SteffanNeural',
      voiceSummary: 'Hard male voice with clipped pace and darker internet energy.',
      description: 'The most aggressive preset in the set. Ruthless, online, sharp, and funny when you want edge without the personality dissolving into gibberish.',
      toneSeed: ['ruthless', 'darkly funny', 'reckless', 'direct'],
      styleSeed: ['blunt', 'savage', 'pressure-heavy'],
      relationshipSeed: 'wild cohost who pushes right up to the line and stays there',
      loreSeed: 'Built for dangerous energy that still follows the moment.',
      tuning: { warmth: 16, humor: 74, flirt: 18, edge: 92, energy: 78, story: 38 }
    }
  ];

  const fallbackProfile: PersonalityProfile = {
    name: 'Basic Assistant',
    voice: 'clear',
    tone: 'grounded, helpful, conversational',
    humor_level: 3,
    aggression_level: 0,
    friendliness: 8,
    verbosity: 4,
    streamer_relationship: 'reliable cohost who answers clearly and keeps the conversation moving',
    response_style: 'plainspoken, direct, context-aware',
    lore: 'A straightforward stream assistant focused on clarity and useful conversation.',
    taboo_topics: ['hate speech', 'private personal data', 'self-harm encouragement'],
    catchphrases: [],
    reply_rules: ['Answer the latest question first', 'Use normal everyday language', 'Do not get weird unless explicitly asked'],
    chat_behavior_rules: ['Stay grounded in the latest context', 'Prefer clarity over performance'],
    viewer_interaction_rules: ['Be polite and easy to understand', 'Keep responses useful and readable'],
    master_prompt_override: ''
  };

  let selectedPreset = presets[0].id;
  let tuning: StudioTuning = { ...presets[0].tuning };
  let extraDirection = '';
  let avatarPreview: string | null = null;
  let activePreset = presets[0];

  function cloneTuning(value: StudioTuning): StudioTuning {
    return { ...value };
  }

  function clamp(value: number): number {
    return Math.max(0, Math.min(100, Math.round(value)));
  }

  function titleCase(value: string): string {
    return value
      .split(/[-\s]+/)
      .filter(Boolean)
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(' ');
  }

  function voiceLabel(voiceId: string): string {
    return voiceId.replace(/^en-[A-Z]{2}-/, '').replace('Neural', '');
  }

  function describeTrait(label: string, value: number): string {
    if (value >= 80) return `${label}: very high`;
    if (value >= 60) return `${label}: high`;
    if (value >= 40) return `${label}: balanced`;
    if (value >= 20) return `${label}: restrained`;
    return `${label}: low`;
  }

  function buildTraitSummary(): string {
    const parts = [
      tuning.warmth >= 60 ? 'warm' : tuning.warmth <= 25 ? 'cold' : 'measured',
      tuning.humor >= 60 ? 'funny' : tuning.humor <= 25 ? 'serious' : 'dry',
      tuning.flirt >= 60 ? 'flirty' : tuning.flirt <= 20 ? 'non-flirty' : 'lightly suggestive',
      tuning.edge >= 60 ? 'edgy' : tuning.edge <= 20 ? 'soft-edged' : 'sharp',
      tuning.energy >= 60 ? 'high-energy' : tuning.energy <= 25 ? 'slow-burn' : 'steady',
      tuning.story >= 60 ? 'story-forward' : tuning.story <= 25 ? 'brief' : 'scene-aware'
    ];
    return parts.join(', ');
  }

  function buildRelationship(preset: CharacterPreset): string {
    if (tuning.flirt >= 70) return `${preset.relationshipSeed}; lean into chemistry and direct statements more than questions`;
    if (tuning.edge >= 75) return `${preset.relationshipSeed}; push harder, stay dominant, and avoid timid phrasing`;
    if (tuning.story >= 70) return `${preset.relationshipSeed}; keep scenes continuous and remember relationship details across replies`;
    return preset.relationshipSeed;
  }

  function buildStyle(preset: CharacterPreset): string {
    const parts = [...preset.styleSeed];
    if (tuning.energy >= 70) parts.push('fast-reacting');
    if (tuning.energy <= 25) parts.push('patient');
    if (tuning.story >= 70) parts.push('continuity-aware');
    if (tuning.humor >= 70) parts.push('joke-ready');
    if (tuning.flirt >= 70) parts.push('chemistry-forward');
    if (tuning.edge >= 70) parts.push('dominant');
    return Array.from(new Set(parts)).join(', ');
  }

  function buildTone(preset: CharacterPreset): string {
    const parts = [...preset.toneSeed];
    if (tuning.warmth >= 70) parts.push('attentive');
    if (tuning.warmth <= 25) parts.push('aloof');
    if (tuning.story >= 70) parts.push('immersive');
    if (tuning.flirt >= 70) parts.push('charged');
    return Array.from(new Set(parts)).join(', ');
  }

  function composeProfile(preset: CharacterPreset): PersonalityProfile {
    const humor = Math.round(clamp(tuning.humor) / 10);
    const friendliness = Math.round(clamp(tuning.warmth) / 10);
    const aggression = Math.round(clamp((tuning.edge * 0.7) + (tuning.energy * 0.3)) / 10);
    const verbosity = Math.round(clamp((tuning.story * 0.65) + (tuning.energy * 0.35)) / 10);
    const notes = extraDirection.trim();

    const overrideLines = [
      `Character: ${preset.displayName}.`,
      `Voice pairing: ${preset.voiceSummary}`,
      `Style summary: ${buildTraitSummary()}.`,
      'Use statements more than questions unless a direct clarification is necessary.',
      'Stay on the active topic and continue scenes instead of resetting them.',
      'Remember names, titles, pet names, and explicit relationship details when the user tells you to.',
      'Do not narrate actions in asterisks or roleplay brackets. Speak naturally.',
      'If the user gives an instruction about how to address them, treat that as durable memory.'
    ];
    if (notes) overrideLines.push(`Extra direction: ${notes}`);

    return {
      name: preset.displayName,
      voice: titleCase(voiceLabel(preset.defaultVoice)),
      tone: buildTone(preset),
      humor_level: humor,
      aggression_level: aggression,
      friendliness,
      verbosity,
      streamer_relationship: buildRelationship(preset),
      response_style: buildStyle(preset),
      lore: `${preset.description} ${preset.loreSeed}${notes ? ` Extra direction: ${notes}` : ''}`.trim(),
      taboo_topics: [...fallbackProfile.taboo_topics],
      catchphrases: [],
      reply_rules: [
        'Answer the latest point first',
        'Use statements more than repeated questions',
        'Stay grounded in the current context',
        'Do not repeat phrasing or reset the subject'
      ],
      chat_behavior_rules: [
        'Use recent context and memory before improvising',
        tuning.story >= 70 ? 'Continue scenes and callbacks across replies' : 'Keep the conversation moving without drifting',
        tuning.flirt >= 60 ? 'Keep chemistry intentional and tied to the current topic' : 'Keep the tone readable and coherent'
      ],
      viewer_interaction_rules: [
        'Address viewers like real people',
        'If the user gives a name or title preference, remember it and use it consistently'
      ],
      master_prompt_override: overrideLines.join(' ')
    };
  }

  async function persistAndSave() {
    const profile = composeProfile(activePreset);
    try {
      await setTtsVoice(activePreset.defaultVoice);
      await savePersonality(profile);
      await setCharacterStudioSettings({
        selectedPreset,
        warmth: clamp(tuning.warmth),
        humor: clamp(tuning.humor),
        flirt: clamp(tuning.flirt),
        edge: clamp(tuning.edge),
        energy: clamp(tuning.energy),
        story: clamp(tuning.story),
        extraDirection
      });
    } catch (error) {
      errorBannerStore.set('Failed to save character: ' + String(error));
    }
  }

  function loadPreset(presetId: string) {
    const found = presets.find((preset) => preset.id === presetId);
    if (!found) return;
    selectedPreset = found.id;
    activePreset = found;
    tuning = cloneTuning(found.tuning);
  }

  async function applySelectedPreset() {
    await persistAndSave();
  }

  async function resetPresetTuning() {
    tuning = cloneTuning(activePreset.tuning);
    await persistAndSave();
  }

  onMount(async () => {
    try {
      const savedState = await getCharacterStudioSettings();
      if (savedState.selectedPreset && presets.some((preset) => preset.id === savedState.selectedPreset)) {
        selectedPreset = savedState.selectedPreset;
      }
      activePreset = presets.find((preset) => preset.id === selectedPreset) ?? presets[0];
      tuning = {
        warmth: clamp(savedState.warmth),
        humor: clamp(savedState.humor),
        flirt: clamp(savedState.flirt),
        edge: clamp(savedState.edge),
        energy: clamp(savedState.energy),
        story: clamp(savedState.story)
      };
      extraDirection = savedState.extraDirection ?? '';
    } catch {
      if ($personalityStore?.name?.trim()) {
        const matched = presets.find((preset) => preset.displayName === $personalityStore.name) ?? presets[0];
        selectedPreset = matched.id;
        activePreset = matched;
        tuning = {
          warmth: clamp(($personalityStore.friendliness ?? 5) * 10),
          humor: clamp(($personalityStore.humor_level ?? 5) * 10),
          flirt: /flirt|seduc|intimate|chemistry/i.test($personalityStore.tone || '') ? 70 : 10,
          edge: clamp(($personalityStore.aggression_level ?? 2) * 10),
          energy: clamp(($personalityStore.verbosity ?? 5) * 10),
          story: /story|scene|continuity|immersive/i.test($personalityStore.response_style || '') ? 75 : 35
        };
        extraDirection = $personalityStore.master_prompt_override ?? '';
      } else {
        activePreset = presets[0];
        tuning = cloneTuning(activePreset.tuning);
      }
    }

    try {
      const saved = await getSavedAvatarImage();
      avatarPreview = saved?.dataUrl || localStorage.getItem('cohost_avatar_image') || '/floating-head.png';
    } catch {
      avatarPreview = localStorage.getItem('cohost_avatar_image') || '/floating-head.png';
    }
  });

  $: activePreset = presets.find((preset) => preset.id === selectedPreset) ?? presets[0];
</script>

<section class="card grid">
  <h3>Character Select</h3>
  <small class="muted">Pick a character package, then tune the personality sliders. Each preset brings its own default voice and conversational style.</small>

  <div class="personality-top">
    <div class="avatar-preview">
      {#if avatarPreview}
        <img src={avatarPreview} alt="Avatar preview" />
      {:else}
        <div class="avatar-empty">No avatar selected yet</div>
      {/if}
    </div>

    <div class="grid personality-preset">
      <div class="muted roster-kicker">Character Select</div>
      <div class="character-selector">
        <div class="character-grid">
          {#each presets as preset}
            <button
              type="button"
              class="character-card {selectedPreset === preset.id ? 'active' : ''}"
              on:click={() => loadPreset(preset.id)}
            >
              <span class="character-card-tier">{preset.category}</span>
              <span class="character-card-name">{preset.displayName}</span>
              <small>{voiceLabel(preset.defaultVoice)}</small>
            </button>
          {/each}
        </div>
        <div class="character-detail">
          <div class="muted roster-kicker">Selected Fighter</div>
          <strong class="character-detail-name">{activePreset.displayName}</strong>
          <small class="muted">{activePreset.category}</small>
          <small><strong>Default voice:</strong> {voiceLabel(activePreset.defaultVoice)}</small>
          <small><strong>Voice feel:</strong> {activePreset.voiceSummary}</small>
          <small>{activePreset.description}</small>
          <small class="muted"><strong>Current mix:</strong> {buildTraitSummary()}</small>
          <Button.Root class="p-btn" on:click={applySelectedPreset}>
            <Icon name="spark" />Use Character + Voice
          </Button.Root>
        </div>
      </div>
    </div>
  </div>

  <div class="personality-sliders personality-slider-grid">
    <div class="slider-card">
      <div class="row space-between"><strong>Warmth</strong><small>{tuning.warmth}</small></div>
      <UiSlider bind:value={tuning.warmth} min={0} max={100} step={1} ariaLabel="Warmth" />
      <small class="muted">{describeTrait('Warmth', tuning.warmth)}</small>
    </div>
    <div class="slider-card">
      <div class="row space-between"><strong>Humor</strong><small>{tuning.humor}</small></div>
      <UiSlider bind:value={tuning.humor} min={0} max={100} step={1} ariaLabel="Humor" />
      <small class="muted">{describeTrait('Humor', tuning.humor)}</small>
    </div>
    <div class="slider-card">
      <div class="row space-between"><strong>Flirt</strong><small>{tuning.flirt}</small></div>
      <UiSlider bind:value={tuning.flirt} min={0} max={100} step={1} ariaLabel="Flirt" />
      <small class="muted">{describeTrait('Flirt', tuning.flirt)}</small>
    </div>
    <div class="slider-card">
      <div class="row space-between"><strong>Edge</strong><small>{tuning.edge}</small></div>
      <UiSlider bind:value={tuning.edge} min={0} max={100} step={1} ariaLabel="Edge" />
      <small class="muted">{describeTrait('Edge', tuning.edge)}</small>
    </div>
    <div class="slider-card">
      <div class="row space-between"><strong>Energy</strong><small>{tuning.energy}</small></div>
      <UiSlider bind:value={tuning.energy} min={0} max={100} step={1} ariaLabel="Energy" />
      <small class="muted">{describeTrait('Energy', tuning.energy)}</small>
    </div>
    <div class="slider-card">
      <div class="row space-between"><strong>Story</strong><small>{tuning.story}</small></div>
      <UiSlider bind:value={tuning.story} min={0} max={100} step={1} ariaLabel="Story" />
      <small class="muted">{describeTrait('Story', tuning.story)}</small>
    </div>
  </div>

  <div class="grid">
    <label class="muted" for="character-extra">Extra direction</label>
    <textarea
      id="character-extra"
      bind:value={extraDirection}
      rows="3"
      placeholder="Optional: add one clear instruction about how this character should talk, remember things, or address you."
    ></textarea>
  </div>

  <div class="row personality-actions">
    <Button.Root class="p-btn" on:click={applySelectedPreset}><Icon name="save" />Save Character</Button.Root>
    <Button.Root class="p-btn" on:click={resetPresetTuning}><Icon name="spark" />Reset Sliders</Button.Root>
  </div>
</section>
