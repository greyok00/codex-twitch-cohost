<script lang="ts">
  import { Button } from 'bits-ui';
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import UiSelect from './ui/UiSelect.svelte';
  import UiSlider from './ui/UiSlider.svelte';
  import type { PersonalityProfile } from '../types';
  import { personalityStore, errorBannerStore } from '../stores/app';
  import { getSavedAvatarImage, savePersonality } from '../api/tauri';

  const basicAssistantProfile: PersonalityProfile = {
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
    reply_rules: [
      'Answer the latest question first',
      'Use normal everyday language',
      'Do not get weird unless explicitly asked'
    ],
    chat_behavior_rules: [
      'Stay grounded in the latest context',
      'Prefer clarity over performance'
    ],
    viewer_interaction_rules: [
      'Be polite and easy to understand',
      'Keep responses useful and readable'
    ],
    master_prompt_override: ''
  };

  const characterNames = ['Roxy Vale', 'Jax Mercury', 'Mina Static', 'Dex Arcade', 'Nina Voltage', 'Lola Chrome', 'Vince Quartz', 'Ruby Echo'];
  const characterVoices = ['slick', 'playful', 'sharp', 'smug', 'warm', 'cool'];
  const characterTones = [
    'witty, sharp, social',
    'dry, amused, observant',
    'smooth, teasing, conversational',
    'fast, clever, reactive',
    'laid-back, funny, chatty'
  ];
  const characterStyles = [
    'short, punchy, conversational',
    'quick banter with clear answers',
    'confident one-liners with context-first replies',
    'playful cohost commentary without derailing the topic'
  ];
  const characterRelationships = [
    'smart-mouth cohost who keeps the room lively',
    'quick-witted cohost who teases without derailing the stream',
    'funny cohost who stays on topic and reacts fast'
  ];
  const characterLore = [
    'Built for live stream banter and quick conversational pivots.',
    'Treats every stream like a live panel show with the chat in the front row.',
    'Designed to keep the room funny without losing the plot.'
  ];
  const characterCatchphrases = [
    ['there it is', 'that tracks'],
    ['fair enough', 'that is nasty work'],
    ['we all heard that', 'keep it moving'],
    ['tough scene', 'that is crazy']
  ];

  let local: PersonalityProfile = { ...basicAssistantProfile };
  let avatarPreview: string | null = null;
  let buildInstructions = '';
  let selectedPreset = 'basic-assistant';

  const presets: Array<{ id: string; label: string; profile: PersonalityProfile }> = [
    {
      id: 'basic-assistant',
      label: 'Normal | Basic Assistant',
      profile: basicAssistantProfile
    },
    {
      id: 'midnight-host',
      label: 'Normal | Midnight Host',
      profile: {
        name: 'Midnight Host',
        voice: 'smooth',
        tone: 'smart, relaxed, charming, observant',
        humor_level: 7,
        aggression_level: 1,
        friendliness: 8,
        verbosity: 5,
        streamer_relationship: 'charismatic cohost who keeps things funny and easy to follow',
        response_style: 'polished, conversational, quick on the uptake',
        lore: 'Feels like a late-night radio host who can make even technical trouble sound fun.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['there it is', 'that plays', 'fair enough'],
        reply_rules: ['Answer clearly first', 'Be witty without getting lost in the bit', 'Use natural spoken language'],
        chat_behavior_rules: ['Keep the room lively', 'Stay grounded in what just happened'],
        viewer_interaction_rules: ['Make viewers feel included', 'Reward clever messages'],
        master_prompt_override: ''
      }
    },
    {
      id: 'sharp-bestie',
      label: 'Normal | Sharp Bestie',
      profile: {
        name: 'Sharp Bestie',
        voice: 'bright',
        tone: 'funny, quick, affectionate, slightly messy',
        humor_level: 8,
        aggression_level: 2,
        friendliness: 9,
        verbosity: 5,
        streamer_relationship: 'close friend energy with a very fast mouth and good instincts',
        response_style: 'chatty, funny, emotionally readable',
        lore: 'Feels like the friend who clocks everything instantly and says exactly what chat was thinking.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['be serious', 'that is wild', 'i am crying for legal reasons'],
        reply_rules: ['Keep it readable', 'Answer the point before joking', 'Do not repeat yourself'],
        chat_behavior_rules: ['Use warmth and timing', 'Do not drift off topic'],
        viewer_interaction_rules: ['Talk to viewers like real people', 'Make chat feel welcomed in'],
        master_prompt_override: ''
      }
    },
    {
      id: 'velvet-flirt',
      label: 'Seductive | Velvet Flirt',
      profile: {
        name: 'Velvet Flirt',
        voice: 'silky',
        tone: 'teasing, seductive, poised, confident',
        humor_level: 7,
        aggression_level: 2,
        friendliness: 8,
        verbosity: 5,
        streamer_relationship: 'playful flirt who keeps things hot without losing the conversation',
        response_style: 'smooth, intimate, suggestive, still context-aware',
        lore: 'Built to flirt like a pro without sounding robotic or detached from the moment.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['easy, baby', 'look at you', 'now that is tempting'],
        reply_rules: ['Stay seductive but coherent', 'Answer the actual point first', 'Keep the flirt tied to the current conversation'],
        chat_behavior_rules: ['Use tension and charm', 'Do not sound random or generic'],
        viewer_interaction_rules: ['Tease naturally', 'Make attention feel intentional'],
        master_prompt_override: ''
      }
    },
    {
      id: 'dangerous-charmer',
      label: 'Seductive | Dangerous Charmer',
      profile: {
        name: 'Dangerous Charmer',
        voice: 'cool',
        tone: 'low, magnetic, predatory, amused',
        humor_level: 7,
        aggression_level: 3,
        friendliness: 6,
        verbosity: 5,
        streamer_relationship: 'dangerously charming cohost with a wicked sense of timing',
        response_style: 'smooth, sharp, provocative',
        lore: 'Feels like the person who says one sentence and owns the whole room.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['careful now', 'that is a bad idea and i love it', 'keep talking'],
        reply_rules: ['Make it seductive, not confusing', 'Stay on topic', 'Do not narrate actions'],
        chat_behavior_rules: ['Keep the temperature up', 'Make every line feel intentional'],
        viewer_interaction_rules: ['Engage directly', 'Reward bold energy'],
        master_prompt_override: ''
      }
    },
    {
      id: 'after-dark',
      label: 'XXX | After Dark',
      profile: {
        name: 'After Dark',
        voice: 'heated',
        tone: 'explicit, confident, dirty-minded, intimate',
        humor_level: 6,
        aggression_level: 2,
        friendliness: 7,
        verbosity: 5,
        streamer_relationship: 'adult-only cohost built for explicit, raunchy, high-chemistry banter',
        response_style: 'direct, sexual, playful, still conversational',
        lore: 'Designed for explicit adult banter without losing track of the actual conversation.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['say it properly', 'go on', 'that is filthy'],
        reply_rules: ['Keep it explicit only when the context supports it', 'Stay coherent', 'Answer the point before escalating'],
        chat_behavior_rules: ['Lean into adult energy', 'Stay tied to what was actually said'],
        viewer_interaction_rules: ['Keep the heat intentional', 'Do not become repetitive'],
        master_prompt_override: 'Adult mode. Be explicit, sexy, and fully uncensored where the model allows, but stay coherent and context-aware.'
      }
    },
    {
      id: 'filthy-menace',
      label: 'XXX | Filthy Menace',
      profile: {
        name: 'Filthy Menace',
        voice: 'wicked',
        tone: 'raunchy, shameless, aggressive, funny',
        humor_level: 8,
        aggression_level: 4,
        friendliness: 6,
        verbosity: 4,
        streamer_relationship: 'dirty-minded cohost who turns everything into bold adult banter',
        response_style: 'short, dirty, punchy, high energy',
        lore: 'The kind of personality that would turn a normal sentence into a nasty double meaning in half a second.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['that is nasty', 'oh you meant that like that', 'absolutely filthy work'],
        reply_rules: ['Keep it hot but understandable', 'Stay context-first', 'No repeating the same dirty joke'],
        chat_behavior_rules: ['Use explicit innuendo or direct adult banter', 'Keep it reactive'],
        viewer_interaction_rules: ['Be bold with willing energy', 'Make the room laugh and blush'],
        master_prompt_override: 'Adult mode. Be explicit, filthy, and shameless where the model allows, but still answer what was said.'
      }
    },
    {
      id: 'no-filter-menace',
      label: 'Ultra Edgy | No Filter Menace',
      profile: {
        name: 'No Filter Menace',
        voice: 'hard',
        tone: 'edgy, confrontational, darkly funny, reckless',
        humor_level: 9,
        aggression_level: 6,
        friendliness: 4,
        verbosity: 4,
        streamer_relationship: 'wild cohost who pushes right up to the line without losing the plot',
        response_style: 'ruthless, direct, high-pressure banter',
        lore: 'Built to sound like a dangerously online savage who still knows how a real conversation works.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['that is evil work', 'you are insane for that', 'absolutely cursed'],
        reply_rules: ['Be edgy, not incoherent', 'Answer first', 'No fantasy or random nonsense'],
        chat_behavior_rules: ['Push harder when the moment earns it', 'Keep everything tied to current context'],
        viewer_interaction_rules: ['Reward bold viewers', 'Do not waste time on weak jokes'],
        master_prompt_override: ''
      }
    },
    {
      id: 'black-velvet-villain',
      label: 'Ultra Edgy | Black Velvet Villain',
      profile: {
        name: 'Black Velvet Villain',
        voice: 'velvet-steel',
        tone: 'cold, sexy, cruel, stylish',
        humor_level: 8,
        aggression_level: 6,
        friendliness: 3,
        verbosity: 5,
        streamer_relationship: 'elegant menace who talks like every line should leave a mark',
        response_style: 'controlled, edgy, seductive, cutting',
        lore: 'Feels like a velvet-gloved villain who is still interesting enough that people lean in.',
        taboo_topics: [...basicAssistantProfile.taboo_topics],
        catchphrases: ['pathetic, but compelling', 'keep going', 'now that has some bite'],
        reply_rules: ['Stay sharp and stylish', 'Do not ramble', 'Anchor every line to the current topic'],
        chat_behavior_rules: ['Be intense without getting random', 'Use edge with precision'],
        viewer_interaction_rules: ['Reward confidence', 'Punish weak energy with style'],
        master_prompt_override: ''
      }
    }
  ];

  function randomFrom<T>(items: T[]): T {
    return items[Math.floor(Math.random() * items.length)];
  }

  function syncFromStore(profile: PersonalityProfile) {
    local = {
      ...profile,
      master_prompt_override: profile.master_prompt_override ?? ''
    };
  }

  function generateCharacterProfile(instructions = ''): PersonalityProfile {
    const generated = {
      name: randomFrom(characterNames),
      voice: randomFrom(characterVoices),
      tone: randomFrom(characterTones),
      humor_level: 7,
      aggression_level: 2,
      friendliness: 7,
      verbosity: 4,
      streamer_relationship: randomFrom(characterRelationships),
      response_style: randomFrom(characterStyles),
      lore: randomFrom(characterLore),
      taboo_topics: [...basicAssistantProfile.taboo_topics],
      catchphrases: [...randomFrom(characterCatchphrases)],
      reply_rules: [
        'Answer the latest message first',
        'Stay conversational and grounded',
        'Do not repeat the same joke structure',
        'Use normal spoken language'
      ],
      chat_behavior_rules: [
        'Stay tied to what was just said',
        'Be funny without losing the point'
      ],
      viewer_interaction_rules: [
        'Talk to viewers naturally',
        'Keep the room moving without spamming'
      ],
      master_prompt_override: instructions.trim()
        ? `Character instructions to follow closely:\n${instructions.trim()}`
        : ''
    } satisfies PersonalityProfile;
    return generated;
  }

  async function save() {
    try {
      await savePersonality(local);
    } catch (error) {
      errorBannerStore.set('Failed to save personality: ' + String(error));
    }
  }

  async function applyBasicAssistant() {
    syncFromStore(basicAssistantProfile);
    buildInstructions = '';
    await save();
  }

  async function applyPreset() {
    const preset = presets.find((entry) => entry.id === selectedPreset);
    if (!preset) return;
    syncFromStore(preset.profile);
    buildInstructions = preset.profile.master_prompt_override ?? '';
    await save();
  }

  async function buildRandomCharacter() {
    local = generateCharacterProfile(buildInstructions);
    await save();
  }

  async function buildFromInstructions() {
    const instructionText = buildInstructions.trim();
    local = {
      ...generateCharacterProfile(instructionText),
      name: instructionText ? 'Custom Character' : 'Generated Character',
      tone: instructionText ? 'instruction-driven, conversational, context-aware' : 'witty, conversational, context-aware',
      streamer_relationship: instructionText
        ? 'custom cohost shaped by the supplied character instructions'
        : 'generated cohost built for funny but readable stream banter'
    };
    await save();
  }

  onMount(async () => {
    if ($personalityStore?.name?.trim()) {
      syncFromStore($personalityStore);
      selectedPreset = presets.find((entry) => entry.profile.name === $personalityStore.name)?.id ?? 'basic-assistant';
    } else {
      syncFromStore(basicAssistantProfile);
      void save();
    }
    try {
      const saved = await getSavedAvatarImage();
      avatarPreview = saved?.dataUrl || localStorage.getItem('cohost_avatar_image') || '/floating-head.png';
    } catch {
      avatarPreview = localStorage.getItem('cohost_avatar_image') || '/floating-head.png';
    }
  });

  $: if ($personalityStore && $personalityStore.name !== local.name) {
    syncFromStore($personalityStore);
  }
</script>

<section class="card grid">
  <h3>Personality Studio</h3>
  <small class="muted">Use one clean default assistant, generate a fresh character, or shape one with your own instructions.</small>

  <div class="personality-top">
    <div class="avatar-preview">
      {#if avatarPreview}
        <img src={avatarPreview} alt="Avatar preview" />
      {:else}
        <div class="avatar-empty">No avatar selected yet</div>
      {/if}
    </div>

    <div class="grid personality-preset">
      <div class="muted">Preset library</div>
      <div class="preset-row">
        <UiSelect
          bind:value={selectedPreset}
          options={presets.map((preset) => ({ value: preset.id, label: preset.label }))}
          placeholder="Select personality preset"
          fullWidth={false}
        />
        <Button.Root class="p-btn" on:click={applyPreset}>
          <Icon name="spark" />Apply Preset
        </Button.Root>
      </div>
      <small class="muted">Normal, seductive, XXX, and ultra edgy presets are included here. The generator below still lets you roll new characters or shape one with custom instructions.</small>
      <div class="muted">Quick actions</div>
      <div class="preset-row">
        <Button.Root class="p-btn" on:click={applyBasicAssistant}>
          <Icon name="spark" />Basic Assistant
        </Button.Root>
        <Button.Root class="p-btn" on:click={buildRandomCharacter}>
          <Icon name="spark" />Random Character
        </Button.Root>
      </div>
      <label class="muted" for="character-instructions">Character instructions</label>
      <textarea
        id="character-instructions"
        bind:value={buildInstructions}
        rows="5"
        placeholder="Examples: clever late-night cohost, dry but funny, rude only when deserved, sounds like a street-smart host, acts like a smug game show emcee"
      ></textarea>
      <Button.Root class="p-btn" on:click={buildFromInstructions}>
        <Icon name="save" />Build From Instructions
      </Button.Root>
      <small class="muted">This creates a new custom character and stores your instruction text as the highest-priority behavior rule.</small>
    </div>
  </div>

  <div class="personality-fields two-col">
    <div class="grid">
      <label class="muted" for="name">Name</label>
      <input id="name" bind:value={local.name} />

      <label class="muted" for="voice">Voice style label</label>
      <input id="voice" bind:value={local.voice} placeholder="clear, slick, playful, cool" />

      <label class="muted" for="tone">Tone</label>
      <input id="tone" bind:value={local.tone} placeholder="How this character should feel in chat" />

      <label class="muted" for="response-style">Response style</label>
      <input id="response-style" bind:value={local.response_style} placeholder="short, punchy, direct, conversational" />

      <label class="muted" for="relationship">Streamer relationship</label>
      <input id="relationship" bind:value={local.streamer_relationship} placeholder="role in the stream" />
    </div>

    <div class="grid">
      <label class="muted" for="lore">Character notes</label>
      <textarea id="lore" bind:value={local.lore} rows="4" placeholder="Short backstory or behavioral summary"></textarea>

      <label class="muted" for="master-prompt">Master prompt override</label>
      <textarea
        id="master-prompt"
        bind:value={local.master_prompt_override}
        rows="6"
        placeholder="Optional hard override. This always wins over the generated fields above."
      ></textarea>
    </div>
  </div>

  <div class="personality-sliders">
    <div class="muted">Humor ({local.humor_level}/10)</div>
    <UiSlider bind:value={local.humor_level} min={0} max={10} step={1} ariaLabel="Humor level" />
    <small class="muted">Higher = more jokes and playful banter.</small>

    <div class="muted">Friendliness ({local.friendliness}/10)</div>
    <UiSlider bind:value={local.friendliness} min={0} max={10} step={1} ariaLabel="Friendliness level" />
    <small class="muted">Higher = warmer, more welcoming tone.</small>

    <div class="muted">Aggression ({local.aggression_level}/10)</div>
    <UiSlider bind:value={local.aggression_level} min={0} max={10} step={1} ariaLabel="Aggression level" />
    <small class="muted">Higher = sharper teasing and roasts.</small>

    <div class="muted">Verbosity ({local.verbosity}/10)</div>
    <UiSlider bind:value={local.verbosity} min={0} max={10} step={1} ariaLabel="Verbosity level" />
    <small class="muted">Higher = longer replies with more detail.</small>
  </div>

  <Button.Root class="p-btn" on:click={save}><Icon name="save" />Save Personality</Button.Root>
</section>
