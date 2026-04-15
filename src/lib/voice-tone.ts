import type { CharacterStudioSettings, PersonalityProfile } from '../frontend-types';

export type VoicePreset = {
  id: string;
  displayName: string;
  defaultVoice: string;
  voiceSummary: string;
};

export type DeliveryPreset = {
  id: string;
  label: string;
  summary: string;
  warmth: number;
  humor: number;
  flirt: number;
  edge: number;
  energy: number;
  story: number;
};

export const defaultToneStudioSettings: CharacterStudioSettings = {
  selectedPreset: 'emma',
  warmth: 55,
  humor: 35,
  flirt: 10,
  edge: 15,
  energy: 60,
  story: 40,
  profanityAllowed: false,
  extraDirection: ''
};

export const voicePresets: VoicePreset[] = [
  { id: 'emma', displayName: 'Emma', defaultVoice: 'en-US-EmmaNeural', voiceSummary: 'Balanced female voice with a clean lower-mid tone.' },
  { id: 'jenny', displayName: 'Jenny', defaultVoice: 'en-US-JennyNeural', voiceSummary: 'Warm female voice with clear, natural host pacing.' },
  { id: 'aria', displayName: 'Aria', defaultVoice: 'en-US-AriaNeural', voiceSummary: 'Smoother female voice with a more polished broadcast feel.' },
  { id: 'guy', displayName: 'Guy', defaultVoice: 'en-US-GuyNeural', voiceSummary: 'Relaxed male voice with warm conversational pacing.' },
  { id: 'roger', displayName: 'Roger', defaultVoice: 'en-US-RogerNeural', voiceSummary: 'Deeper male voice with more weight and lower presence.' }
];

export const deliveryPresets: DeliveryPreset[] = [
  { id: 'natural', label: 'Natural', summary: 'Plain conversational co-hosting with balanced humor and steady energy.', warmth: 55, humor: 35, flirt: 10, edge: 15, energy: 60, story: 40 },
  { id: 'warm', label: 'Warm', summary: 'Friendlier and softer, with calmer replies and less bite.', warmth: 78, humor: 30, flirt: 12, edge: 8, energy: 46, story: 42 },
  { id: 'playful', label: 'Playful', summary: 'Livelier banter, more jokes, and more obvious co-host personality.', warmth: 62, humor: 72, flirt: 18, edge: 18, energy: 78, story: 45 },
  { id: 'dry', label: 'Dry Wit', summary: 'More understated and sarcastic without going fully mean.', warmth: 42, humor: 58, flirt: 4, edge: 34, energy: 48, story: 35 },
  { id: 'sharp', label: 'Sharp', summary: 'More aggressive uncensored edge with direct, punchier lines.', warmth: 34, humor: 54, flirt: 6, edge: 72, energy: 72, story: 28 },
  { id: 'storyteller', label: 'Storyteller', summary: 'More vivid, contextual, and willing to carry a thread forward.', warmth: 60, humor: 36, flirt: 8, edge: 16, energy: 58, story: 82 }
];

export function voiceLabel(voiceId: string) {
  return voiceId.replace(/^en-[A-Z]{2}-/, '').replace('Neural', '');
}

export function clampToneScore(value: number) {
  return Math.max(0, Math.min(100, Math.round(value)));
}

export function findVoicePresetByVoice(voiceName?: string | null) {
  return voicePresets.find((preset) => preset.defaultVoice === voiceName) ?? voicePresets[0];
}

export function findVoicePresetById(id?: string | null) {
  return voicePresets.find((preset) => preset.id === id) ?? voicePresets[0];
}

export function findDeliveryPreset(character: CharacterStudioSettings) {
  return deliveryPresets.find((preset) =>
    preset.warmth === character.warmth
    && preset.humor === character.humor
    && preset.flirt === character.flirt
    && preset.edge === character.edge
    && preset.energy === character.energy
    && preset.story === character.story
  ) ?? deliveryPresets[0];
}

export function applyDeliveryPreset(
  presetId: string,
  current: CharacterStudioSettings
): CharacterStudioSettings {
  const preset = deliveryPresets.find((entry) => entry.id === presetId) ?? deliveryPresets[0];
  return {
    ...current,
    warmth: preset.warmth,
    humor: preset.humor,
    flirt: preset.flirt,
    edge: preset.edge,
    energy: preset.energy,
    story: preset.story
  };
}

export function composeDirectProfile(character: CharacterStudioSettings, voiceName: string): PersonalityProfile {
  const warmth = clampToneScore(character.warmth);
  const humor = clampToneScore(character.humor);
  const flirt = clampToneScore(character.flirt);
  const edge = clampToneScore(character.edge);
  const energy = clampToneScore(character.energy);
  const story = clampToneScore(character.story);
  const profanityAllowed = !!character.profanityAllowed;
  const notes = character.extraDirection.trim();

  const toneParts = [
    warmth >= 65 ? 'warm' : warmth <= 30 ? 'cool' : 'balanced',
    humor >= 65 ? 'funny' : humor <= 30 ? 'dry' : 'lightly funny',
    edge >= 65 ? 'sharp' : edge <= 30 ? 'soft-edged' : 'confident',
    energy >= 65 ? 'high-energy' : energy <= 30 ? 'calm' : 'steady'
  ];

  const behaviorRules = [
    'Answer the latest point directly before adding a joke or aside.',
    humor >= 60 ? 'Use humor often, but anchor it to what was just said.' : 'Keep humor contextual instead of random.',
    story >= 60 ? 'Carry context forward and make callbacks when they fit.' : 'Keep replies concise and on the current point.',
    flirt >= 60 ? 'Allow a more teasing tone when it fits the context.' : 'Do not drift into flirty phrasing unless directly asked.',
    edge >= 60 ? 'Use a rougher, more uncensored tone when safe.' : 'Keep the tone readable and controlled.',
    profanityAllowed ? 'Profanity is allowed when it feels natural, funny, and context-appropriate.' : 'Keep profanity out unless the user explicitly asks for it or quoting it is necessary.'
  ];

  const promptLines = [
    `Voice: ${voiceLabel(voiceName)}.`,
    `Warmth ${warmth}/100. Humor ${humor}/100. Flirt ${flirt}/100. Edge ${edge}/100. Energy ${energy}/100. Story ${story}/100.`,
    profanityAllowed ? 'Profanity is allowed when it improves the line naturally.' : 'Avoid profanity in normal replies.',
    'Stay conversational and respond to what was actually said.',
    'Do not narrate actions in asterisks or brackets.',
    'Avoid repetitive wording and avoid resetting the subject.',
    humor >= 60 ? 'Be funny often, but keep the joke tied to the active topic.' : 'Prefer direct conversational responses over constant jokes.'
  ];
  if (notes) promptLines.push(`Extra direction: ${notes}`);

  return {
    name: 'Direct Control',
    voice: voiceLabel(voiceName),
    tone: toneParts.join(', '),
    humor_level: Math.round(humor / 10),
    aggression_level: Math.round(((edge * 0.7) + (energy * 0.3)) / 10),
    friendliness: Math.round(warmth / 10),
    verbosity: Math.round(((story * 0.7) + (energy * 0.3)) / 10),
    streamer_relationship: flirt >= 55 ? 'playful conversational cohost' : 'direct conversational cohost',
    response_style: toneParts.join(', '),
    lore: notes || 'Directly tuned conversational cohost settings.',
    taboo_topics: ['hate speech', 'private personal data', 'self-harm encouragement'],
    catchphrases: [],
    reply_rules: ['Stay on the latest topic', 'Do not repeat stock phrases', 'Keep replies conversational and context-aware'],
    chat_behavior_rules: behaviorRules,
    viewer_interaction_rules: ['Address viewers like real people', 'Use recent context before improvising'],
    master_prompt_override: promptLines.join(' ')
  };
}
