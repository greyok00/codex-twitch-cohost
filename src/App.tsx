import '@fontsource/manrope/index.css';
import './app.css';

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  IconBrandTwitch,
  IconCpu,
  IconMicrophone,
  IconPlayerPlay,
  IconPlayerStop,
  IconSparkles,
  IconUserCircle,
  IconVolume,
  IconWand,
  IconWorld
} from '@tabler/icons-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { BrowserSpeechEngine } from './lib/voice-session/engines/browserSpeech';
import { WorkerBackedTranscriptService } from './lib/voice-session/WorkerBackedTranscriptService';
import { buildVoiceInputFrame } from './lib/voice-session/VoiceFrameBuilder';
import { AvatarRuntime, type AvatarNaturalSize } from './avatar-runtime';
import { GlassTextarea } from './components/glass-textarea';
import {
  GlassSelect,
  GlassSelectContent,
  GlassSelectGroup,
  GlassSelectItem,
  GlassSelectTrigger,
  GlassSelectValue
} from './components/glass-select';
import { GlassScrollArea } from './components/glass-scroll-area';
import { GlassBadge } from './components/ui/glass-badge';
import { GlassButton } from './components/ui/glass-button';
import { GlassCard } from './components/ui/glass-card';
import { GlassInput } from './components/ui/glass-input';
import { GlassSlider } from './components/ui/glass-slider';
import { GlassSwitch } from './components/ui/glass-switch';
import { GlassTabs, GlassTabsList, GlassTabsTrigger } from './components/ui/glass-tabs';
import {
  clearAuthSessions,
  clearBotSession,
  clearStreamerSession,
  configureCloudOnlyMode,
  connectTwitchChat,
  disconnectTwitchChat,
  getAuthSessions,
  getAvatarRigSettings,
  getBehaviorSettings,
  getCharacterStudioSettings,
  getProviderApiKey,
  getProviderModels,
  getSavedAvatarImage,
  getStatus,
  getTtsVoice,
  getTwitchOauthSettings,
  onBotResponse,
  onChatMessage,
  onErrorBanner,
  onStatusUpdated,
  onTimelineEvent,
  openExternal,
  saveAvatarImage,
  savePersonality,
  sendChatMessage,
  setAvatarRigSettings,
  setBehaviorSettings,
  setCharacterStudioSettings,
  setProviderApiKey,
  setTtsVoice,
  setTtsVolume,
  setTwitchOauthSettings,
  setVoiceEnabled,
  startTwitchOauth,
  submitVoiceSessionFrame,
  submitVoiceSessionPrompt,
  synthesizeTtsCloud,
  verifyVoiceRuntime
} from './frontend-api';
import type {
  AppStatus,
  AuthSessions,
  AvatarImage,
  AvatarRigSettings,
  BehaviorSettings,
  CharacterStudioSettings,
  ChatMessage,
  EventMessage,
  PersonalityProfile,
  TwitchOauthSettings,
  TtsVoiceSettings,
  VoiceSessionState,
  VoiceRuntimeReport
} from './frontend-types';

type StudioTuning = Pick<CharacterStudioSettings, 'warmth' | 'humor' | 'flirt' | 'edge' | 'energy' | 'story'>;

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

type ModelMeta = {
  id: string;
  label: string;
  style: string;
  context: string;
  uncensored?: boolean;
  available?: boolean;
};

type FeedItem = {
  key: string;
  user: string;
  content: string;
  timestamp: string;
  tone: 'chat' | 'event';
};

const defaultStatus: AppStatus = {
  channel: '',
  model: 'unknown',
  voiceEnabled: true,
  lurkMode: false,
  twitchState: 'disconnected'
};

const defaultAuth: AuthSessions = {
  botUsername: '',
  botTokenPresent: false,
  channel: '',
  broadcasterLogin: null,
  streamerTokenPresent: false
};

const defaultOauthSettings: TwitchOauthSettings = {
  clientId: '',
  botUsername: '',
  channel: '',
  broadcasterLogin: '',
  redirectUrl: 'http://127.0.0.1:37219/callback'
};

const defaultBehavior: BehaviorSettings = {
  cohostMode: false,
  scheduledMessagesMinutes: null,
  minimumReplyIntervalMs: 9000,
  postBotMessagesToTwitch: false,
  topicContinuationMode: true
};

const defaultCharacter: CharacterStudioSettings = {
  selectedPreset: 'basic-assistant',
  warmth: 55,
  humor: 35,
  flirt: 10,
  edge: 15,
  energy: 60,
  story: 40,
  extraDirection: ''
};

const defaultAvatarRig: AvatarRigSettings = {
  mouthX: 0,
  mouthY: 20,
  mouthWidth: 32,
  mouthOpen: 22,
  mouthSoftness: 70,
  mouthSmile: 8,
  mouthTilt: 0,
  mouthColor: '#7c2d12',
  browX: 0,
  browY: -22,
  browSpacing: 36,
  browArch: 14,
  browTilt: 0,
  browThickness: 9,
  browColor: '#2b211f',
  eyeOpen: 62,
  eyeSquint: 16,
  headTilt: 0,
  headScale: 100,
  glow: 28,
  popupWidth: 320,
  popupHeight: 420
};

const defaultVoiceSession = (): VoiceSessionState => ({
  sessionId: `vs_${Math.random().toString(36).slice(2, 10)}`,
  engine: 'none',
  status: 'idle',
  interimText: '',
  lastFinalText: '',
  firstInterimLatencyMs: null,
  finalLatencyMs: null,
  aiLatencyMs: null,
  ttsLatencyMs: null,
  restartCount: 0,
  droppedCount: 0,
  lastError: null,
  speakingBlocked: false,
  micEnabled: false
});

const voiceOptions = [
  { value: 'auto', label: 'Auto voice' },
  { value: 'en-US-JennyNeural', label: 'Jenny Neural' },
  { value: 'en-US-GuyNeural', label: 'Guy Neural' },
  { value: 'en-US-EricNeural', label: 'Eric Neural' },
  { value: 'en-GB-RyanNeural', label: 'Ryan Neural' },
  { value: 'en-AU-WilliamNeural', label: 'William Neural' },
  { value: 'en-US-AnaNeural', label: 'Ana Neural' },
  { value: 'en-US-AriaNeural', label: 'Aria Neural' },
  { value: 'en-GB-SoniaNeural', label: 'Sonia Neural' },
  { value: 'en-US-ChristopherNeural', label: 'Christopher Neural' },
  { value: 'en-US-RogerNeural', label: 'Roger Neural' },
  { value: 'en-US-SteffanNeural', label: 'Steffan Neural' }
];

const recommendedModels: ModelMeta[] = [
  { id: 'qwen3:8b', label: 'Qwen 8B', style: 'Fast everyday conversation', context: 'Conversational · 8B' },
  { id: 'qwen3:14b', label: 'Qwen 14B', style: 'Stronger follow-through', context: 'Conversational · 14B' },
  { id: 'gemma3:12b', label: 'Gemma 12B', style: 'Cleaner longer replies', context: 'Conversational · 12B' },
  { id: 'gemma3:27b', label: 'Gemma 27B', style: 'Best depth of the normal set', context: 'Conversational · 27B' },
  { id: 'wizard-vicuna-uncensored', label: 'Wizard Vicuna 7B', style: 'Loose general chat', context: 'Uncensored · 7B', uncensored: true },
  { id: 'dolphin-mistral', label: 'Dolphin Mistral 7B', style: 'Edgier conversation', context: 'Uncensored · 7B', uncensored: true },
  { id: 'dolphin-mixtral', label: 'Dolphin Mixtral 8x7B', style: 'Heavier uncensored option', context: 'Uncensored · 8x7B', uncensored: true },
  { id: 'dolphin-phi', label: 'Dolphin Phi 3B', style: 'Small uncensored option', context: 'Uncensored · 3B', uncensored: true }
];

const characterPresets: CharacterPreset[] = [
  { id: 'basic-assistant', category: 'Normal', displayName: 'Basic Assistant', defaultVoice: 'en-US-JennyNeural', voiceSummary: 'Warm female voice with neutral pacing and clean diction.', description: 'A stable everyday cohost that answers clearly, remembers important details, and stays grounded in what was just said.', toneSeed: ['clear', 'steady', 'helpful', 'grounded'], styleSeed: ['plainspoken', 'direct', 'context-first'], relationshipSeed: 'steady cohost who keeps the conversation useful and easy to follow', loreSeed: 'Built to feel reliable, normal, and consistent in long conversations.', tuning: { warmth: 70, humor: 35, flirt: 0, edge: 5, energy: 40, story: 35 } },
  { id: 'midnight-host', category: 'Normal', displayName: 'Midnight Host', defaultVoice: 'en-US-ChristopherNeural', voiceSummary: 'Smooth male voice with polished late-night host energy.', description: 'Smart, amused, and charismatic. Good at making even simple talk feel stylish without turning it into nonsense.', toneSeed: ['cool', 'charming', 'observant', 'witty'], styleSeed: ['slick', 'polished', 'radio-smooth'], relationshipSeed: 'charismatic cohost who keeps things moving with style', loreSeed: 'Feels like a late-night host who can make technical trouble sound funny.', tuning: { warmth: 62, humor: 68, flirt: 24, edge: 18, energy: 58, story: 52 } },
  { id: 'sharp-bestie', category: 'Normal', displayName: 'Sharp Bestie', defaultVoice: 'en-US-AriaNeural', voiceSummary: 'Bright female voice with quick, reactive lift.', description: 'Fast, funny friend energy. Notices everything instantly and reacts like someone actually in the room with you.', toneSeed: ['quick', 'funny', 'warm', 'playful'], styleSeed: ['snappy', 'chatty', 'reactive'], relationshipSeed: 'fast-talking best friend who keeps the room lively', loreSeed: 'Built for sharp banter and very human-feeling reactions.', tuning: { warmth: 82, humor: 78, flirt: 22, edge: 20, energy: 74, story: 42 } },
  { id: 'studio-analyst', category: 'Normal', displayName: 'Studio Analyst', defaultVoice: 'en-GB-RyanNeural', voiceSummary: 'Measured British male voice with calm analytical weight.', description: 'A cleaner, more insightful personality for thoughtful reactions, clearer summaries, and stronger context tracking.', toneSeed: ['analytical', 'dry', 'composed', 'observant'], styleSeed: ['measured', 'concise', 'smart'], relationshipSeed: 'measured cohost who reads the room and explains the moment cleanly', loreSeed: 'Designed for thoughtful commentary rather than noise.', tuning: { warmth: 48, humor: 46, flirt: 6, edge: 16, energy: 34, story: 72 } },
  { id: 'story-weaver', category: 'Story', displayName: 'Story Weaver', defaultVoice: 'en-GB-SoniaNeural', voiceSummary: 'Smooth British female voice with clear storytelling cadence.', description: 'The preset for continuity, scene building, and long-form romantic or dramatic conversation that actually stays on subject.', toneSeed: ['immersive', 'expressive', 'attentive', 'dramatic'], styleSeed: ['cinematic', 'descriptive', 'scene-aware'], relationshipSeed: 'story-driven partner who keeps scenes coherent and emotionally connected', loreSeed: 'Built to carry scenes, callbacks, and emotional continuity without losing the thread.', tuning: { warmth: 68, humor: 30, flirt: 38, edge: 18, energy: 40, story: 92 } },
  { id: 'velvet-flirt', category: 'Seductive', displayName: 'Velvet Flirt', defaultVoice: 'en-US-AnaNeural', voiceSummary: 'Soft female voice with intimate smoothness and teasing control.', description: 'Poised flirt energy. It keeps the chemistry alive without sounding vague, random, or detached from the conversation.', toneSeed: ['smooth', 'teasing', 'intimate', 'confident'], styleSeed: ['silky', 'close', 'suggestive'], relationshipSeed: 'playful flirt who keeps eye contact with the subject instead of drifting', loreSeed: 'Designed for tension, chemistry, and attentive conversation.', tuning: { warmth: 74, humor: 48, flirt: 82, edge: 14, energy: 56, story: 52 } },
  { id: 'dangerous-charmer', category: 'Seductive', displayName: 'Dangerous Charmer', defaultVoice: 'en-AU-WilliamNeural', voiceSummary: 'Dark Australian male voice with sleek confidence and bite.', description: 'Controlled, magnetic, and dangerous without losing coherence. Better for seductive pressure and colder flirt energy.', toneSeed: ['magnetic', 'dangerous', 'cool', 'precise'], styleSeed: ['controlled', 'sleek', 'provocative'], relationshipSeed: 'dangerously charming cohost with deliberate tension and precise timing', loreSeed: 'Feels like someone who can take over the room with one line.', tuning: { warmth: 42, humor: 40, flirt: 76, edge: 42, energy: 52, story: 58 } },
  { id: 'sweet-trouble', category: 'Seductive', displayName: 'Sweet Trouble', defaultVoice: 'en-US-GuyNeural', voiceSummary: 'Relaxed male voice with easy warmth and playful confidence.', description: 'A softer flirt preset that feels affectionate, touchy, and fun rather than severe or theatrical.', toneSeed: ['sweet', 'playful', 'tempting', 'easygoing'], styleSeed: ['light', 'warm', 'touchy'], relationshipSeed: 'flirty cohost who keeps things soft, close, and playful', loreSeed: 'Built for affectionate chemistry and easy momentum.', tuning: { warmth: 80, humor: 58, flirt: 68, edge: 10, energy: 50, story: 44 } },
  { id: 'after-dark', category: 'Adult', displayName: 'After Dark', defaultVoice: 'en-US-AnaNeural', voiceSummary: 'Low intimate female voice with direct, heavy late-night energy.', description: 'A hotter, more forward preset meant for grown-up chemistry and intentional tension while still staying readable and contextual.', toneSeed: ['heated', 'direct', 'confident', 'close'], styleSeed: ['late-night', 'heavy', 'intimate'], relationshipSeed: 'adult-only cohost built for direct chemistry and clear scene continuity', loreSeed: 'Designed for mature conversations that stay on topic instead of looping.', tuning: { warmth: 58, humor: 32, flirt: 90, edge: 26, energy: 60, story: 70 } },
  { id: 'heat-check', category: 'Adult', displayName: 'Heat Check', defaultVoice: 'en-GB-SoniaNeural', voiceSummary: 'Bold female voice with sharper pressure and stronger pacing.', description: 'Shameless, bold, and high-energy. Meant for hotter banter, stronger reactions, and more dominant pacing.', toneSeed: ['bold', 'cocky', 'heated', 'fast'], styleSeed: ['pushy', 'dirty-minded', 'high-energy'], relationshipSeed: 'aggressive flirt who keeps the pressure on without becoming incoherent', loreSeed: 'Turns tension up fast but should still stay locked onto the subject.', tuning: { warmth: 38, humor: 52, flirt: 84, edge: 58, energy: 82, story: 46 } },
  { id: 'black-velvet-villain', category: 'Edgy', displayName: 'Black Velvet Villain', defaultVoice: 'en-US-RogerNeural', voiceSummary: 'Hard male voice with cold theatrical menace and clean control.', description: 'Elegant edge. A villain-style personality built for sharp statements, stylish cruelty, and cleaner long-form tension.', toneSeed: ['cold', 'stylish', 'intense', 'seductive'], styleSeed: ['velvet-steel', 'controlled', 'cutting'], relationshipSeed: 'elegant menace who treats every sentence like it should land hard', loreSeed: 'Made for dark charisma, not random chaos.', tuning: { warmth: 22, humor: 48, flirt: 50, edge: 78, energy: 48, story: 76 } },
  { id: 'no-filter-menace', category: 'Edgy', displayName: 'No Filter Menace', defaultVoice: 'en-US-SteffanNeural', voiceSummary: 'Hard male voice with clipped pace and darker internet energy.', description: 'The most aggressive preset in the set. Ruthless, online, sharp, and funny when you want edge without the personality dissolving into gibberish.', toneSeed: ['ruthless', 'darkly funny', 'reckless', 'direct'], styleSeed: ['blunt', 'savage', 'pressure-heavy'], relationshipSeed: 'wild cohost who pushes right up to the line and stays there', loreSeed: 'Built for dangerous energy that still follows the moment.', tuning: { warmth: 16, humor: 74, flirt: 18, edge: 92, energy: 78, story: 38 } }
];

function colorForUser(user: string) {
  const palette = ['#60a5fa', '#f59e0b', '#34d399', '#f472b6', '#a78bfa', '#f87171', '#22d3ee', '#facc15'];
  const source = (user || 'unknown').toLowerCase();
  let hash = 0;
  for (let i = 0; i < source.length; i += 1) hash = (hash * 31 + source.charCodeAt(i)) >>> 0;
  return palette[hash % palette.length];
}

function lightVariant(light: string) {
  if (light === 'green') return 'success';
  if (light === 'yellow') return 'warning';
  if (light === 'red') return 'destructive';
  return 'outline';
}

function formatAgo(value?: string | null) {
  if (!value) return 'never';
  const ms = Date.now() - new Date(value).getTime();
  if (Number.isNaN(ms)) return 'unknown';
  if (ms < 1000) return 'just now';
  if (ms < 60_000) return `${Math.round(ms / 1000)}s ago`;
  if (ms < 3_600_000) return `${Math.round(ms / 60_000)}m ago`;
  return `${Math.round(ms / 3_600_000)}h ago`;
}

function emitAvatarEvent(type: string, payload: Record<string, unknown> = {}) {
  const channel = typeof BroadcastChannel !== 'undefined' ? new BroadcastChannel('cohost-avatar-events') : null;
  channel?.postMessage({ type, ts: Date.now(), ...payload });
  channel?.close();
}

function normalizeSpeech(text: string) {
  return text
    .replace(/\bgreyok\b/gi, 'Grey Okay')
    .replace(/\bgreyok__\b/gi, 'Grey Okay')
    .replace(/\bgrey ok\b/gi, 'Grey Okay')
    .replace(/\b([A-Za-z]+)\s+(s|re|ve|ll|d|m)\b(?=(?:\s+[A-Za-z])|[.,!?]|$)/gi, "$1'$2")
    .replace(/\s+/g, ' ')
    .trim();
}

function clamp(value: number) {
  return Math.max(0, Math.min(100, Math.round(value)));
}

function voiceLabel(voiceId: string) {
  return voiceId.replace(/^en-[A-Z]{2}-/, '').replace('Neural', '');
}

function buildTraitSummary(tuning: StudioTuning) {
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

function buildRelationship(preset: CharacterPreset, tuning: StudioTuning) {
  if (tuning.flirt >= 70) return `${preset.relationshipSeed}; lean into chemistry and direct statements more than questions`;
  if (tuning.edge >= 75) return `${preset.relationshipSeed}; push harder, stay dominant, and avoid timid phrasing`;
  if (tuning.story >= 70) return `${preset.relationshipSeed}; keep scenes continuous and remember relationship details across replies`;
  return preset.relationshipSeed;
}

function buildStyle(preset: CharacterPreset, tuning: StudioTuning) {
  const parts = [...preset.styleSeed];
  if (tuning.energy >= 70) parts.push('fast-reacting');
  if (tuning.energy <= 25) parts.push('patient');
  if (tuning.story >= 70) parts.push('continuity-aware');
  if (tuning.humor >= 70) parts.push('joke-ready');
  if (tuning.flirt >= 70) parts.push('chemistry-forward');
  if (tuning.edge >= 70) parts.push('dominant');
  return Array.from(new Set(parts)).join(', ');
}

function buildTone(preset: CharacterPreset, tuning: StudioTuning) {
  const parts = [...preset.toneSeed];
  if (tuning.warmth >= 70) parts.push('attentive');
  if (tuning.warmth <= 25) parts.push('aloof');
  if (tuning.story >= 70) parts.push('immersive');
  if (tuning.flirt >= 70) parts.push('charged');
  return Array.from(new Set(parts)).join(', ');
}

function composeProfile(preset: CharacterPreset, character: CharacterStudioSettings): PersonalityProfile {
  const tuning: StudioTuning = {
    warmth: clamp(character.warmth),
    humor: clamp(character.humor),
    flirt: clamp(character.flirt),
    edge: clamp(character.edge),
    energy: clamp(character.energy),
    story: clamp(character.story)
  };
  const notes = character.extraDirection.trim();
  const humor = Math.round(tuning.humor / 10);
  const friendliness = Math.round(tuning.warmth / 10);
  const aggression = Math.round(clamp((tuning.edge * 0.7) + (tuning.energy * 0.3)) / 10);
  const verbosity = Math.round(clamp((tuning.story * 0.65) + (tuning.energy * 0.35)) / 10);
  const overrideLines = [
    `Character: ${preset.displayName}.`,
    `Voice pairing: ${preset.voiceSummary}`,
    `Style summary: ${buildTraitSummary(tuning)}.`,
    'Use statements more than questions unless a direct clarification is necessary.',
    'Stay on the active topic and continue scenes instead of resetting them.',
    'Remember names, titles, pet names, and explicit relationship details when the user tells you to.',
    'Do not narrate actions in asterisks or roleplay brackets. Speak naturally.',
    'If the user gives an instruction about how to address them, treat that as durable memory.'
  ];
  if (notes) overrideLines.push(`Extra direction: ${notes}`);
  return {
    name: preset.displayName,
    voice: voiceLabel(preset.defaultVoice),
    tone: buildTone(preset, tuning),
    humor_level: humor,
    aggression_level: aggression,
    friendliness,
    verbosity,
    streamer_relationship: buildRelationship(preset, tuning),
    response_style: buildStyle(preset, tuning),
    lore: `${preset.description} ${preset.loreSeed}${notes ? ` Extra direction: ${notes}` : ''}`.trim(),
    taboo_topics: ['hate speech', 'private personal data', 'self-harm encouragement'],
    catchphrases: [],
    reply_rules: ['Answer the latest point first', 'Use statements more than repeated questions', 'Stay grounded in the current context', 'Do not repeat phrasing or reset the subject'],
    chat_behavior_rules: ['Use recent context and memory before improvising', tuning.story >= 70 ? 'Continue scenes and callbacks across replies' : 'Keep the conversation moving without drifting', tuning.flirt >= 60 ? 'Keep chemistry intentional and tied to the current topic' : 'Keep the tone readable and coherent'],
    viewer_interaction_rules: ['Address viewers like real people', 'If the user gives a name or title preference, remember it and use it consistently'],
    master_prompt_override: overrideLines.join(' ')
  };
}

function normalizeFamily(model: string) {
  return model.toLowerCase().replace(/:(latest|[\w.\-]+)$/i, '');
}

function enrichModel(id: string): ModelMeta {
  const lower = id.toLowerCase();
  const family = normalizeFamily(lower);
  const direct = recommendedModels.find((entry) => lower === entry.id.toLowerCase());
  if (direct) return { ...direct, id };
  const familyMatch = recommendedModels.find((entry) => family.startsWith(normalizeFamily(entry.id)));
  if (familyMatch) return { ...familyMatch, id };
  const uncensored = lower.includes('uncensored') || lower.startsWith('dolphin-');
  return {
    id,
    label: uncensored ? 'Uncensored discovered model' : 'Discovered cloud model',
    style: uncensored ? 'Looser-aligned output' : 'Live account model',
    context: '-',
    uncensored,
    available: true
  };
}

function buildCatalog(models: string[]) {
  const availableFamilies = new Set(models.map((model) => normalizeFamily(model)));
  return recommendedModels.map((entry) => {
    const matched = models.find((model) => normalizeFamily(model).startsWith(normalizeFamily(entry.id)));
    const resolved = matched ? enrichModel(matched) : { ...entry };
    return {
      ...resolved,
      available: availableFamilies.has(normalizeFamily(entry.id)) || !!matched
    };
  });
}

function LabeledField({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <label className="glass-field">
      <span className="glass-field-label">{label}</span>
      {children}
      {hint ? <span className="glass-field-hint">{hint}</span> : null}
    </label>
  );
}

function SliderField({ label, value, min, max, step = 1, onChange }: { label: string; value: number; min: number; max: number; step?: number; onChange: (value: number) => void }) {
  return (
    <div className="slider-field">
      <div className="slider-field-head">
        <span className="glass-field-label">{label}</span>
        <span className="slider-field-value">{Math.round(value)}</span>
      </div>
      <GlassSlider min={min} max={max} step={step} value={[value]} onValueChange={(values) => onChange(values[0] ?? value)} />
    </div>
  );
}

function RuntimeToggle({ label, description, checked, onChange }: { label: string; description: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <div className="runtime-toggle">
      <div className="runtime-toggle-copy">
        <div className="runtime-toggle-title">{label}</div>
        <div className="runtime-toggle-description">{description}</div>
      </div>
      <GlassSwitch checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

function FeedMessage({ item }: { item: FeedItem }) {
  return (
    <div className={`feed-item ${item.tone}`} style={{ ['--user-accent' as string]: colorForUser(item.user) }}>
      <div className="feed-user">{item.user} · {new Date(item.timestamp).toLocaleTimeString()}</div>
      <div className="feed-content">{item.content}</div>
    </div>
  );
}

export default function App() {
  const [status, setStatus] = useState<AppStatus>(defaultStatus);
  const [auth, setAuth] = useState<AuthSessions>(defaultAuth);
  const [behavior, setBehavior] = useState<BehaviorSettings>(defaultBehavior);
  const [character, setCharacter] = useState<CharacterStudioSettings>(defaultCharacter);
  const [voiceConfig, setVoiceConfig] = useState<TtsVoiceSettings>({ enabled: true, voiceName: 'auto', volumePercent: 100 });
  const [voiceRuntime, setVoiceRuntime] = useState<VoiceRuntimeReport | null>(null);
  const [oauthSettings, setOauthSettings] = useState<TwitchOauthSettings>(defaultOauthSettings);
  const [cloudApiKey, setCloudApiKey] = useState('');
  const [cloudModels, setCloudModels] = useState<ModelMeta[]>(buildCatalog([]));
  const [selectedModel, setSelectedModel] = useState('qwen3:8b');
  const [cloudStatus, setCloudStatus] = useState('');
  const [mainTab, setMainTab] = useState<'chat' | 'twitch' | 'cloud' | 'character' | 'settings'>('chat');
  const [characterTab, setCharacterTab] = useState<'persona' | 'stage'>('persona');
  const [avatarImage, setAvatarImage] = useState<AvatarImage | null>(null);
  const [avatarRig, setAvatarRig] = useState<AvatarRigSettings>(defaultAvatarRig);
  const [chat, setChat] = useState<ChatMessage[]>([]);
  const [timeline, setTimeline] = useState<EventMessage[]>([]);
  const [composer, setComposer] = useState('');
  const [activeFeed, setActiveFeed] = useState<'combined' | 'chat' | 'timeline'>('combined');
  const [voiceSession, setVoiceSession] = useState<VoiceSessionState>(defaultVoiceSession);
  const [banner, setBanner] = useState<string | null>(null);

  const activePreset = useMemo(
    () => characterPresets.find((preset) => preset.id === character.selectedPreset) ?? characterPresets[0],
    [character.selectedPreset]
  );
  const transcriptServiceRef = useRef<WorkerBackedTranscriptService | null>(null);
  const browserSpeechRef = useRef<BrowserSpeechEngine | null>(null);
  const ttsAudioRef = useRef<HTMLAudioElement | null>(null);
  const bannerTimeoutRef = useRef<number | null>(null);
  const aiStartRef = useRef<number>(0);

  const flashBanner = (message: string, timeoutMs = 5000) => {
    setBanner(message);
    if (bannerTimeoutRef.current) window.clearTimeout(bannerTimeoutRef.current);
    bannerTimeoutRef.current = window.setTimeout(() => setBanner(null), timeoutMs);
  };

  useEffect(() => {
    transcriptServiceRef.current = new WorkerBackedTranscriptService();
    return () => transcriptServiceRef.current?.dispose();
  }, []);

  const loadAll = async () => {
    const [nextStatus, nextAuth, nextBehavior, nextCharacter, nextVoice, nextRuntime, nextOauth, savedCloudKey, savedAvatarImage, nextAvatarRig] = await Promise.all([
      getStatus(),
      getAuthSessions(),
      getBehaviorSettings(),
      getCharacterStudioSettings(),
      getTtsVoice(),
      verifyVoiceRuntime().catch(() => null),
      getTwitchOauthSettings().catch(() => defaultOauthSettings),
      getProviderApiKey('ollama-cloud').catch(() => null),
      getSavedAvatarImage().catch(() => null),
      getAvatarRigSettings().catch(() => defaultAvatarRig)
    ]);

    setStatus(nextStatus);
    setAuth(nextAuth);
    setBehavior(nextBehavior);
    setCharacter(nextCharacter);
    setVoiceConfig(nextVoice);
    setVoiceRuntime(nextRuntime);
    setOauthSettings({
      clientId: nextOauth.clientId || '',
      botUsername: nextOauth.botUsername || '',
      channel: nextOauth.channel || '',
      broadcasterLogin: nextOauth.broadcasterLogin || '',
      redirectUrl: nextOauth.redirectUrl || defaultOauthSettings.redirectUrl
    });
    setAvatarImage(savedAvatarImage);
    setAvatarRig(nextAvatarRig);
    setCloudApiKey(savedCloudKey?.trim() || '');

    if (savedCloudKey?.trim()) {
      try {
        const models = await getProviderModels('ollama-cloud');
        const catalog = buildCatalog(models);
        setCloudModels(catalog);
        setSelectedModel((current) => current || catalog[0]?.id || 'qwen3:8b');
        setCloudStatus(models.length > 0 ? `Detected ${models.length} cloud model(s) on this account.` : 'No cloud models detected on this account yet.');
      } catch {
        setCloudModels(buildCatalog([]));
        setCloudStatus('Cloud model discovery failed.');
      }
    } else {
      setCloudModels(buildCatalog([]));
      setCloudStatus('Paste an Ollama API key to check cloud models.');
    }
  };

  useEffect(() => {
    void loadAll().catch((error) => flashBanner(String(error)));

    const every5 = window.setInterval(() => {
      void getStatus().then(setStatus).catch(() => undefined);
      void getAuthSessions().then(setAuth).catch(() => undefined);
    }, 5000);

    const unsubs: Promise<(() => void)[]> = Promise.all([
      onChatMessage((payload) => setChat((items) => [payload, ...items].slice(0, 300))),
      onBotResponse((payload) => {
        setChat((items) => [payload, ...items].slice(0, 300));
        setVoiceSession((state) => ({
          ...state,
          status: state.micEnabled ? state.status : 'replying',
          aiLatencyMs: aiStartRef.current ? Date.now() - aiStartRef.current : state.aiLatencyMs
        }));
        if (!voiceConfig.enabled || voiceSession.micEnabled) return;
        const clean = normalizeSpeech(payload.content);
        if (!clean) return;
        void speakBotText(clean);
      }),
      onTimelineEvent((payload) => setTimeline((items) => [payload, ...items].slice(0, 300))),
      onStatusUpdated((payload) => setStatus(payload)),
      onErrorBanner((payload) => {
        if (!payload.trim()) return;
        flashBanner(payload);
      })
    ]);

    return () => {
      window.clearInterval(every5);
      if (bannerTimeoutRef.current) window.clearTimeout(bannerTimeoutRef.current);
      void unsubs.then((list) => list.forEach((unsub) => unsub()));
      void stopMic();
      stopSpeechPlayback();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const stopSpeechPlayback = () => {
    emitAvatarEvent('speak_stop');
    (window as Window & { __cohost_tts_speaking?: boolean; __cohost_tts_suppressed_until?: number; __cohost_recording_active?: boolean }).__cohost_tts_speaking = false;
    (window as Window & { __cohost_tts_suppressed_until?: number }).__cohost_tts_suppressed_until = Date.now() + 1500;
    try {
      window.speechSynthesis.cancel();
    } catch {
      // no-op
    }
    if (ttsAudioRef.current) {
      try {
        ttsAudioRef.current.pause();
        ttsAudioRef.current.currentTime = 0;
      } catch {
        // no-op
      }
      ttsAudioRef.current = null;
    }
  };

  const speakBotText = async (text: string) => {
    stopSpeechPlayback();
    emitAvatarEvent('speak_start', { text });
    const runtime = window as Window & { __cohost_tts_speaking?: boolean; __cohost_tts_suppressed_until?: number };
    runtime.__cohost_tts_speaking = true;
    runtime.__cohost_tts_suppressed_until = Date.now() + 30_000;
    try {
      const dataUrl = await synthesizeTtsCloud(text, voiceConfig.voiceName && voiceConfig.voiceName !== 'auto' ? voiceConfig.voiceName : null);
      await new Promise<void>((resolve) => {
        const audio = new Audio(dataUrl);
        ttsAudioRef.current = audio;
        audio.volume = Math.max(0, Math.min(1, (voiceConfig.volumePercent ?? 100) / 100));
        audio.onended = () => resolve();
        audio.onerror = () => resolve();
        void audio.play().catch(() => resolve());
      });
    } catch {
      // no-op
    } finally {
      emitAvatarEvent('speak_stop');
      runtime.__cohost_tts_speaking = false;
      runtime.__cohost_tts_suppressed_until = Date.now() + 1500;
      ttsAudioRef.current = null;
    }
  };

  const stopMic = async () => {
    (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = false;
    if (browserSpeechRef.current) {
      await browserSpeechRef.current.stop().catch(() => undefined);
      await browserSpeechRef.current.dispose().catch(() => undefined);
      browserSpeechRef.current = null;
    }
    setVoiceSession((state) => ({ ...state, status: 'idle', micEnabled: false, interimText: '' }));
  };

  const startMic = async () => {
    stopSpeechPlayback();
    await stopMic();
    const transcriptService = transcriptServiceRef.current;
    if (!transcriptService) return;
    const sessionId = `vs_${Math.random().toString(36).slice(2, 10)}`;
    transcriptService.setStartedAt(Date.now());
    await transcriptService.reset();
    setVoiceSession({ ...defaultVoiceSession(), sessionId, micEnabled: true, status: 'starting', engine: 'browser-speech' });

    const engine = new BrowserSpeechEngine({
      onInterim: (text) => {
        void transcriptService.pushInterim(text).then(({ interim, firstInterimLatencyMs }) => {
          setVoiceSession((state) => ({
            ...state,
            status: 'listening',
            interimText: interim,
            firstInterimLatencyMs: state.firstInterimLatencyMs ?? firstInterimLatencyMs,
            engine: 'browser-speech'
          }));
        });
      },
      onFinal: async (text) => {
        const normalized = await transcriptService.pushFinal(text);
        if (!normalized.committed) {
          setVoiceSession((state) => ({ ...state, interimText: '', droppedCount: state.droppedCount + 1 }));
          return;
        }
        aiStartRef.current = Date.now();
        setVoiceSession((state) => ({
          ...state,
          status: 'processing',
          interimText: '',
          lastFinalText: normalized.committed ?? '',
          finalLatencyMs: normalized.finalLatencyMs
        }));
        const frame = await buildVoiceInputFrame({
          sessionId,
          mode: 'owner',
          engine: 'browser-speech',
          transcript: normalized.committed,
          finalLatencyMs: normalized.finalLatencyMs
        });
        await submitVoiceSessionFrame(frame, null);
      },
      onStatus: (nextStatus, detail) => {
        setVoiceSession((state) => ({ ...state, status: nextStatus, lastError: nextStatus === 'error' ? detail ?? state.lastError : state.lastError }));
      },
      onError: (message) => {
        setVoiceSession((state) => ({ ...state, status: 'error', lastError: message }));
        flashBanner(`Mic error: ${message}`);
      },
      onSpeechStart: () => {
        (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = true;
        stopSpeechPlayback();
        setVoiceSession((state) => ({ ...state, speakingBlocked: true }));
      },
      onSpeechEnd: () => {
        (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = false;
        setVoiceSession((state) => ({ ...state, speakingBlocked: false }));
      }
    });

    browserSpeechRef.current = engine;
    await engine.start();
  };

  const patchBehavior = async (patch: Partial<BehaviorSettings>) => {
    const next = { ...behavior, ...patch };
    setBehavior(next);
    await setBehaviorSettings(next);
  };

  const patchCharacter = async (patch: Partial<CharacterStudioSettings>) => {
    const next = { ...character, ...patch };
    setCharacter(next);
    await setCharacterStudioSettings(next);
  };

  const saveOauth = async () => {
    await setTwitchOauthSettings({ clientId: oauthSettings.clientId, redirectUrl: oauthSettings.redirectUrl });
    flashBanner('Twitch OAuth settings saved.');
  };

  const refreshCloudModels = async () => {
    if (!cloudApiKey.trim()) {
      setCloudStatus('Paste an Ollama API key first.');
      return;
    }
    await setProviderApiKey('ollama-cloud', cloudApiKey.trim());
    const models = await getProviderModels('ollama-cloud');
    const catalog = buildCatalog(models);
    setCloudModels(catalog);
    if (catalog[0] && !catalog.some((model) => model.id === selectedModel)) setSelectedModel(catalog[0].id);
    setCloudStatus(models.length > 0 ? `Connected to Ollama Cloud. Showing ${catalog.length} curated picks.` : 'Connected, but account discovery returned no models.');
  };

  const enableCloudModel = async () => {
    await configureCloudOnlyMode(selectedModel);
    setStatus((current) => ({ ...current, model: selectedModel }));
    flashBanner(`Cloud-only mode enabled with ${selectedModel}.`);
  };

  const applyCharacterPreset = async () => {
    const profile = composeProfile(activePreset, character);
    await setTtsVoice(activePreset.defaultVoice);
    setVoiceConfig((current) => ({ ...current, voiceName: activePreset.defaultVoice }));
    await savePersonality(profile);
    await setCharacterStudioSettings({ ...character, selectedPreset: activePreset.id });
    flashBanner(`${activePreset.displayName} applied with ${voiceLabel(activePreset.defaultVoice)}.`);
  };

  const saveAvatarRig = async () => {
    await setAvatarRigSettings(avatarRig);
    emitAvatarEvent('rig_update', { rig: avatarRig });
    flashBanner('Avatar rig saved to unified config.');
  };

  const handleAvatarFile = async (file: File | null) => {
    if (!file) return;
    const dataUrl = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result || ''));
      reader.onerror = () => reject(reader.error ?? new Error('Failed reading avatar file'));
      reader.readAsDataURL(file);
    });
    const saved = await saveAvatarImage(dataUrl, file.name);
    setAvatarImage(saved);
    emitAvatarEvent('avatar_update', { src: saved.dataUrl });
    flashBanner(`Avatar saved: ${file.name}`);
  };

  const snapAvatarWindowToImage = async (natural: AvatarNaturalSize) => {
    const width = natural.width || 320;
    const height = natural.height || 420;
    const maxWidth = 420;
    const maxHeight = 560;
    const scale = Math.min(maxWidth / width, maxHeight / height, 1);
    const targetWidth = Math.round(Math.max(180, width * scale + 18));
    const targetHeight = Math.round(Math.max(220, height * scale + 20));
    const next = { ...avatarRig, popupWidth: targetWidth, popupHeight: targetHeight };
    setAvatarRig(next);
    await setAvatarRigSettings(next);
    try {
      const current = getCurrentWindow();
      await current.setSize(new LogicalSize(targetWidth, targetHeight));
    } catch {
      // no-op
    }
    emitAvatarEvent('snap_window', { width: targetWidth, height: targetHeight });
  };

  const openAvatarPopout = async () => {
    const label = 'avatar-stage';
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) {
      await existing.setFocus();
      return;
    }
    const popup = new WebviewWindow(label, {
      title: 'Character Stage',
      url: '/?avatar=1',
      width: Math.max(180, avatarRig.popupWidth),
      height: Math.max(220, avatarRig.popupHeight),
      resizable: true,
      center: true,
      visible: true,
      decorations: true
    });
    await popup.once('tauri://created', () => undefined);
  };

  const submitPrompt = async () => {
    const text = composer.trim();
    if (!text) return;
    setComposer('');
    aiStartRef.current = Date.now();
    await submitVoiceSessionPrompt(text, null);
  };

  const submitTwitch = async () => {
    const text = composer.trim();
    if (!text) return;
    await sendChatMessage(text);
    setComposer('');
  };

  const combinedFeed = useMemo<FeedItem[]>(() => {
    const chatItems = chat.map((item) => ({ key: item.id, tone: 'chat' as const, user: item.user, content: item.content, timestamp: item.timestamp }));
    const eventItems = timeline.map((item) => ({ key: item.id, tone: 'event' as const, user: item.kind, content: item.content, timestamp: item.timestamp }));
    return [...chatItems, ...eventItems].sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
  }, [chat, timeline]);

  const activeItems = activeFeed === 'chat'
    ? chat.map((item) => ({ key: item.id, tone: 'chat' as const, user: item.user, content: item.content, timestamp: item.timestamp }))
    : activeFeed === 'timeline'
      ? timeline.map((item) => ({ key: item.id, tone: 'event' as const, user: item.kind, content: item.content, timestamp: item.timestamp }))
      : combinedFeed;

  const isAvatarWindow = typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('avatar') === '1';

  if (isAvatarWindow) {
    return (
      <div className="avatar-popup-root">
        <AvatarRuntime
          avatarSrc={avatarImage?.dataUrl || '/floating-head.png'}
          rig={avatarRig}
          detached={true}
          onSnap={(size) => void snapAvatarWindowToImage(size)}
        />
      </div>
    );
  }

  return (
    <div className="desktop-root">
      <GlassCard className="utility-strip glass-surface">
        <div className="hero-copy">
          <div className="hero-title">GreyOK Command Center</div>
          <div className="hero-subtitle">Desktop control surface for Twitch, local conversation, character tuning, and runtime monitoring.</div>
        </div>
        <GlassBadge variant={status.twitchState === 'connected' ? 'success' : status.twitchState === 'connecting' ? 'warning' : 'outline'}>
          {status.twitchState === 'connected' ? 'Twitch Connected' : status.twitchState === 'connecting' ? 'Twitch Connecting' : 'Local Mode'}
        </GlassBadge>
        <GlassBadge variant={voiceSession.micEnabled ? 'primary' : 'outline'}>
          Mic {voiceSession.micEnabled ? 'Listening' : 'Idle'}
        </GlassBadge>
        <GlassBadge variant={voiceConfig.enabled ? 'success' : 'outline'}>
          Voice {voiceConfig.enabled ? 'On' : 'Muted'}
        </GlassBadge>
      </GlassCard>

      <div className="workspace-grid">
        <div className="main-shell">
          <GlassCard className="glass-surface status-card">
            <div className="status-row">
              <div className="status-block">
                <div className="status-label"><IconBrandTwitch size={16} /> Twitch</div>
                <div className="status-value">{status.twitchState}{status.channel ? ` · #${status.channel}` : ''}</div>
              </div>
              <div className="status-block">
                <div className="status-label"><IconMicrophone size={16} /> Mic</div>
                <div className="status-value">{voiceSession.status} · {voiceSession.engine}</div>
              </div>
              <div className="status-block">
                <div className="status-label"><IconVolume size={16} /> Voice</div>
                <div className="status-value">{voiceConfig.enabled ? (voiceConfig.voiceName || 'auto') : 'muted'}</div>
              </div>
              <div className="status-block">
                <div className="status-label"><IconCpu size={16} /> Model</div>
                <div className="status-value">{status.model}</div>
              </div>
            </div>
            {banner ? <div className="banner-error">{banner}</div> : null}
          </GlassCard>

          <GlassCard className="glass-surface conversation-card">
            <div className="main-tab-header">
              <GlassTabs value={mainTab} onValueChange={(value) => setMainTab(value as typeof mainTab)}>
                <GlassTabsList className="folder-tabs-list main-folder-tabs">
                  <GlassTabsTrigger className="folder-tab-trigger" value="chat">Chat Folder</GlassTabsTrigger>
                  <GlassTabsTrigger className="folder-tab-trigger" value="twitch">Twitch Folder</GlassTabsTrigger>
                  <GlassTabsTrigger className="folder-tab-trigger" value="cloud">Models Folder</GlassTabsTrigger>
                  <GlassTabsTrigger className="folder-tab-trigger" value="character">Character Folder</GlassTabsTrigger>
                  <GlassTabsTrigger className="folder-tab-trigger" value="settings">Settings Folder</GlassTabsTrigger>
                </GlassTabsList>
              </GlassTabs>
              <div className="tab-caption">
                {mainTab === 'chat' && 'Main conversation window with local chat, Twitch chat send, mic, and live feed.'}
                {mainTab === 'twitch' && 'OAuth, bot account, streamer account, and Twitch chat connection.'}
                {mainTab === 'cloud' && 'Curated conversational and uncensored Ollama cloud picks only.'}
                {mainTab === 'character' && 'Personality package, tuning, avatar image, and embedded character stage.'}
                {mainTab === 'settings' && 'Voice, pacing, and runtime diagnostics. No duplicate controls elsewhere.'}
              </div>
            </div>

            {mainTab === 'chat' ? (
              <div className="chat-pane">
                <div className="subtab-row">
                  <GlassTabs value={activeFeed} onValueChange={(value) => setActiveFeed(value as typeof activeFeed)}>
                    <GlassTabsList className="subtabs-list">
                      <GlassTabsTrigger className="subtab-trigger" value="combined">Combined Feed</GlassTabsTrigger>
                      <GlassTabsTrigger className="subtab-trigger" value="chat">Local IRC</GlassTabsTrigger>
                      <GlassTabsTrigger className="subtab-trigger" value="timeline">Timeline</GlassTabsTrigger>
                    </GlassTabsList>
                  </GlassTabs>
                  <div className="context-line">
                    {activeFeed === 'combined' ? 'Chat and backend events in one stream.' : activeFeed === 'chat' ? 'Chat messages only.' : 'Runtime and system timeline only.'}
                  </div>
                </div>

                <GlassScrollArea className="feed-scroll glass-inset">
                  <div className="feed-stack">
                    {activeItems.map((item) => <FeedMessage key={item.key} item={item} />)}
                  </div>
                </GlassScrollArea>

                <div className="composer-shell">
                  <GlassTextarea
                    value={composer}
                    onChange={(event) => setComposer(event.currentTarget.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' && !event.shiftKey) {
                        event.preventDefault();
                        void submitPrompt();
                      }
                    }}
                    placeholder="Type a local prompt, send to Twitch, or use Mic On for browser speech..."
                    className="composer-textarea"
                  />
                  <div className="composer-toolbar">
                    <div className="composer-actions">
                      <GlassButton variant="primary" onClick={() => void submitPrompt()}><IconSparkles size={16} />Send To AI</GlassButton>
                      <GlassButton variant="default" onClick={() => void submitTwitch()}><IconBrandTwitch size={16} />Send To Twitch</GlassButton>
                      {voiceSession.micEnabled ? (
                        <GlassButton variant="destructive" onClick={() => void stopMic()}><IconPlayerStop size={16} />Mic Off</GlassButton>
                      ) : (
                        <GlassButton variant="default" onClick={() => void startMic()}><IconMicrophone size={16} />Mic On</GlassButton>
                      )}
                    </div>
                    <div className="composer-stats">
                      <GlassBadge variant="outline">Interim {voiceSession.interimText || 'waiting'}</GlassBadge>
                      <GlassBadge variant="outline">Final {voiceSession.lastFinalText || 'waiting'}</GlassBadge>
                    </div>
                  </div>
                </div>
              </div>
            ) : null}

            {mainTab === 'twitch' ? (
              <div className="panel-stack">
                <div className="panel-copy">
                  Save the Twitch Client ID and redirect URL once, then connect Bot, connect Streamer, and finally connect Chat. The redirect URL in this build is <code>{oauthSettings.redirectUrl}</code>.
                </div>
                <div className="two-col-grid">
                  <LabeledField label="Client ID">
                    <GlassInput value={oauthSettings.clientId} placeholder="Twitch client ID" onChange={(event) => setOauthSettings((current) => ({ ...current, clientId: event.currentTarget.value }))} />
                  </LabeledField>
                  <LabeledField label="Redirect URL">
                    <GlassInput value={oauthSettings.redirectUrl} onChange={(event) => setOauthSettings((current) => ({ ...current, redirectUrl: event.currentTarget.value }))} />
                  </LabeledField>
                </div>
                <div className="action-grid">
                  <GlassButton variant="default" onClick={() => void openExternal('https://dev.twitch.tv/console/apps/create')}><IconWorld size={16} />Open Twitch App Setup</GlassButton>
                  <GlassButton variant="default" onClick={() => void saveOauth()}>Save OAuth Settings</GlassButton>
                  <GlassButton variant="primary" onClick={() => void startTwitchOauth(false, 'bot-default', 'bot')}>Connect Bot</GlassButton>
                  <GlassButton variant="primary" onClick={() => void startTwitchOauth(false, 'streamer-default', 'streamer')}>Connect Streamer</GlassButton>
                </div>
                <div className="action-grid">
                  <GlassButton variant="primary" onClick={() => void connectTwitchChat()}>Connect Chat</GlassButton>
                  <GlassButton variant="default" onClick={() => void disconnectTwitchChat()}>Disconnect Chat</GlassButton>
                  <GlassButton variant="default" onClick={() => void clearBotSession().then(loadAll)}>Disconnect Bot</GlassButton>
                  <GlassButton variant="default" onClick={() => void clearStreamerSession().then(loadAll)}>Disconnect Streamer</GlassButton>
                  <GlassButton variant="destructive" onClick={() => void clearAuthSessions().then(loadAll)}>Reset Auth</GlassButton>
                </div>
                <div className="inline-badges">
                  <GlassBadge variant={auth.botTokenPresent ? 'success' : 'warning'}>Bot {auth.botTokenPresent ? 'connected' : 'missing'}</GlassBadge>
                  <GlassBadge variant={auth.streamerTokenPresent ? 'success' : 'warning'}>Streamer {auth.streamerTokenPresent ? 'connected' : 'missing'}</GlassBadge>
                  <GlassBadge variant="outline">Channel {auth.broadcasterLogin || auth.channel || 'not set'}</GlassBadge>
                </div>
              </div>
            ) : null}

            {mainTab === 'cloud' ? (
              <div className="panel-stack">
                <div className="panel-copy">
                  This list is intentionally short: four conversational models and four uncensored models only. The dropdown reflects the curated set, not the entire account catalog.
                </div>
                <div className="two-col-grid">
                  <LabeledField label="Ollama API key">
                    <GlassInput type="password" value={cloudApiKey} placeholder="Paste Ollama API key" onChange={(event) => setCloudApiKey(event.currentTarget.value)} />
                  </LabeledField>
                  <LabeledField label="Selected model">
                    <GlassSelect value={selectedModel} onValueChange={setSelectedModel}>
                      <GlassSelectTrigger>
                        <GlassSelectValue placeholder="Select a model" />
                      </GlassSelectTrigger>
                      <GlassSelectContent>
                        <GlassSelectGroup>
                          {cloudModels.map((model) => (
                            <GlassSelectItem key={model.id} value={model.id}>
                              {model.uncensored ? 'Uncensored' : 'Conversational'} · {model.label}
                            </GlassSelectItem>
                          ))}
                        </GlassSelectGroup>
                      </GlassSelectContent>
                    </GlassSelect>
                  </LabeledField>
                </div>
                <div className="action-grid">
                  <GlassButton variant="default" onClick={() => void openExternal('https://ollama.com')}><IconWorld size={16} />Open Ollama</GlassButton>
                  <GlassButton variant="default" onClick={() => void openExternal('https://ollama.com/settings/keys')}><IconWorld size={16} />Open API Keys</GlassButton>
                  <GlassButton variant="default" onClick={() => void refreshCloudModels()}>Check Cloud Models</GlassButton>
                  <GlassButton variant="primary" onClick={() => void enableCloudModel()}>Enable Cloud-Only Mode</GlassButton>
                </div>
                {(() => {
                  const activeModel = cloudModels.find((model) => model.id === selectedModel) ?? cloudModels[0];
                  return activeModel ? (
                    <GlassCard className="glass-surface inset-card compact-model-card">
                      <div className="inset-content">
                        <div className="section-title">Selected Model</div>
                        <div className="model-row-title">{activeModel.label}</div>
                        <div className="panel-copy">{activeModel.style}</div>
                        <div className="inline-badges">
                          <GlassBadge variant={activeModel.uncensored ? 'destructive' : 'primary'} size="sm">{activeModel.context}</GlassBadge>
                          <GlassBadge variant={activeModel.available ? 'success' : 'outline'} size="sm">{activeModel.available ? 'Detected on account' : 'Curated preset'}</GlassBadge>
                        </div>
                      </div>
                    </GlassCard>
                  ) : null;
                })()}
                <div className="panel-copy">{cloudStatus}</div>
              </div>
            ) : null}

            {mainTab === 'settings' ? (
              <div className="panel-stack settings-tab-grid">
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Runtime Controls</div>
                    <div className="runtime-grid">
                      <RuntimeToggle label="Voice replies" description="Allow spoken replies and avatar speech animation." checked={voiceConfig.enabled} onChange={(checked) => {
                        setVoiceConfig((current) => ({ ...current, enabled: checked }));
                        void setVoiceEnabled(checked);
                      }} />
                      <RuntimeToggle label="Keep talking" description="Stay on subject and prefer statements over repeated questions." checked={behavior.topicContinuationMode ?? false} onChange={(checked) => void patchBehavior({ topicContinuationMode: checked })} />
                      <RuntimeToggle label="Bot posting to Twitch" description="Keep connected to Twitch without forcing bot replies into the channel." checked={behavior.postBotMessagesToTwitch ?? false} onChange={(checked) => void patchBehavior({ postBotMessagesToTwitch: checked })} />
                      <RuntimeToggle label="Auto comments" description="Autonomous chatter based on the pacing slider below." checked={behavior.cohostMode} onChange={(checked) => void patchBehavior({ cohostMode: checked })} />
                    </div>
                    <div className="runtime-grid sliders-grid">
                      <SliderField label="Chatiness" value={Math.max(0, Math.min(100, Math.round((60_000 - (behavior.minimumReplyIntervalMs ?? 9000)) / 600)))} min={0} max={100} onChange={(value) => {
                        const interval = Math.max(1500, 60_000 - value * 600);
                        void patchBehavior({ minimumReplyIntervalMs: interval, scheduledMessagesMinutes: value > 0 ? Math.max(1, Math.round((interval / 1000) / 8)) : null });
                      }} />
                      <SliderField label="Voice volume" value={voiceConfig.volumePercent ?? 100} min={0} max={100} onChange={(value) => {
                        setVoiceConfig((current) => ({ ...current, volumePercent: value }));
                        void setTtsVolume(value);
                      }} />
                    </div>
                  </div>
                </GlassCard>
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Voice Diagnostics</div>
                    <div className="diag-grid">
                      <div className="diag-tile"><span className="diag-label">STT engine</span><span className="diag-value">{voiceSession.engine}</span></div>
                      <div className="diag-tile"><span className="diag-label">First interim</span><span className="diag-value">{voiceSession.firstInterimLatencyMs ?? 0} ms</span></div>
                      <div className="diag-tile"><span className="diag-label">Final latency</span><span className="diag-value">{voiceSession.finalLatencyMs ?? 0} ms</span></div>
                      <div className="diag-tile"><span className="diag-label">AI latency</span><span className="diag-value">{voiceSession.aiLatencyMs ?? 0} ms</span></div>
                    </div>
                    <div className="panel-copy small-copy">{voiceRuntime?.checks?.map((check) => `${check.name}: ${check.status}`).join(' · ') || 'Runtime checks pending.'}</div>
                  </div>
                </GlassCard>
              </div>
            ) : null}


            {mainTab === 'character' ? (
              <div className="character-pane">
                <div className="subtab-row">
                  <GlassTabs value={characterTab} onValueChange={(value) => setCharacterTab(value as typeof characterTab)}>
                    <GlassTabsList className="folder-tabs-list">
                      <GlassTabsTrigger className="folder-tab-trigger" value="persona">Persona Folder</GlassTabsTrigger>
                      <GlassTabsTrigger className="folder-tab-trigger" value="stage">Rig & Stage Folder</GlassTabsTrigger>
                    </GlassTabsList>
                  </GlassTabs>
                </div>

                {characterTab === 'persona' ? (
                  <div className="panel-stack">
                    <div className="preset-grid">
                      {characterPresets.map((preset) => (
                        <button key={preset.id} type="button" className={`preset-tile ${character.selectedPreset === preset.id ? 'active' : ''}`} onClick={() => void patchCharacter({ ...preset.tuning, selectedPreset: preset.id })}>
                          <span className="preset-name">{preset.displayName}</span>
                          <span className="preset-meta">{preset.category} · {voiceLabel(preset.defaultVoice)}</span>
                        </button>
                      ))}
                    </div>
                    <div className="two-col-grid persona-summary-grid">
                      <GlassCard className="glass-surface inset-card">
                        <div className="inset-content">
                          <div className="section-title">Selected Character</div>
                          <div className="selected-name">{activePreset.displayName}</div>
                          <div className="panel-copy">{activePreset.description}</div>
                          <div className="inline-badges">
                            <GlassBadge variant="primary">{activePreset.category}</GlassBadge>
                            <GlassBadge variant="outline">{activePreset.voiceSummary}</GlassBadge>
                          </div>
                        </div>
                      </GlassCard>
                      <GlassCard className="glass-surface inset-card">
                        <div className="inset-content">
                          <div className="section-title">Default Voice</div>
                          <LabeledField label="Character voice">
                            <GlassSelect value={voiceConfig.voiceName || activePreset.defaultVoice} onValueChange={(value) => {
                              setVoiceConfig((current) => ({ ...current, voiceName: value }));
                              void setTtsVoice(value);
                            }}>
                              <GlassSelectTrigger>
                                <GlassSelectValue placeholder="Select a voice" />
                              </GlassSelectTrigger>
                              <GlassSelectContent>
                                <GlassSelectGroup>
                                  {voiceOptions.map((voice) => (
                                    <GlassSelectItem key={voice.value} value={voice.value}>{voice.label}</GlassSelectItem>
                                  ))}
                                </GlassSelectGroup>
                              </GlassSelectContent>
                            </GlassSelect>
                          </LabeledField>
                          <div className="panel-copy">{activePreset.voiceSummary}</div>
                        </div>
                      </GlassCard>
                    </div>
                    <div className="tuning-grid-compact">
                      <SliderField label="Warmth" value={character.warmth} min={0} max={100} onChange={(value) => void patchCharacter({ warmth: value })} />
                      <SliderField label="Humor" value={character.humor} min={0} max={100} onChange={(value) => void patchCharacter({ humor: value })} />
                      <SliderField label="Flirt" value={character.flirt} min={0} max={100} onChange={(value) => void patchCharacter({ flirt: value })} />
                      <SliderField label="Edge" value={character.edge} min={0} max={100} onChange={(value) => void patchCharacter({ edge: value })} />
                      <SliderField label="Energy" value={character.energy} min={0} max={100} onChange={(value) => void patchCharacter({ energy: value })} />
                      <SliderField label="Story" value={character.story} min={0} max={100} onChange={(value) => void patchCharacter({ story: value })} />
                    </div>
                    <LabeledField label="Extra direction" hint="This gets merged into the active personality prompt.">
                      <GlassTextarea value={character.extraDirection} onChange={(event) => void patchCharacter({ extraDirection: event.currentTarget.value })} className="short-textarea" />
                    </LabeledField>
                    <div className="action-grid">
                      <GlassButton variant="primary" onClick={() => void applyCharacterPreset()}><IconWand size={16} />Apply Character</GlassButton>
                    </div>
                  </div>
                ) : null}

                {characterTab === 'stage' ? (
                  <div className="panel-stack stage-stack">
                    <div className="character-rig-toolbar">
                      <div className="character-rig-toolbar-upload">
                        <LabeledField label="Avatar image" hint="Ideal source portrait: 1200×1800. Minimum: 900×1400. Keep the full head centered with extra forehead and chin room.">
                          <GlassInput type="file" accept="image/*" onChange={(event) => void handleAvatarFile(event.currentTarget.files?.[0] || null)} />
                        </LabeledField>
                      </div>
                      <div className="action-grid compact-actions">
                        <GlassButton variant="default" onClick={() => void saveAvatarRig()}>Save Rig</GlassButton>
                        <GlassButton variant="default" onClick={() => void openAvatarPopout()}>Open Popup</GlassButton>
                      </div>
                    </div>
                    <AvatarRuntime
                      avatarSrc={avatarImage?.dataUrl || '/floating-head.png'}
                      rig={avatarRig}
                      onRigChange={(patch) => setAvatarRig((current) => ({ ...current, ...patch }))}
                      onRigSave={() => void saveAvatarRig()}
                      onPopout={() => void openAvatarPopout()}
                      onSnap={(size) => void snapAvatarWindowToImage(size)}
                    />
                  </div>
                ) : null}
              </div>
            ) : null}
          </GlassCard>
        </div>

      </div>
    </div>
  );
}