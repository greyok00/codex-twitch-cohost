import type { CharacterStudioSettings, PersonalityProfile } from '../frontend-types';

export type VoicePreset = {
  id: string;
  displayName: string;
  defaultVoice: string;
  voiceSummary: string;
};

export const defaultToneStudioSettings: CharacterStudioSettings = {
  selectedPreset: 'guy',
  warmth: 55,
  humor: 35,
  flirt: 10,
  edge: 15,
  energy: 60,
  story: 40,
  extraDirection: ''
};

export const voicePresets: VoicePreset[] = [
  { id: 'jenny', displayName: 'Jenny', defaultVoice: 'en-US-JennyNeural', voiceSummary: 'Warm female voice with calm, clear pacing.' },
  { id: 'ana', displayName: 'Ana', defaultVoice: 'en-US-AnaNeural', voiceSummary: 'Smooth female voice with softer, lower delivery.' },
  { id: 'emma', displayName: 'Emma', defaultVoice: 'en-US-EmmaNeural', voiceSummary: 'Balanced female voice with a clean lower-mid tone.' },
  { id: 'guy', displayName: 'Guy', defaultVoice: 'en-US-GuyNeural', voiceSummary: 'Relaxed male voice with warm conversational pacing.' },
  { id: 'christopher', displayName: 'Christopher', defaultVoice: 'en-US-ChristopherNeural', voiceSummary: 'Polished male voice with stronger host energy.' },
  { id: 'roger', displayName: 'Roger', defaultVoice: 'en-US-RogerNeural', voiceSummary: 'Deeper male voice with heavier darker weight.' }
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

export function composeDirectProfile(character: CharacterStudioSettings, voiceName: string): PersonalityProfile {
  const warmth = clampToneScore(character.warmth);
  const humor = clampToneScore(character.humor);
  const flirt = clampToneScore(character.flirt);
  const edge = clampToneScore(character.edge);
  const energy = clampToneScore(character.energy);
  const story = clampToneScore(character.story);
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
    edge >= 60 ? 'Use a rougher, more uncensored tone when safe.' : 'Keep the tone readable and controlled.'
  ];

  const promptLines = [
    `Voice: ${voiceLabel(voiceName)}.`,
    `Warmth ${warmth}/100. Humor ${humor}/100. Flirt ${flirt}/100. Edge ${edge}/100. Energy ${energy}/100. Story ${story}/100.`,
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
