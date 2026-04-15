import '@fontsource/manrope/index.css';
import './app.css';

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  IconBrandTwitch,
  IconCpu,
  IconKey,
  IconMicrophone,
  IconPlayerPlay,
  IconPlayerStop,
  IconSparkles,
  IconVolume,
  IconWorld
} from '@tabler/icons-react';
import { AssemblyAiBackendSpeechEngine } from './lib/voice-session/engines/assemblyAiBackend';
import { LocalFallbackSpeechEngine } from './lib/voice-session/engines/localFallback';
import { WorkerBackedTranscriptService } from './lib/voice-session/WorkerBackedTranscriptService';
import { buildVoiceInputFrame } from './lib/voice-session/VoiceFrameBuilder';
import { normalizeSpeechText, seemsWeakTranscript, stripWakeWords } from './lib/voice/utterance';
import {
  applyDeliveryPreset,
  composeDirectProfile,
  defaultToneStudioSettings,
  deliveryPresets,
  findDeliveryPreset,
  findVoicePresetById,
  findVoicePresetByVoice,
  voiceLabel,
  voicePresets
} from './lib/voice-tone';
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
import { GlassSwitch } from './components/ui/glass-switch';
import { GlassTabs, GlassTabsList, GlassTabsTrigger } from './components/ui/glass-tabs';
import {
  clearAuthSessions,
  clearBotSession,
  clearMemory,
  clearStreamerSession,
  autoConfigureSttFast,
  captureMicDebug,
  configureCloudOnlyMode,
  connectTwitchChat,
  disconnectTwitchChat,
  getAuthSessions,
  getBehaviorSettings,
  getCharacterStudioSettings,
  getProviderApiKey,
  getProviderModels,
  getStatus,
  getTtsVoice,
  getTwitchOauthSettings,
  onBotResponse,
  onChatMessage,
  onErrorBanner,
  onSttSetupProgress,
  onStatusUpdated,
  onTimelineEvent,
  openExternal,
  openMemoryLog,
  savePersonality,
  sendChatMessage,
  setBehaviorSettings,
  setCharacterStudioSettings,
  setAssemblyAiLiveSttPaused,
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
  BehaviorSettings,
  CharacterStudioSettings,
  ChatMessage,
  EventMessage,
  MicDebugView,
  SttAutoConfigResult,
  TwitchOauthSettings,
  TtsVoiceSettings,
  VoiceSessionState,
  VoiceRuntimeReport
} from './frontend-types';
import type { SpeechEngine, VoiceSessionCallbacks } from './lib/voice-session/types';

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
  topicContinuationMode: true,
  replyLengthMode: 'natural',
  allowBriefReactions: true
};

const defaultCharacter: CharacterStudioSettings = defaultToneStudioSettings;

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

const chatinessOptions = [
  { id: 'low', label: 'Low', intervalMs: 18000 },
  { id: 'medium', label: 'Medium', intervalMs: 10000 },
  { id: 'high', label: 'High', intervalMs: 4500 }
] as const;

const volumeOptions = [
  { id: 'low', label: 'Low', volumePercent: 45 },
  { id: 'medium', label: 'Medium', volumePercent: 70 },
  { id: 'high', label: 'High', volumePercent: 100 }
] as const;

function colorForUser(user: string) {
  const normalized = (user || 'unknown').toLowerCase().trim();
  if (normalized === 'greycohostai' || normalized.includes('cohost') || normalized === 'bot') {
    return '#a78bfa';
  }
  if (normalized === 'greyok__' || normalized === 'greyok' || normalized.includes('streamer')) {
    return '#22d3ee';
  }
  const palette = ['#60a5fa', '#f59e0b', '#34d399', '#a78bfa', '#f87171', '#22d3ee', '#facc15', '#94a3b8'];
  const source = normalized;
  let hash = 0;
  for (let i = 0; i < source.length; i += 1) hash = (hash * 31 + source.charCodeAt(i)) >>> 0;
  return palette[hash % palette.length];
}

function hexToRgba(hex: string, alpha: number) {
  const clean = hex.replace('#', '');
  const normalized = clean.length === 3
    ? clean.split('').map((char) => `${char}${char}`).join('')
    : clean;
  const value = parseInt(normalized, 16);
  const r = (value >> 16) & 255;
  const g = (value >> 8) & 255;
  const b = value & 255;
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
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

function normalizeSpeech(text: string) {
  return text
    .replace(/\bgreyok\b/gi, 'Grey Okay')
    .replace(/\bgreyok__\b/gi, 'Grey Okay')
    .replace(/\bgrey ok\b/gi, 'Grey Okay')
    .replace(/\b([A-Za-z]+)\s+(s|re|ve|ll|d|m)\b(?=(?:\s+[A-Za-z])|[.,!?]|$)/gi, "$1'$2")
    .replace(/\s+/g, ' ')
    .trim();
}

function buildSpokenTtsText(text: string) {
  const trimmed = text.trim();
  if (!trimmed) return '';
  const sentences = trimmed
    .split(/(?<=[.!?])\s+/)
    .map((part) => part.trim())
    .filter(Boolean);
  const base = sentences.slice(0, 2).join(' ').trim() || trimmed;
  const words = base.split(/\s+/).filter(Boolean);
  const clamped = words.slice(0, 30).join(' ').trim();
  const out = clamped || base;
  if (/[.!?]$/.test(out)) return out;
  return `${out}.`;
}

function normalizeFamily(model: string) {
  return model.toLowerCase().replace(/:(latest|[\w.\-]+)$/i, '');
}

function voiceEngineLabel(engine: VoiceSessionState['engine']) {
  if (engine === 'assemblyai-realtime') return 'AssemblyAI Cloud';
  if (engine === 'local-fallback') return 'Local Vosk';
  return 'Idle';
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

function choosePreferredModel(catalog: ModelMeta[], current?: string | null) {
  const currentMatch = current ? catalog.find((model) => model.id === current) : null;
  if (currentMatch?.uncensored && currentMatch.available) return currentMatch.id;
  return catalog.find((model) => model.uncensored && model.available)?.id
    ?? catalog.find((model) => model.uncensored)?.id
    ?? catalog.find((model) => model.available)?.id
    ?? catalog[0]?.id
    ?? 'dolphin-mistral';
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
  const accent = colorForUser(item.user);
  return (
    <div
      className={`feed-item ${item.tone}`}
      style={{
        ['--user-accent' as string]: accent,
        ['--user-bg' as string]: hexToRgba(accent, item.tone === 'event' ? 0.08 : 0.14),
        ['--user-bg-alt' as string]: hexToRgba(accent, item.tone === 'event' ? 0.03 : 0.06),
      }}
    >
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
  const [selectedModel, setSelectedModel] = useState('dolphin-mistral');
  const [cloudStatus, setCloudStatus] = useState('');
  const [assemblyApiKey, setAssemblyApiKey] = useState('');
  const [micDebugBusy, setMicDebugBusy] = useState(false);
  const [micDebug, setMicDebug] = useState<MicDebugView | null>(null);
  const [voiceDraft, setVoiceDraft] = useState('en-US-EmmaNeural');
  const [mainTab, setMainTab] = useState<'chat' | 'twitch' | 'ai' | 'voice' | 'runtime' | 'speech'>('chat');
  const [chat, setChat] = useState<ChatMessage[]>([]);
  const [timeline, setTimeline] = useState<EventMessage[]>([]);
  const [composer, setComposer] = useState('');
  const [activeFeed, setActiveFeed] = useState<'combined' | 'chat' | 'timeline'>('combined');
  const [voiceSession, setVoiceSession] = useState<VoiceSessionState>(defaultVoiceSession);
  const [sttSetupBusy, setSttSetupBusy] = useState(false);
  const [sttSetupMessage, setSttSetupMessage] = useState('');
  const [sttSetupProgress, setSttSetupProgress] = useState(0);
  const [appReady, setAppReady] = useState(false);
  const [startupOverlayVisible, setStartupOverlayVisible] = useState(true);
  const [startupMessage, setStartupMessage] = useState('Loading command center...');

  const activePreset = useMemo(
    () => findVoicePresetByVoice(voiceDraft || voiceConfig.voiceName) ?? findVoicePresetById(character.selectedPreset) ?? voicePresets[0],
    [character.selectedPreset, voiceConfig.voiceName, voiceDraft]
  );
  const activeDeliveryPreset = useMemo(() => findDeliveryPreset(character), [character]);
  const transcriptServiceRef = useRef<WorkerBackedTranscriptService | null>(null);
  const speechEngineRef = useRef<SpeechEngine | null>(null);
  const ttsAudioRef = useRef<HTMLAudioElement | null>(null);
  const voiceConfigRef = useRef<TtsVoiceSettings>(voiceConfig);
  const voiceSessionRef = useRef<VoiceSessionState>(voiceSession);
  const characterPersistTimeoutRef = useRef<number | null>(null);
  const aiStartRef = useRef<number>(0);
  const sttBootstrapRef = useRef(false);
  const recentBotRepliesRef = useRef<Array<{ normalized: string; at: number }>>([]);
  const activeBotSpeechTextRef = useRef<string | null>(null);
  const interruptedBotSpeechTextRef = useRef<string | null>(null);
  const pendingInterruptCaptureRef = useRef<{ at: number; source: string | null } | null>(null);
  const ttsSttReleaseTimeoutRef = useRef<number | null>(null);
  const ttsPlaybackTokenRef = useRef(0);
  const assemblyStartupWatchdogRef = useRef<number | null>(null);
  const lastSttNoticeRef = useRef<{ message: string; at: number } | null>(null);
  const lastVoiceTurnSubmitRef = useRef<{ at: number; normalized: string } | null>(null);

  const logRuntimeNotice = (message: string, kind = 'system') => {
    setTimeline((items) => [{
      id: `local_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      kind,
      content: message,
      timestamp: new Date().toISOString()
    }, ...items].slice(0, 300));
  };

  const logSttDiagnostic = (message: string, kind = 'stt', minGapMs = 2500) => {
    const normalized = message.trim();
    if (!normalized) return;
    const now = Date.now();
    const last = lastSttNoticeRef.current;
    if (last && last.message === normalized && now - last.at < minGapMs) return;
    lastSttNoticeRef.current = { message: normalized, at: now };
    logRuntimeNotice(normalized, kind);
  };

  const rememberBotReply = (text: string) => {
    const normalized = normalizeSpeechText(text);
    if (!normalized) return;
    const now = Date.now();
    recentBotRepliesRef.current = [{ normalized, at: now }, ...recentBotRepliesRef.current]
      .filter((entry, index, arr) => index === arr.findIndex((candidate) => candidate.normalized === entry.normalized))
      .filter((entry) => now - entry.at <= 90_000)
      .slice(0, 6);
  };

  const soundsLikeRecentBotReply = (text: string) => {
    const normalized = normalizeSpeechText(text);
    if (!normalized) return false;
    const now = Date.now();
    recentBotRepliesRef.current = recentBotRepliesRef.current.filter((entry) => now - entry.at <= 90_000);
    return recentBotRepliesRef.current.some((entry) => (
      normalized === entry.normalized
      || normalized.includes(entry.normalized)
      || entry.normalized.includes(normalized)
    ));
  };

  const soundsLikeActiveBotEcho = (text: string) => {
    const normalized = normalizeSpeechText(text);
    const activeNormalized = normalizeSpeechText(activeBotSpeechTextRef.current || '');
    if (!normalized || !activeNormalized) return false;
    if (normalized === activeNormalized || normalized.includes(activeNormalized) || activeNormalized.includes(normalized)) {
      return true;
    }
    const sampleWords = normalized.split(' ').filter(Boolean);
    const activeWords = activeNormalized.split(' ').filter(Boolean);
    if (sampleWords.length < 2 || activeWords.length < 2) return false;
    const overlap = sampleWords.filter((word) => activeWords.includes(word)).length;
    const ratio = overlap / Math.max(sampleWords.length, 1);
    return overlap >= 2 && ratio >= 0.6;
  };

  const isResumeIntent = (text: string) => {
    const normalized = normalizeSpeechText(text);
    return /^(continue|go on|keep going|finish that|finish what you were saying|what were you saying|resume|carry on)\b/.test(normalized);
  };

  const isInterruptIntent = (text: string) => {
    const normalized = normalizeSpeechText(text);
    return /^(stop|wait|hold on|shut up|quiet|pause|no wait|hang on|one second)\b/.test(normalized);
  };

  const salvageInterruptedTranscript = (text: string, source: string | null) => {
    const cleaned = text.trim();
    if (!cleaned) return '';
    const sourceNormalized = normalizeSpeechText(source || '');
    const cleanedWords = cleaned.split(/\s+/).filter(Boolean);
    const sourceWords = sourceNormalized.split(/\s+/).filter(Boolean);
    if (!cleanedWords.length) return '';
    let overlap = 0;
    const max = Math.min(cleanedWords.length, sourceWords.length, 18);
    for (let size = max; size >= 3; size -= 1) {
      const head = normalizeSpeechText(cleanedWords.slice(0, size).join(' '));
      const sourceHead = sourceWords.slice(0, size).join(' ');
      if (head && head === sourceHead) {
        overlap = size;
        break;
      }
    }
    let candidate = cleanedWords.slice(overlap).join(' ').trim();
    candidate = candidate.replace(/^(stop|wait|hold on|pause|hang on|one second)\b[\s,.:;-]*/i, '').trim();
    return candidate;
  };

  const salvageMixedBotTranscript = (text: string) => {
    const sources = [
      activeBotSpeechTextRef.current,
      ...recentBotRepliesRef.current.map((entry) => entry.normalized)
    ].filter(Boolean) as string[];
    for (const source of sources) {
      const salvaged = salvageInterruptedTranscript(text, source);
      if (
        salvaged
        && normalizeSpeechText(salvaged) !== normalizeSpeechText(text)
        && !seemsWeakTranscript(salvaged)
      ) {
        return salvaged;
      }
    }
    return '';
  };

  const releaseAssemblyPause = (afterMs = 0) => {
    if (ttsSttReleaseTimeoutRef.current) {
      window.clearTimeout(ttsSttReleaseTimeoutRef.current);
      ttsSttReleaseTimeoutRef.current = null;
    }
    if (afterMs <= 0) {
      void syncAssemblyPause(false);
      return;
    }
    ttsSttReleaseTimeoutRef.current = window.setTimeout(() => {
      void syncAssemblyPause(false);
      ttsSttReleaseTimeoutRef.current = null;
    }, afterMs);
  };

  const clearAssemblyStartupWatchdog = () => {
    if (assemblyStartupWatchdogRef.current) {
      window.clearTimeout(assemblyStartupWatchdogRef.current);
      assemblyStartupWatchdogRef.current = null;
    }
  };

  useEffect(() => {
    transcriptServiceRef.current = new WorkerBackedTranscriptService();
    return () => transcriptServiceRef.current?.dispose();
  }, []);

  useEffect(() => {
    voiceConfigRef.current = voiceConfig;
  }, [voiceConfig]);

  useEffect(() => {
    voiceSessionRef.current = voiceSession;
  }, [voiceSession]);

  const runSttAutoConfigure = async (report: VoiceRuntimeReport | null, force = false) => {
    if (!force && sttBootstrapRef.current && report?.sttReady) return report;
    setSttSetupBusy(true);
    setStartupMessage('Preparing local Vosk speech...');
    try {
      const configured = await autoConfigureSttFast() as SttAutoConfigResult;
      const refreshed = await verifyVoiceRuntime().catch(() => report);
      if (refreshed) setVoiceRuntime(refreshed);
      setSttSetupMessage(configured.message || '');
      if (configured.applied) {
        sttBootstrapRef.current = true;
        logRuntimeNotice('STT auto-configured.', 'stt');
      } else if (force) {
        logRuntimeNotice(configured.message || 'STT setup is still incomplete.', 'stt');
      }
      return refreshed ?? report;
    } catch (error) {
      if (force) {
        logRuntimeNotice(`STT setup failed: ${String(error)}`, 'stt_error');
      }
      return report;
    } finally {
      setSttSetupBusy(false);
    }
  };

  const ensureSttReady = async (report: VoiceRuntimeReport | null) => {
    if (report?.sttReady) return report;
    return runSttAutoConfigure(report);
  };

  const loadAll = async () => {
    const firstBoot = !appReady;
    if (firstBoot) {
      setAppReady(false);
      setStartupOverlayVisible(true);
    }
    const [nextStatus, nextAuth, nextBehavior, nextCharacter, nextVoice, nextRuntime, nextOauth, savedCloudKey, savedAssemblyKey] = await Promise.all([
      getStatus(),
      getAuthSessions(),
      getBehaviorSettings(),
      getCharacterStudioSettings().catch(() => defaultCharacter),
      getTtsVoice(),
      verifyVoiceRuntime().catch(() => null),
      getTwitchOauthSettings().catch(() => defaultOauthSettings),
      getProviderApiKey('ollama-cloud').catch(() => null),
      getProviderApiKey('assemblyai').catch(() => null)
    ]);

    setStatus(nextStatus);
    setAuth(nextAuth);
    setBehavior(nextBehavior);
    setVoiceRuntime(nextRuntime);
    setOauthSettings({
      clientId: nextOauth.clientId || '',
      botUsername: nextOauth.botUsername || '',
      channel: nextOauth.channel || '',
      broadcasterLogin: nextOauth.broadcasterLogin || '',
      redirectUrl: nextOauth.redirectUrl || defaultOauthSettings.redirectUrl
    });
    setCloudApiKey(savedCloudKey?.trim() || '');
    setAssemblyApiKey(savedAssemblyKey?.trim() || '');
    setStartupMessage('Applying saved voice and runtime settings...');

    let resolvedVoice = nextVoice.voiceName?.trim() || '';
    const savedCharacter = { ...defaultCharacter, ...nextCharacter };
    const matchedPreset = findVoicePresetByVoice(resolvedVoice);
    if (!resolvedVoice || resolvedVoice === 'auto') {
      resolvedVoice = findVoicePresetById(savedCharacter.selectedPreset)?.defaultVoice ?? matchedPreset.defaultVoice;
      await setTtsVoice(resolvedVoice).catch(() => undefined);
    }
    const syncedCharacter = { ...savedCharacter, selectedPreset: findVoicePresetByVoice(resolvedVoice).id };
    setCharacter(syncedCharacter);
    voiceConfigRef.current = { ...nextVoice, voiceName: resolvedVoice, volumePercent: nextVoice.volumePercent ?? 100 };
    setVoiceConfig(voiceConfigRef.current);
    setVoiceDraft(resolvedVoice);
    await setCharacterStudioSettings(syncedCharacter).catch(() => undefined);
    await savePersonality(composeDirectProfile(syncedCharacter, resolvedVoice)).catch(() => undefined);

    if (savedAssemblyKey?.trim()) {
      setSttSetupMessage('AssemblyAI realtime STT is configured.');
      setVoiceRuntime(nextRuntime);
    } else {
      const repairedRuntime = await ensureSttReady(nextRuntime);
      if (repairedRuntime) {
        setVoiceRuntime(repairedRuntime);
      }
    }
    setStartupMessage('Finalizing command center...');
    setAppReady(true);
    if (firstBoot) {
      setStartupOverlayVisible(false);
    }

    if (savedCloudKey?.trim()) {
      try {
        const models = await getProviderModels('ollama-cloud');
        const catalog = buildCatalog(models);
        const preferredModel = choosePreferredModel(catalog, nextStatus.model);
        setCloudModels(catalog);
        setSelectedModel(preferredModel);
        await configureCloudOnlyMode(preferredModel);
        setStatus((current) => ({ ...current, model: preferredModel }));
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
    void loadAll().catch((error) => {
      setStartupMessage('Startup hit an error.');
      logRuntimeNotice(String(error), 'startup_error');
      setAppReady(true);
      setStartupOverlayVisible(false);
    });

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
        const currentVoiceConfig = voiceConfigRef.current;
        if (!currentVoiceConfig.enabled) return;
        const clean = normalizeSpeech(payload.content);
        if (!clean) return;
        rememberBotReply(clean);
        (window as Window & { __cohost_last_bot_reply_at?: number }).__cohost_last_bot_reply_at = Date.now();
        void speakBotText(clean);
      }),
      onTimelineEvent((payload) => setTimeline((items) => [payload, ...items].slice(0, 300))),
      onStatusUpdated((payload) => setStatus(payload)),
      onSttSetupProgress((payload) => {
        setSttSetupBusy(payload.progress < 100);
        setSttSetupProgress(payload.progress);
        setSttSetupMessage(payload.message || '');
        setStartupMessage(payload.message || 'Preparing local Vosk speech...');
      }),
      onErrorBanner((payload) => {
        if (!payload.trim()) return;
        logRuntimeNotice(payload, 'error');
      })
    ]);

    return () => {
      window.clearInterval(every5);
      if (characterPersistTimeoutRef.current) window.clearTimeout(characterPersistTimeoutRef.current);
      void unsubs.then((list) => list.forEach((unsub) => unsub()));
      void stopMic();
      stopSpeechPlayback();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const stopSpeechPlayback = () => {
    const runtime = window as Window & {
      __cohost_tts_speaking?: boolean;
      __cohost_tts_suppressed_until?: number;
      __cohost_recording_active?: boolean;
      __cohost_tts_started_at?: number;
    };
    if (runtime.__cohost_tts_speaking && activeBotSpeechTextRef.current) {
      interruptedBotSpeechTextRef.current = activeBotSpeechTextRef.current;
    }
    runtime.__cohost_tts_speaking = false;
    runtime.__cohost_tts_started_at = 0;
    runtime.__cohost_tts_suppressed_until = Date.now() + 1500;
    ttsPlaybackTokenRef.current += 1;
    releaseAssemblyPause(0);
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

  const syncAssemblyPause = async (paused: boolean) => {
    if (speechEngineRef.current?.kind === 'assemblyai-realtime') {
      await setAssemblyAiLiveSttPaused(paused).catch(() => undefined);
    }
  };

  const speakBotText = async (text: string) => {
    const currentVoiceConfig = voiceConfigRef.current;
    const spokenText = buildSpokenTtsText(text);
    if (!spokenText) return;
    stopSpeechPlayback();
    const playbackToken = ttsPlaybackTokenRef.current;
    const runtime = window as Window & {
      __cohost_tts_speaking?: boolean;
      __cohost_tts_suppressed_until?: number;
      __cohost_tts_started_at?: number;
    };
    runtime.__cohost_tts_speaking = true;
    runtime.__cohost_tts_started_at = Date.now();
    runtime.__cohost_tts_suppressed_until = Date.now() + 30_000;
    void syncAssemblyPause(true);
    releaseAssemblyPause(900);
    activeBotSpeechTextRef.current = spokenText;
    rememberBotReply(spokenText);
    const startedAt = Date.now();
    try {
      const dataUrl = await synthesizeTtsCloud(spokenText, currentVoiceConfig.voiceName && currentVoiceConfig.voiceName !== 'auto' ? currentVoiceConfig.voiceName : null);
      if (playbackToken !== ttsPlaybackTokenRef.current || !runtime.__cohost_tts_speaking) {
        return;
      }
      setVoiceSession((state) => ({ ...state, ttsLatencyMs: Date.now() - startedAt }));
      await new Promise<void>((resolve) => {
        const audio = new Audio(dataUrl);
        if (playbackToken !== ttsPlaybackTokenRef.current || !runtime.__cohost_tts_speaking) {
          resolve();
          return;
        }
        ttsAudioRef.current = audio;
        audio.volume = Math.max(0, Math.min(1, (currentVoiceConfig.volumePercent ?? 100) / 100));
        audio.onended = () => resolve();
        audio.onerror = () => resolve();
        void audio.play().catch(() => resolve());
      });
    } catch {
      // no-op
    } finally {
      if (playbackToken !== ttsPlaybackTokenRef.current) {
        return;
      }
      runtime.__cohost_tts_speaking = false;
      runtime.__cohost_tts_started_at = 0;
      runtime.__cohost_tts_suppressed_until = Date.now() + 1500;
      releaseAssemblyPause(300);
      activeBotSpeechTextRef.current = null;
      ttsAudioRef.current = null;
    }
  };

  const previewVoiceSample = async () => {
    const sampleVoice = (voiceDraft || voiceConfig.voiceName || activePreset.defaultVoice || '').trim();
    const sampleText = `Alright, testing voice check. If you can hear this clean, this is the one.`;
    stopSpeechPlayback();
    try {
      const dataUrl = await synthesizeTtsCloud(sampleText, sampleVoice || null);
      await new Promise<void>((resolve) => {
        const audio = new Audio(dataUrl);
        ttsAudioRef.current = audio;
        audio.volume = Math.max(0, Math.min(1, (voiceConfigRef.current.volumePercent ?? 100) / 100));
        audio.onended = () => resolve();
        audio.onerror = () => resolve();
        void audio.play().catch(() => resolve());
      });
    } finally {
      ttsAudioRef.current = null;
    }
  };

  const stopMic = async () => {
    (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = false;
    if (speechEngineRef.current) {
      await speechEngineRef.current.stop().catch(() => undefined);
      await speechEngineRef.current.dispose().catch(() => undefined);
      speechEngineRef.current = null;
    }
    setVoiceSession((state) => ({ ...state, status: 'idle', micEnabled: false, interimText: '' }));
  };

  const startMic = async () => {
    const runtimeReport = voiceRuntime ?? await verifyVoiceRuntime().catch(() => null);
    const useAssemblyAi = Boolean(
      assemblyApiKey.trim()
      || runtimeReport?.checks?.some((check) =>
        check.name.toLowerCase().includes('stt key')
        || check.details.toLowerCase().includes('assemblyai')
      )
    );
    if (!useAssemblyAi && !runtimeReport?.sttReady) {
      const repaired = await runSttAutoConfigure(runtimeReport, true);
      if (!repaired?.sttReady) {
        setVoiceRuntime(repaired ?? runtimeReport);
        setVoiceSession((state) => ({ ...state, status: 'error', lastError: 'Vosk STT is not ready.' }));
        return;
      }
      setVoiceRuntime(repaired);
    }
    stopSpeechPlayback();
    await stopMic();
    const transcriptService = transcriptServiceRef.current;
    if (!transcriptService) return;
    const sessionId = `vs_${Math.random().toString(36).slice(2, 10)}`;
    transcriptService.setStartedAt(Date.now());
    await transcriptService.reset();
    const initialEngineKind: SpeechEngine['kind'] = useAssemblyAi ? 'assemblyai-realtime' : 'local-fallback';
    setVoiceSession({ ...defaultVoiceSession(), sessionId, micEnabled: true, status: 'starting', engine: initialEngineKind });
    logSttDiagnostic(
      useAssemblyAi
        ? 'STT starting with AssemblyAI Cloud.'
        : 'STT starting with Local Vosk.',
      'stt',
      500
    );

    let recoveringToLocalFallback = false;
    let switchToLocalFallback: ((reason: string) => Promise<void>) | null = null;

    const callbacks: VoiceSessionCallbacks = {
      onInterim: (text: string) => {
        clearAssemblyStartupWatchdog();
        void transcriptService.pushInterim(text).then(({ interim, firstInterimLatencyMs }) => {
          const cleanedInterim = stripWakeWords(interim).trim();
          const runtime = window as Window & {
            __cohost_tts_speaking?: boolean;
            __cohost_tts_started_at?: number;
          };
          const looksLikeBotEcho = soundsLikeRecentBotReply(cleanedInterim) || soundsLikeActiveBotEcho(cleanedInterim);
          const wordCount = cleanedInterim ? cleanedInterim.split(/\s+/).filter(Boolean).length : 0;
          const charCount = cleanedInterim.length;
          const ttsAgeMs = Math.max(0, Date.now() - (runtime.__cohost_tts_started_at ?? 0));
          const hasInterruptIntent = isInterruptIntent(cleanedInterim) || /\b(chatbot|chat bot|robot)\b/i.test(cleanedInterim);
          const strongNaturalBargeIn =
            wordCount >= 4
            && charCount >= 18
            && ttsAgeMs >= 1200
            && !seemsWeakTranscript(cleanedInterim);
          const explicitBargeIn =
            hasInterruptIntent
            && wordCount >= 1
            && charCount >= 4
            && ttsAgeMs >= 350;
          if (
            runtime.__cohost_tts_speaking
            && cleanedInterim
            && !looksLikeBotEcho
            && (explicitBargeIn || strongNaturalBargeIn)
          ) {
            pendingInterruptCaptureRef.current = {
              at: Date.now(),
              source: activeBotSpeechTextRef.current
            };
            transcriptService.setStartedAt(Date.now());
            void transcriptService.reset();
            logSttDiagnostic(`User barge-in detected: "${cleanedInterim}"`, 'stt', 800);
            stopSpeechPlayback();
          }
          setVoiceSession((state) => ({
            ...state,
            status: 'listening',
            interimText: interim,
            firstInterimLatencyMs: state.firstInterimLatencyMs ?? firstInterimLatencyMs,
            engine: speechEngineRef.current?.kind ?? state.engine
          }));
        });
      },
      onFinal: async (text: string) => {
        clearAssemblyStartupWatchdog();
        const normalized = await transcriptService.pushFinal(text);
        if (!normalized.committed) {
          logSttDiagnostic('Dropped transcript before final commit. The sample was too weak or incomplete.', 'stt', 1500);
          setVoiceSession((state) => ({ ...state, interimText: '', droppedCount: state.droppedCount + 1 }));
          return;
        }
        let cleaned = stripWakeWords(normalized.committed).trim();
        const pendingInterrupt = pendingInterruptCaptureRef.current;
        if (pendingInterrupt && Date.now() - pendingInterrupt.at <= 5000) {
          const salvaged = salvageInterruptedTranscript(cleaned, pendingInterrupt.source);
          if (salvaged && !seemsWeakTranscript(salvaged)) {
            logSttDiagnostic(`Recovered interrupted user speech: "${salvaged}"`, 'stt', 1200);
            cleaned = salvaged;
          } else {
            logSttDiagnostic(`Dropped mixed interrupt transcript: "${cleaned}"`, 'stt', 1200);
            pendingInterruptCaptureRef.current = null;
            setVoiceSession((state) => ({
              ...state,
              interimText: '',
              lastFinalText: cleaned || state.lastFinalText,
              droppedCount: state.droppedCount + 1
            }));
            return;
          }
          pendingInterruptCaptureRef.current = null;
        }
        const salvagedMixed = salvageMixedBotTranscript(cleaned);
        if (salvagedMixed) {
          logSttDiagnostic(`Recovered mixed bot/user transcript: "${salvagedMixed}"`, 'stt', 1200);
          cleaned = salvagedMixed;
        }
        const droppedForEmpty = !cleaned;
        const droppedForWeak = !isInterruptIntent(cleaned) && seemsWeakTranscript(cleaned);
        const droppedForEcho = soundsLikeRecentBotReply(cleaned) || soundsLikeActiveBotEcho(cleaned);
        if (droppedForEmpty || droppedForWeak || droppedForEcho) {
          if (droppedForEmpty) {
            logSttDiagnostic('Dropped transcript after wake-word cleanup because nothing meaningful remained.', 'stt', 1500);
          } else if (droppedForWeak) {
            logSttDiagnostic(`Dropped weak transcript: "${cleaned}"`, 'stt', 1500);
          } else if (droppedForEcho) {
            logSttDiagnostic(`Dropped likely bot-echo transcript: "${cleaned}"`, 'stt', 1500);
          }
          setVoiceSession((state) => ({
            ...state,
            interimText: '',
            lastFinalText: cleaned || state.lastFinalText,
            droppedCount: state.droppedCount + 1
          }));
          return;
        }
        const normalizedCleaned = normalizeSpeechText(cleaned);
        const recentSubmit = lastVoiceTurnSubmitRef.current;
        if (
          recentSubmit
          && Date.now() - recentSubmit.at <= 2800
          && (
            normalizedCleaned === recentSubmit.normalized
            || normalizedCleaned.includes(recentSubmit.normalized)
            || recentSubmit.normalized.includes(normalizedCleaned)
          )
        ) {
          logSttDiagnostic(`Dropped duplicate voice turn: "${cleaned}"`, 'stt', 1000);
          setVoiceSession((state) => ({
            ...state,
            interimText: '',
            lastFinalText: cleaned || state.lastFinalText,
            droppedCount: state.droppedCount + 1
          }));
          return;
        }
        aiStartRef.current = Date.now();
        setVoiceSession((state) => ({
          ...state,
          status: 'processing',
          interimText: '',
          lastFinalText: cleaned,
          finalLatencyMs: normalized.finalLatencyMs
        }));
        if (isResumeIntent(cleaned) && interruptedBotSpeechTextRef.current) {
          const resumeTarget = interruptedBotSpeechTextRef.current;
          interruptedBotSpeechTextRef.current = null;
          await submitVoiceSessionPrompt(
            `Resume your interrupted last thought naturally and briefly. Do not restart from the top. Pick up where you left off in a short spoken reply. Interrupted line: ${resumeTarget}`,
            null
          );
          return;
        }
        interruptedBotSpeechTextRef.current = null;
        const frame = await buildVoiceInputFrame({
          sessionId,
          mode: 'owner',
          engine: speechEngineRef.current?.kind ?? 'none',
          transcript: cleaned,
          finalLatencyMs: normalized.finalLatencyMs
        });
        lastVoiceTurnSubmitRef.current = {
          at: Date.now(),
          normalized: normalizedCleaned
        };
        await submitVoiceSessionFrame(frame, null);
      },
      onStatus: (nextStatus, detail?: string) => {
        setVoiceSession((state) => ({ ...state, status: nextStatus, lastError: nextStatus === 'error' ? detail ?? state.lastError : state.lastError }));
        const engineName = speechEngineRef.current?.kind === 'assemblyai-realtime' ? 'AssemblyAI Cloud' : speechEngineRef.current?.kind === 'local-fallback' ? 'Local Vosk' : 'STT';
        if (nextStatus === 'starting') {
          logSttDiagnostic(`${engineName} is starting.${detail ? ` ${detail}` : ''}`, 'stt', 1000);
        } else if (nextStatus === 'error') {
          logSttDiagnostic(`${engineName} status error.${detail ? ` ${detail}` : ''}`, 'stt_error', 1000);
        } else if (detail && /paused|stopped|failed|inactive|waiting|connected/i.test(detail)) {
          logSttDiagnostic(`${engineName}: ${detail}`, 'stt', 1500);
        }
      },
      onError: (message: string) => {
        setVoiceSession((state) => ({ ...state, status: 'error', lastError: message }));
        logSttDiagnostic(`Mic error: ${message}`, 'mic_error', 1000);
        if (speechEngineRef.current?.kind === 'assemblyai-realtime' && switchToLocalFallback) {
          void switchToLocalFallback(message);
        }
      },
      onSpeechStart: () => {
        transcriptService.setStartedAt(Date.now());
        (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = true;
        setVoiceSession((state) => ({ ...state, speakingBlocked: true }));
      },
      onSpeechEnd: () => {
        (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = false;
        setVoiceSession((state) => ({ ...state, speakingBlocked: false }));
      }
    };

    switchToLocalFallback = async (reason: string) => {
      if (recoveringToLocalFallback || !speechEngineRef.current || speechEngineRef.current.kind !== 'assemblyai-realtime') {
        return;
      }
      clearAssemblyStartupWatchdog();
      recoveringToLocalFallback = true;
      logRuntimeNotice(`AssemblyAI live mic failed, switching to local Vosk. ${reason}`, 'stt');
      await speechEngineRef.current.stop().catch(() => undefined);
      await speechEngineRef.current.dispose().catch(() => undefined);
      const localEngine = new LocalFallbackSpeechEngine(callbacks);
      speechEngineRef.current = localEngine;
      setVoiceSession((state) => ({
        ...state,
        engine: localEngine.kind,
        status: 'starting',
        lastError: reason
      }));
      try {
        await localEngine.start();
        setVoiceSession((state) => ({ ...state, engine: localEngine.kind, status: 'listening' }));
      } catch (fallbackError) {
        setVoiceSession((state) => ({
          ...state,
          status: 'error',
          engine: localEngine.kind,
          lastError: String(fallbackError)
        }));
        logRuntimeNotice(`Local Vosk fallback failed: ${String(fallbackError)}`, 'stt_error');
      }
    };

    const buildEngine = (preferAssemblyAi: boolean): SpeechEngine => preferAssemblyAi
      ? new AssemblyAiBackendSpeechEngine(callbacks)
      : new LocalFallbackSpeechEngine(callbacks);

    let engine = buildEngine(useAssemblyAi);
    speechEngineRef.current = engine;
    try {
      await engine.start();
      setVoiceSession((state) => ({ ...state, engine: engine.kind }));
      if (useAssemblyAi) {
        clearAssemblyStartupWatchdog();
        assemblyStartupWatchdogRef.current = window.setTimeout(() => {
          if (speechEngineRef.current?.kind === 'assemblyai-realtime') {
            setVoiceSession((state) => ({
              ...state,
              lastError: 'AssemblyAI is connected but no live transcript activity has appeared yet.'
            }));
            logRuntimeNotice('AssemblyAI is connected but no live transcript activity has appeared yet.', 'stt');
          }
        }, 12000);
      }
    } catch (error) {
      if (useAssemblyAi) {
        logRuntimeNotice(`AssemblyAI mic start failed, falling back to local STT. ${String(error)}`, 'stt');
        engine = buildEngine(false);
        speechEngineRef.current = engine;
        await engine.start();
        setVoiceSession((state) => ({ ...state, engine: engine.kind, lastError: null }));
        return;
      }
      if (!useAssemblyAi) {
        const repairedRuntime = await ensureSttReady(voiceRuntime);
        if (repairedRuntime) setVoiceRuntime(repairedRuntime);
      }
      engine = buildEngine(false);
      speechEngineRef.current = engine;
      await engine.start();
      setVoiceSession((state) => ({ ...state, engine: engine.kind }));
    }
  };

  const saveAssemblyAiKey = async () => {
    const trimmed = assemblyApiKey.trim();
    await setProviderApiKey('assemblyai', trimmed);
    const refreshed = await verifyVoiceRuntime().catch(() => voiceRuntime);
    if (refreshed) setVoiceRuntime(refreshed);
    setSttSetupMessage(trimmed ? 'AssemblyAI realtime STT is configured.' : 'AssemblyAI key cleared. Local Vosk will be used if available.');
    logRuntimeNotice(trimmed ? 'AssemblyAI key saved. Mic will use cloud STT.' : 'AssemblyAI key cleared.', 'stt');
  };

  const runMicDebugCapture = async () => {
    setMicDebugBusy(true);
    setMicDebug(null);
    try {
      await stopMic();
      const result = await captureMicDebug(2200);
      setMicDebug(result);
      logRuntimeNotice(
        `Mic debug captured via ${result.backend}. Transcript: ${result.transcript?.trim() || 'empty'}`,
        'stt'
      );
    } catch (error) {
      const message = String(error);
      logRuntimeNotice(`Mic debug failed: ${message}`, 'stt_error');
      setMicDebug({
        backend: 'error',
        wavPath: '',
        transcript: message,
        durationMs: 2200
      });
    } finally {
      setMicDebugBusy(false);
    }
  };

  const saveVoiceSelection = async () => {
    const voiceName = (voiceDraft || activePreset.defaultVoice).trim();
    const preset = findVoicePresetByVoice(voiceName);
    const nextCharacter = { ...character, selectedPreset: preset.id };
    if (characterPersistTimeoutRef.current) window.clearTimeout(characterPersistTimeoutRef.current);
    setCharacter(nextCharacter);
    voiceConfigRef.current = { ...voiceConfigRef.current, voiceName };
    setVoiceConfig((current) => ({ ...current, voiceName }));
    await Promise.all([
      setTtsVoice(voiceName),
      persistCharacterState(nextCharacter, voiceName)
    ]);
    logRuntimeNotice(`Voice set to ${preset.displayName}.`, 'voice');
  };

  const patchBehavior = async (patch: Partial<BehaviorSettings>) => {
    const previous = behavior;
    const next = { ...behavior, ...patch };
    setBehavior(next);
    await setBehaviorSettings(next);
    if (typeof patch.cohostMode === 'boolean' && patch.cohostMode !== previous.cohostMode) {
      logRuntimeNotice(patch.cohostMode ? 'Auto cohost enabled.' : 'Auto cohost disabled.', 'runtime');
      if (patch.cohostMode) {
        aiStartRef.current = Date.now();
        await submitVoiceSessionPrompt(
          'Auto cohost cue: say one short fresh line about what is happening right now, grounded in current chat and stream context, without repeating prior wording.',
          null
        ).catch(() => undefined);
      }
    }
    if (typeof patch.topicContinuationMode === 'boolean' && patch.topicContinuationMode !== previous.topicContinuationMode) {
      logRuntimeNotice(
        patch.topicContinuationMode ? 'Keep talking mode enabled.' : 'Keep talking mode disabled.',
        'runtime'
      );
    }
    if (typeof patch.postBotMessagesToTwitch === 'boolean' && patch.postBotMessagesToTwitch !== previous.postBotMessagesToTwitch) {
      logRuntimeNotice(
        patch.postBotMessagesToTwitch ? 'Bot Twitch posting enabled.' : 'Bot Twitch posting disabled.',
        'runtime'
      );
    }
    if (typeof patch.minimumReplyIntervalMs === 'number' && patch.minimumReplyIntervalMs !== previous.minimumReplyIntervalMs) {
      logRuntimeNotice(`Auto cohost pacing set to ${Math.round(patch.minimumReplyIntervalMs / 1000)}s reply interval.`, 'runtime');
    }
  };

  const persistCharacterState = async (next: CharacterStudioSettings, voiceNameOverride?: string | null) => {
    const activeVoice = voiceNameOverride && voiceNameOverride !== 'auto'
      ? voiceNameOverride
      : voiceConfigRef.current.voiceName && voiceConfigRef.current.voiceName !== 'auto'
        ? voiceConfigRef.current.voiceName
        : findVoicePresetById(next.selectedPreset)?.defaultVoice
          ?? voicePresets[0].defaultVoice;
    await Promise.all([
      setCharacterStudioSettings(next),
      savePersonality(composeDirectProfile(next, activeVoice))
    ]);
  };

  const scheduleCharacterPersistence = (next: CharacterStudioSettings) => {
    if (characterPersistTimeoutRef.current) window.clearTimeout(characterPersistTimeoutRef.current);
    characterPersistTimeoutRef.current = window.setTimeout(() => {
      void persistCharacterState(next).catch(() => undefined);
      characterPersistTimeoutRef.current = null;
    }, 220);
  };

  const patchCharacter = async (patch: Partial<CharacterStudioSettings>) => {
    const next = { ...character, ...patch };
    setCharacter(next);
    scheduleCharacterPersistence(next);
  };

  const saveOauth = async () => {
    await setTwitchOauthSettings({ clientId: oauthSettings.clientId, redirectUrl: oauthSettings.redirectUrl });
    logRuntimeNotice('Twitch OAuth settings saved.', 'oauth');
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
    logRuntimeNotice(`Cloud-only mode enabled with ${selectedModel}.`, 'model');
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
  const activeChatiness =
    chatinessOptions.reduce((best, option) => (
      Math.abs((behavior.minimumReplyIntervalMs ?? defaultBehavior.minimumReplyIntervalMs ?? 10000) - option.intervalMs)
      < Math.abs((behavior.minimumReplyIntervalMs ?? defaultBehavior.minimumReplyIntervalMs ?? 10000) - best.intervalMs)
        ? option
        : best
    ), chatinessOptions[1]);
  const activeVolume =
    volumeOptions.reduce((best, option) => (
      Math.abs((voiceConfig.volumePercent ?? 100) - option.volumePercent)
      < Math.abs((voiceConfig.volumePercent ?? 100) - best.volumePercent)
        ? option
        : best
    ), volumeOptions[1]);
  const liveSpeechText = voiceSession.interimText || voiceSession.lastFinalText || '';
  const liveSpeechState = voiceSession.interimText
    ? 'Listening now'
    : voiceSession.lastFinalText
      ? 'Last heard'
      : voiceSession.micEnabled
        ? 'Waiting for speech'
        : 'Mic idle';

  return (
    <div className="desktop-root">
      {startupOverlayVisible && !appReady ? (
        <div className="startup-overlay">
          <div className="startup-card">
            <div className="startup-title">Launching Co-Host</div>
            <div className="startup-subtitle">{startupMessage}</div>
            <div className="startup-progress-track">
              <div
                className="startup-progress-fill"
                style={{ width: `${Math.max(8, sttSetupBusy ? sttSetupProgress : 100)}%` }}
              />
            </div>
            <div className="startup-status-row">
              <span>{sttSetupBusy ? 'Preparing speech runtime' : 'Starting interface'}</span>
              <span>{sttSetupBusy ? `${Math.max(0, sttSetupProgress)}%` : 'Ready'}</span>
            </div>
            <div className="startup-note">
              {sttSetupMessage || 'Loading saved account, voice, and model settings.'}
            </div>
          </div>
        </div>
      ) : null}
      <GlassCard className="utility-strip glass-surface">
        <div className="utility-diagnostics utility-diagnostics-full">
          <div className="stt-summary">
            <div className="stt-summary-head">
              <span className="stt-summary-title">STT Tracking</span>
              <GlassBadge variant={assemblyApiKey.trim() || voiceRuntime?.sttReady ? (sttSetupBusy ? 'outline' : 'success') : 'destructive'} size="sm">
                {assemblyApiKey.trim() || voiceRuntime?.sttReady ? (sttSetupBusy ? 'Setting up' : 'Ready') : 'Missing'}
              </GlassBadge>
            </div>
            <div className="stt-summary-grid">
              <div className="stt-summary-item">
                <span className="stt-summary-label">Engine</span>
                <span className="stt-summary-value">{voiceEngineLabel(voiceSession.engine)}</span>
                
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Mic</span>
                <span className="stt-summary-value">{voiceSession.micEnabled ? 'Listening' : 'Idle'}</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Interim</span>
                <span className="stt-summary-value">{voiceSession.firstInterimLatencyMs ?? 0} ms</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Final</span>
                <span className="stt-summary-value">{voiceSession.finalLatencyMs ?? 0} ms</span>
              </div>
            </div>
            <div className="stt-summary-note">
              {sttSetupBusy
                ? (sttSetupMessage || 'Preparing local Vosk...')
                : assemblyApiKey.trim()
                  ? (sttSetupMessage || 'AssemblyAI cloud STT is configured and active.')
                  : voiceRuntime?.sttReady
                  ? (sttSetupMessage || 'Local Vosk runtime and model are ready.')
                  : (sttSetupMessage || 'Local Vosk is not ready yet.')}
            </div>
          </div>
          <div className="stt-summary">
            <div className="stt-summary-head">
              <span className="stt-summary-title">TTS Tracking</span>
              <GlassBadge variant={voiceConfig.enabled ? 'success' : 'outline'} size="sm">
                {voiceConfig.enabled ? 'Enabled' : 'Muted'}
              </GlassBadge>
            </div>
            <div className="stt-summary-grid">
              <div className="stt-summary-item">
                <span className="stt-summary-label">Voice</span>
                <span className="stt-summary-value">{voiceLabel(voiceConfig.voiceName || activePreset.defaultVoice)}</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Volume</span>
                <span className="stt-summary-value">{voiceConfig.volumePercent ?? 100}%</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Latency</span>
                <span className="stt-summary-value">{voiceSession.ttsLatencyMs ?? 0} ms</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">State</span>
                <span className="stt-summary-value">{voiceConfig.enabled ? 'Ready' : 'Off'}</span>
              </div>
            </div>
            <div className="stt-summary-note">
              Spoken replies use the selected voice and current volume setting.
            </div>
          </div>
          <div className="stt-summary">
            <div className="stt-summary-head">
              <span className="stt-summary-title">Twitch Tracking</span>
              <GlassBadge variant={status.twitchState === 'connected' ? 'success' : status.twitchState === 'connecting' ? 'outline' : 'destructive'} size="sm">
                {status.twitchState}
              </GlassBadge>
            </div>
            <div className="stt-summary-grid">
              <div className="stt-summary-item">
                <span className="stt-summary-label">Channel</span>
                <span className="stt-summary-value">{status.channel ? `#${status.channel}` : 'Unset'}</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Bot</span>
                <span className="stt-summary-value">{auth.botTokenPresent ? (auth.botUsername || 'Connected') : 'Missing'}</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Streamer</span>
                <span className="stt-summary-value">{auth.streamerTokenPresent ? (auth.broadcasterLogin || 'Connected') : 'Missing'}</span>
              </div>
              <div className="stt-summary-item">
                <span className="stt-summary-label">Model</span>
                <span className="stt-summary-value">{status.model}</span>
              </div>
            </div>
            <div className="stt-summary-note">
              Twitch auth and chat connectivity are tracked here live.
            </div>
          </div>
        </div>
      </GlassCard>

      <div className="workspace-grid">
        <div className="main-shell">
          <GlassCard className="glass-surface conversation-card">
            <div className="conversation-shell">
            <div className="main-tab-header">
                <GlassTabs value={mainTab} onValueChange={(value) => setMainTab(value as typeof mainTab)}>
                  <GlassTabsList className="folder-tabs-list main-folder-tabs">
                    <GlassTabsTrigger className="folder-tab-trigger" value="chat">Chat</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="ai">AI</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="twitch">Twitch</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="voice">Voice</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="runtime">Runtime</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="speech">Speech</GlassTabsTrigger>
                  </GlassTabsList>
                </GlassTabs>
              <div className="tab-caption">
                {mainTab === 'chat' && 'Main conversation window with local chat, Twitch chat send, mic, and live feed.'}
                {mainTab === 'ai' && 'Model selection only.'}
                {mainTab === 'twitch' && 'OAuth, bot account, streamer account, and Twitch chat connection.'}
                {mainTab === 'voice' && 'Voice selection and direct tone controls for the co-host.'}
                {mainTab === 'runtime' && 'Runtime toggles, pacing, volume, and live latency diagnostics.'}
                {mainTab === 'speech' && 'Speech setup, transcription, and memory recovery.'}
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

                <div className="feed-region">
                  <div className="feed-scroll-native glass-inset">
                    <div className="feed-stack">
                      {activeItems.length > 0 ? activeItems.map((item) => <FeedMessage key={item.key} item={item} />) : (
                        <div className="feed-empty">
                          <div className="feed-empty-title">Feed Ready</div>
                          <div className="feed-empty-copy">Chat, timeline events, and co-host replies will appear here as soon as activity starts.</div>
                        </div>
                      )}
                    </div>
                  </div>
                </div>

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
                    placeholder="Type a local prompt, send to Twitch, or use Mic On for local Vosk speech..."
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
                    <div className="speech-monitor glass-inset">
                      <div className="speech-monitor-head">
                        <span className="speech-monitor-title">Live Speech</span>
                        <span className="speech-monitor-state">{liveSpeechState}</span>
                      </div>
                      <div className="speech-monitor-text">
                        {liveSpeechText || 'Start the mic and speak. Live transcription will appear here.'}
                      </div>
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
                  <GlassBadge variant={auth.botTokenPresent ? 'success' : 'destructive'}>Bot {auth.botTokenPresent ? 'connected' : 'missing'}</GlassBadge>
                  <GlassBadge variant={auth.streamerTokenPresent ? 'success' : 'destructive'}>Streamer {auth.streamerTokenPresent ? 'connected' : 'missing'}</GlassBadge>
                  <GlassBadge variant="outline">Channel {auth.broadcasterLogin || auth.channel || 'not set'}</GlassBadge>
                </div>
              </div>
            ) : null}

            {mainTab === 'ai' ? (
              <div className="ai-pane">
              <GlassScrollArea className="ai-scroll glass-inset">
              <div className="panel-stack settings-tab-grid ai-stack">
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Model</div>
                    <div className="panel-copy">
                      Curated conversational and uncensored Ollama cloud picks only.
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
                      <GlassButton variant="primary" onClick={() => void enableCloudModel()}>Use Selected Model</GlassButton>
                    </div>
                    {(() => {
                      const activeModel = cloudModels.find((model) => model.id === selectedModel) ?? cloudModels[0];
                      return activeModel ? (
                        <div className="inline-badges">
                          <GlassBadge variant={activeModel.uncensored ? 'accent' : 'info'} size="sm">{activeModel.context}</GlassBadge>
                          <GlassBadge variant={activeModel.available ? 'success' : 'outline'} size="sm">{activeModel.available ? 'Detected on account' : 'Curated preset'}</GlassBadge>
                          <GlassBadge variant="outline" size="sm">{activeModel.label}</GlassBadge>
                        </div>
                      ) : null;
                    })()}
                    <div className="panel-copy small-copy">{cloudStatus}</div>
                  </div>
                </GlassCard>
              </div>
              </GlassScrollArea>
              </div>
            ) : null}

            {mainTab === 'voice' ? (
              <div className="voice-pane">
              <GlassScrollArea className="voice-scroll glass-inset">
              <div className="panel-stack voice-stack">
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Voice Selection</div>
                    <div className="two-col-grid persona-summary-grid">
                      <div>
                        <LabeledField label="Select voice" hint="Low and mid-range voices only. Save explicitly after choosing.">
                          <GlassSelect value={voiceDraft || voiceConfig.voiceName || activePreset.defaultVoice} onValueChange={setVoiceDraft}>
                            <GlassSelectTrigger>
                              <GlassSelectValue placeholder="Select voice" />
                            </GlassSelectTrigger>
                            <GlassSelectContent>
                              <GlassSelectGroup>
                                {voicePresets.map((preset) => (
                                  <GlassSelectItem key={preset.id} value={preset.defaultVoice}>
                                    {preset.displayName} · {voiceLabel(preset.defaultVoice)}
                                  </GlassSelectItem>
                                ))}
                              </GlassSelectGroup>
                            </GlassSelectContent>
                          </GlassSelect>
                        </LabeledField>
                        <div className="selected-name">{activePreset.displayName}</div>
                        <div className="panel-copy">{activePreset.voiceSummary}</div>
                        <div className="inline-badges">
                          <GlassBadge variant="primary">{voiceLabel(activePreset.defaultVoice)}</GlassBadge>
                          <GlassBadge variant="outline">{voiceConfig.enabled ? 'TTS enabled' : 'TTS muted'}</GlassBadge>
                          <GlassBadge variant="outline">{(voiceDraft || voiceConfig.voiceName) === voiceConfig.voiceName ? 'Saved' : 'Unsaved'}</GlassBadge>
                        </div>
                        <div className="action-grid">
                          <GlassButton variant="primary" onClick={() => void saveVoiceSelection()}><IconVolume size={16} />Save Voice</GlassButton>
                          <GlassButton variant="default" onClick={() => void previewVoiceSample()}><IconPlayerPlay size={16} />Play Sample</GlassButton>
                        </div>
                      </div>
                      <div>
                        <div className="section-title compact-title">Delivery Control</div>
                        <div className="panel-copy">This shapes how the model responds without forcing robotic pitch or rate changes.</div>
                        <div className="inline-badges">
                          <GlassBadge variant="outline">{activeDeliveryPreset.label}</GlassBadge>
                          <GlassBadge variant="outline">{character.profanityAllowed ? 'Profanity on' : 'Profanity off'}</GlassBadge>
                          <GlassBadge variant="outline">{character.extraDirection.trim() ? 'Custom note active' : 'No custom note'}</GlassBadge>
                        </div>
                      </div>
                    </div>
                    <LabeledField label="Delivery mode" hint="Preset conversation styles for the AI personality and response feel.">
                      <GlassSelect
                        value={activeDeliveryPreset.id}
                        onValueChange={(value) => void patchCharacter(applyDeliveryPreset(value, character))}
                      >
                        <GlassSelectTrigger>
                          <GlassSelectValue placeholder="Select delivery mode" />
                        </GlassSelectTrigger>
                        <GlassSelectContent>
                          <GlassSelectGroup>
                            {deliveryPresets.map((preset) => (
                              <GlassSelectItem key={preset.id} value={preset.id}>
                                {preset.label}
                              </GlassSelectItem>
                            ))}
                          </GlassSelectGroup>
                        </GlassSelectContent>
                      </GlassSelect>
                    </LabeledField>
                    <div className="panel-copy">{activeDeliveryPreset.summary}</div>
                    <div className="runtime-grid">
                      <RuntimeToggle
                        label="Allow profanity"
                        description="Let the model swear when it fits naturally instead of forcing clean replies."
                        checked={character.profanityAllowed}
                        onChange={(checked) => void patchCharacter({ profanityAllowed: checked })}
                      />
                    </div>
                    <LabeledField label="Extra direction" hint="Merged directly into the live model instruction.">
                      <GlassTextarea value={character.extraDirection} onChange={(event) => void patchCharacter({ extraDirection: event.currentTarget.value })} className="short-textarea" />
                    </LabeledField>
                  </div>
                </GlassCard>
              </div>
              </GlassScrollArea>
              </div>
            ) : null}

            {mainTab === 'runtime' ? (
              <div className="voice-pane">
              <GlassScrollArea className="voice-scroll glass-inset">
              <div className="panel-stack voice-stack">
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Runtime Controls</div>
                    <div className="runtime-grid">
                      <RuntimeToggle label="Voice replies" description="Allow spoken replies from the co-host voice." checked={voiceConfig.enabled} onChange={(checked) => {
                        setVoiceConfig((current) => ({ ...current, enabled: checked }));
                        if (!checked) stopSpeechPlayback();
                        void setVoiceEnabled(checked);
                        logRuntimeNotice(checked ? 'Voice replies enabled.' : 'Voice replies disabled.', 'runtime');
                      }} />
                      <RuntimeToggle label="Keep talking" description="Stay on subject and prefer statements over repeated questions." checked={behavior.topicContinuationMode ?? false} onChange={(checked) => void patchBehavior({ topicContinuationMode: checked })} />
                      <RuntimeToggle label="Bot posting to Twitch" description="Keep connected to Twitch without forcing bot replies into the channel." checked={behavior.postBotMessagesToTwitch ?? false} onChange={(checked) => void patchBehavior({ postBotMessagesToTwitch: checked })} />
                      <RuntimeToggle label="Auto comments" description="Autonomous chatter based on the fixed pacing level below." checked={behavior.cohostMode} onChange={(checked) => void patchBehavior({ cohostMode: checked })} />
                      <RuntimeToggle label="Brief reactions" description="Allow quick human reactions like 'hmm', 'ugh', 'yeah', or 'no shot' when they fit." checked={behavior.allowBriefReactions ?? true} onChange={(checked) => void patchBehavior({ allowBriefReactions: checked })} />
                    </div>
                    <LabeledField label="Reply size" hint="Controls whether replies stay short, vary naturally, or run longer when the context supports it.">
                      <GlassSelect value={behavior.replyLengthMode ?? 'natural'} onValueChange={(value) => void patchBehavior({ replyLengthMode: value as 'short' | 'natural' | 'long' })}>
                        <GlassSelectTrigger>
                          <GlassSelectValue placeholder="Select reply size" />
                        </GlassSelectTrigger>
                        <GlassSelectContent>
                          <GlassSelectGroup>
                            <GlassSelectItem value="short">Short</GlassSelectItem>
                            <GlassSelectItem value="natural">Natural</GlassSelectItem>
                            <GlassSelectItem value="long">Long</GlassSelectItem>
                          </GlassSelectGroup>
                        </GlassSelectContent>
                      </GlassSelect>
                    </LabeledField>
                    <div className="runtime-grid">
                      <LabeledField label="Chattiness" hint="Fixed pacing levels so the bot behaves consistently instead of drifting between arbitrary slider values.">
                        <GlassSelect
                          value={activeChatiness.id}
                          onValueChange={(value) => {
                            const option = chatinessOptions.find((entry) => entry.id === value) ?? chatinessOptions[1];
                            void patchBehavior({
                              minimumReplyIntervalMs: option.intervalMs,
                              scheduledMessagesMinutes: Math.max(1, Math.round((option.intervalMs / 1000) / 8))
                            });
                          }}
                        >
                          <GlassSelectTrigger>
                            <GlassSelectValue placeholder="Select chattiness" />
                          </GlassSelectTrigger>
                          <GlassSelectContent>
                            <GlassSelectGroup>
                              {chatinessOptions.map((option) => (
                                <GlassSelectItem key={option.id} value={option.id}>{option.label}</GlassSelectItem>
                              ))}
                            </GlassSelectGroup>
                          </GlassSelectContent>
                        </GlassSelect>
                      </LabeledField>
                      <LabeledField label="Voice volume" hint="Discrete output levels instead of a continuous slider.">
                        <GlassSelect
                          value={activeVolume.id}
                          onValueChange={(value) => {
                            const option = volumeOptions.find((entry) => entry.id === value) ?? volumeOptions[1];
                            setVoiceConfig((current) => ({ ...current, volumePercent: option.volumePercent }));
                            void setTtsVolume(option.volumePercent);
                          }}
                        >
                          <GlassSelectTrigger>
                            <GlassSelectValue placeholder="Select voice volume" />
                          </GlassSelectTrigger>
                          <GlassSelectContent>
                            <GlassSelectGroup>
                              {volumeOptions.map((option) => (
                                <GlassSelectItem key={option.id} value={option.id}>{option.label}</GlassSelectItem>
                              ))}
                            </GlassSelectGroup>
                          </GlassSelectContent>
                        </GlassSelect>
                      </LabeledField>
                    </div>
                  </div>
                </GlassCard>
              </div>
              </GlassScrollArea>
              </div>
            ) : null}

            {mainTab === 'speech' ? (
              <div className="voice-pane">
              <GlassScrollArea className="voice-scroll glass-inset">
              <div className="panel-stack voice-stack">
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Speech Setup</div>
                    <div className="runtime-grid">
                      <LabeledField label="AssemblyAI API key" hint="Create a free AssemblyAI account, open API keys, paste the key here, then save. The mic will prefer AssemblyAI automatically when a key is present.">
                        <GlassInput type="password" value={assemblyApiKey} placeholder="Paste AssemblyAI API key" onChange={(event) => setAssemblyApiKey(event.currentTarget.value)} />
                      </LabeledField>
                      <div className="runtime-toggle">
                        <div className="runtime-toggle-copy">
                          <div className="runtime-toggle-title">Active STT path</div>
                          <div className="runtime-toggle-description">
                            {assemblyApiKey.trim()
                              ? 'AssemblyAI is the primary mic engine. Local Vosk stays available for fallback work.'
                              : (voiceRuntime?.sttReady
                                ? 'Local Vosk runtime and model are ready.'
                                : (sttSetupMessage || 'Use auto-configure to repair the local Vosk runtime and model.'))}
                          </div>
                        </div>
                        <div className="action-grid compact-actions">
                          <GlassButton variant="default" onClick={() => void openExternal('https://www.assemblyai.com/dashboard/signup')}><IconWorld size={16} />Open AssemblyAI</GlassButton>
                          <GlassButton variant="default" onClick={() => void openExternal('https://www.assemblyai.com/dashboard/api-keys')}><IconKey size={16} />Open API Keys</GlassButton>
                          <GlassButton variant="primary" onClick={() => void saveAssemblyAiKey()}>Save AssemblyAI Key</GlassButton>
                          <GlassButton variant="default" onClick={() => void runSttAutoConfigure(voiceRuntime, true)} disabled={sttSetupBusy}>
                            <IconMicrophone size={16} />
                            {sttSetupBusy ? 'Configuring...' : 'Repair Local Vosk'}
                          </GlassButton>
                        </div>
                      </div>
                    </div>
                  </div>
                </GlassCard>
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Mic Debug</div>
                    <div className="panel-copy">
                      Capture one short sample with the native mic backend and show the exact backend path and transcript result. Use this when STT says it is listening but nothing is arriving.
                    </div>
                    <div className="action-grid">
                      <GlassButton variant="default" onClick={() => void runMicDebugCapture()} disabled={micDebugBusy}>
                        <IconMicrophone size={16} />
                        {micDebugBusy ? 'Capturing...' : 'Run Mic Debug'}
                      </GlassButton>
                    </div>
                    {micDebug ? (
                      <div className="runtime-grid">
                        <LabeledField label="Backend">
                          <GlassInput value={micDebug.backend} readOnly />
                        </LabeledField>
                        <LabeledField label="Duration">
                          <GlassInput value={`${micDebug.durationMs} ms`} readOnly />
                        </LabeledField>
                        <LabeledField label="WAV path">
                          <GlassInput value={micDebug.wavPath || 'n/a'} readOnly />
                        </LabeledField>
                        <LabeledField label="Transcript">
                          <GlassTextarea value={micDebug.transcript || 'empty'} readOnly className="short-textarea" />
                        </LabeledField>
                      </div>
                    ) : null}
                  </div>
                </GlassCard>
                <GlassCard className="glass-surface inset-card settings-card">
                  <div className="inset-content">
                    <div className="section-title">Memory Banks</div>
                    <div className="panel-copy">Persistent memory banks now auto-capture profile, setup, and priority facts. Use reset if the bot gets poisoned by bad STT context.</div>
                    <div className="action-grid">
                      <GlassButton variant="default" onClick={() => void openMemoryLog()}><IconWorld size={16} />Open Memory Log</GlassButton>
                      <GlassButton variant="destructive" onClick={() => void clearMemory().then(() => logRuntimeNotice('Memory reset complete.', 'memory'))}>Reset Memory</GlassButton>
                    </div>
                  </div>
                </GlassCard>
              </div>
              </GlassScrollArea>
              </div>
            ) : null}
            </div>
          </GlassCard>
        </div>

      </div>
    </div>
  );
}
