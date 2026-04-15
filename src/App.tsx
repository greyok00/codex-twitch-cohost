import '@fontsource/manrope/index.css';
import './app.css';

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  IconBrandTwitch,
  IconCpu,
  IconMicrophone,
  IconPlayerStop,
  IconSparkles,
  IconVolume,
  IconWorld
} from '@tabler/icons-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { BrowserSpeechEngine, browserSpeechSupported } from './lib/voice-session/engines/browserSpeech';
import { LocalFallbackSpeechEngine } from './lib/voice-session/engines/localFallback';
import { WorkerBackedTranscriptService } from './lib/voice-session/WorkerBackedTranscriptService';
import { buildVoiceInputFrame } from './lib/voice-session/VoiceFrameBuilder';
import { AvatarRuntime, type AvatarNaturalSize } from './avatar-runtime';
import {
  composeDirectProfile,
  defaultToneStudioSettings,
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
import { GlassSlider } from './components/ui/glass-slider';
import { GlassSwitch } from './components/ui/glass-switch';
import { GlassTabs, GlassTabsList, GlassTabsTrigger } from './components/ui/glass-tabs';
import {
  clearAuthSessions,
  clearBotSession,
  clearStreamerSession,
  autoConfigureSttFast,
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
  topicContinuationMode: true
};

const defaultCharacter: CharacterStudioSettings = defaultToneStudioSettings;

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
  const [selectedModel, setSelectedModel] = useState('dolphin-mistral');
  const [cloudStatus, setCloudStatus] = useState('');
  const [mainTab, setMainTab] = useState<'chat' | 'twitch' | 'cloud' | 'voice' | 'settings'>('chat');
  const [characterTab, setCharacterTab] = useState<'voice' | 'stage'>('voice');
  const [avatarImage, setAvatarImage] = useState<AvatarImage | null>(null);
  const [avatarRig, setAvatarRig] = useState<AvatarRigSettings>(defaultAvatarRig);
  const [chat, setChat] = useState<ChatMessage[]>([]);
  const [timeline, setTimeline] = useState<EventMessage[]>([]);
  const [composer, setComposer] = useState('');
  const [activeFeed, setActiveFeed] = useState<'combined' | 'chat' | 'timeline'>('combined');
  const [voiceSession, setVoiceSession] = useState<VoiceSessionState>(defaultVoiceSession);
  const [banner, setBanner] = useState<string | null>(null);

  const activePreset = useMemo(
    () => findVoicePresetByVoice(voiceConfig.voiceName) ?? findVoicePresetById(character.selectedPreset) ?? voicePresets[0],
    [character.selectedPreset, voiceConfig.voiceName]
  );
  const transcriptServiceRef = useRef<WorkerBackedTranscriptService | null>(null);
  const speechEngineRef = useRef<SpeechEngine | null>(null);
  const ttsAudioRef = useRef<HTMLAudioElement | null>(null);
  const voiceConfigRef = useRef<TtsVoiceSettings>(voiceConfig);
  const voiceSessionRef = useRef<VoiceSessionState>(voiceSession);
  const bannerTimeoutRef = useRef<number | null>(null);
  const characterPersistTimeoutRef = useRef<number | null>(null);
  const aiStartRef = useRef<number>(0);
  const sttBootstrapRef = useRef(false);

  const flashBanner = (message: string, timeoutMs = 5000) => {
    setBanner(message);
    if (bannerTimeoutRef.current) window.clearTimeout(bannerTimeoutRef.current);
    bannerTimeoutRef.current = window.setTimeout(() => setBanner(null), timeoutMs);
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

  const ensureSttReady = async (report: VoiceRuntimeReport | null) => {
    if (sttBootstrapRef.current || report?.sttReady) return report;
    sttBootstrapRef.current = true;
    try {
      const configured = await autoConfigureSttFast() as SttAutoConfigResult;
      const refreshed = await verifyVoiceRuntime().catch(() => report);
      if (refreshed) setVoiceRuntime(refreshed);
      if (configured.applied) {
        flashBanner('STT auto-configured.');
      }
      return refreshed ?? report;
    } catch {
      return report;
    }
  };

  const loadAll = async () => {
    const [nextStatus, nextAuth, nextBehavior, nextCharacter, nextVoice, nextRuntime, nextOauth, savedCloudKey, savedAvatarImage, nextAvatarRig] = await Promise.all([
      getStatus(),
      getAuthSessions(),
      getBehaviorSettings(),
      getCharacterStudioSettings().catch(() => defaultCharacter),
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
    await setCharacterStudioSettings(syncedCharacter).catch(() => undefined);
    await savePersonality(composeDirectProfile(syncedCharacter, resolvedVoice)).catch(() => undefined);

    const repairedRuntime = await ensureSttReady(nextRuntime);
    if (repairedRuntime) {
      setVoiceRuntime(repairedRuntime);
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
        const currentVoiceConfig = voiceConfigRef.current;
        if (!currentVoiceConfig.enabled) return;
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
      if (characterPersistTimeoutRef.current) window.clearTimeout(characterPersistTimeoutRef.current);
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
    const currentVoiceConfig = voiceConfigRef.current;
    stopSpeechPlayback();
    emitAvatarEvent('speak_start', { text });
    const runtime = window as Window & { __cohost_tts_speaking?: boolean; __cohost_tts_suppressed_until?: number };
    runtime.__cohost_tts_speaking = true;
    runtime.__cohost_tts_suppressed_until = Date.now() + 30_000;
    try {
      const dataUrl = await synthesizeTtsCloud(text, currentVoiceConfig.voiceName && currentVoiceConfig.voiceName !== 'auto' ? currentVoiceConfig.voiceName : null);
      await new Promise<void>((resolve) => {
        const audio = new Audio(dataUrl);
        ttsAudioRef.current = audio;
        audio.volume = Math.max(0, Math.min(1, (currentVoiceConfig.volumePercent ?? 100) / 100));
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
    if (speechEngineRef.current) {
      await speechEngineRef.current.stop().catch(() => undefined);
      await speechEngineRef.current.dispose().catch(() => undefined);
      speechEngineRef.current = null;
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
    const initialEngineKind: SpeechEngine['kind'] = browserSpeechSupported() ? 'browser-speech' : 'local-fallback';
    setVoiceSession({ ...defaultVoiceSession(), sessionId, micEnabled: true, status: 'starting', engine: initialEngineKind });

    const callbacks: VoiceSessionCallbacks = {
      onInterim: (text: string) => {
        void transcriptService.pushInterim(text).then(({ interim, firstInterimLatencyMs }) => {
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
          engine: speechEngineRef.current?.kind ?? 'none',
          transcript: normalized.committed,
          finalLatencyMs: normalized.finalLatencyMs
        });
        await submitVoiceSessionFrame(frame, null);
      },
      onStatus: (nextStatus, detail?: string) => {
        setVoiceSession((state) => ({ ...state, status: nextStatus, lastError: nextStatus === 'error' ? detail ?? state.lastError : state.lastError }));
      },
      onError: (message: string) => {
        setVoiceSession((state) => ({ ...state, status: 'error', lastError: message }));
        flashBanner(`Mic error: ${message}`);
      },
      onSpeechStart: () => {
        transcriptService.setStartedAt(Date.now());
        (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = true;
        stopSpeechPlayback();
        setVoiceSession((state) => ({ ...state, speakingBlocked: true }));
      },
      onSpeechEnd: () => {
        (window as Window & { __cohost_recording_active?: boolean }).__cohost_recording_active = false;
        setVoiceSession((state) => ({ ...state, speakingBlocked: false }));
      }
    };

    const buildEngine = (preferBrowser: boolean): SpeechEngine =>
      preferBrowser && browserSpeechSupported()
        ? new BrowserSpeechEngine(callbacks)
        : new LocalFallbackSpeechEngine(callbacks);

    let engine = buildEngine(true);
    speechEngineRef.current = engine;
    try {
      await engine.start();
      setVoiceSession((state) => ({ ...state, engine: engine.kind }));
    } catch (error) {
      if (engine.kind === 'browser-speech') {
        const repairedRuntime = await ensureSttReady(voiceRuntime);
        if (repairedRuntime) setVoiceRuntime(repairedRuntime);
        engine = buildEngine(false);
        speechEngineRef.current = engine;
        await engine.start();
        setVoiceSession((state) => ({ ...state, engine: engine.kind }));
      } else if (browserSpeechSupported()) {
        engine = buildEngine(true);
        speechEngineRef.current = engine;
        await engine.start();
        setVoiceSession((state) => ({ ...state, engine: engine.kind }));
      } else {
        throw error;
      }
    }
  };

  const patchBehavior = async (patch: Partial<BehaviorSettings>) => {
    const next = { ...behavior, ...patch };
    setBehavior(next);
    await setBehaviorSettings(next);
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

  const applyVoiceSelection = async (voiceName: string) => {
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
    flashBanner(`Voice set to ${preset.displayName}.`);
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
      title: 'Avatar Stage',
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
                    <GlassTabsTrigger className="folder-tab-trigger" value="chat">Chat</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="twitch">Twitch</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="cloud">Models</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="voice">Voice</GlassTabsTrigger>
                    <GlassTabsTrigger className="folder-tab-trigger" value="settings">Settings</GlassTabsTrigger>
                  </GlassTabsList>
                </GlassTabs>
              <div className="tab-caption">
                {mainTab === 'chat' && 'Main conversation window with local chat, Twitch chat send, mic, and live feed.'}
                {mainTab === 'twitch' && 'OAuth, bot account, streamer account, and Twitch chat connection.'}
                {mainTab === 'cloud' && 'Curated conversational and uncensored Ollama cloud picks only.'}
                {mainTab === 'voice' && 'Direct voice selection plus tone sliders that shape the model response style.'}
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


            {mainTab === 'voice' ? (
              <div className="character-pane">
                <div className="subtab-row">
                  <GlassTabs value={characterTab} onValueChange={(value) => setCharacterTab(value as typeof characterTab)}>
                    <GlassTabsList className="folder-tabs-list">
                      <GlassTabsTrigger className="folder-tab-trigger" value="voice">Voice & Tone</GlassTabsTrigger>
                      <GlassTabsTrigger className="folder-tab-trigger" value="stage">Avatar Stage</GlassTabsTrigger>
                    </GlassTabsList>
                  </GlassTabs>
                </div>

                {characterTab === 'voice' ? (
                  <div className="panel-stack">
                    <div className="two-col-grid persona-summary-grid">
                      <GlassCard className="glass-surface inset-card">
                        <div className="inset-content">
                          <div className="section-title">Voice</div>
                          <LabeledField label="Select voice" hint="Low and mid-range voices only. Saves immediately.">
                            <GlassSelect value={voiceConfig.voiceName || activePreset.defaultVoice} onValueChange={(value) => void applyVoiceSelection(value)}>
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
                          </div>
                        </div>
                      </GlassCard>
                      <GlassCard className="glass-surface inset-card">
                        <div className="inset-content">
                          <div className="section-title">Direct Model Tone</div>
                          <div className="panel-copy">These sliders write straight into the active model prompt. No preset personalities, no hidden voice pairing.</div>
                          <div className="inline-badges">
                            <GlassBadge variant="outline">Warmth {Math.round(character.warmth)}</GlassBadge>
                            <GlassBadge variant="outline">Humor {Math.round(character.humor)}</GlassBadge>
                            <GlassBadge variant="outline">Edge {Math.round(character.edge)}</GlassBadge>
                          </div>
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
                    <LabeledField label="Extra direction" hint="Merged directly into the live model instruction.">
                      <GlassTextarea value={character.extraDirection} onChange={(event) => void patchCharacter({ extraDirection: event.currentTarget.value })} className="short-textarea" />
                    </LabeledField>
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
