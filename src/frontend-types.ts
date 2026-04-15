export type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';
export type ModuleLight = 'red' | 'yellow' | 'green' | string;

export interface ChatMessage {
  id: string;
  user: string;
  content: string;
  timestamp: string;
  isBot?: boolean;
}

export interface EventMessage {
  id: string;
  kind: string;
  content: string;
  timestamp: string;
}

export interface AppStatus {
  channel?: string;
  model: string;
  voiceEnabled: boolean;
  lurkMode: boolean;
  twitchState: ConnectionState;
}

export interface AuthSessions {
  botUsername: string;
  botTokenPresent: boolean;
  channel: string;
  broadcasterLogin?: string | null;
  streamerTokenPresent: boolean;
}

export interface TwitchOauthSettings {
  clientId: string;
  botUsername: string;
  channel: string;
  broadcasterLogin?: string | null;
  redirectUrl: string;
}

export interface BehaviorSettings {
  cohostMode: boolean;
  scheduledMessagesMinutes?: number | null;
  minimumReplyIntervalMs?: number | null;
  postBotMessagesToTwitch?: boolean;
  topicContinuationMode?: boolean;
  replyLengthMode?: 'short' | 'natural' | 'long';
  allowBriefReactions?: boolean;
}

export interface CharacterStudioSettings {
  selectedPreset: string;
  warmth: number;
  humor: number;
  flirt: number;
  edge: number;
  energy: number;
  story: number;
  profanityAllowed: boolean;
  extraDirection: string;
}

export interface AvatarImage {
  dataUrl: string;
  fileName?: string | null;
}

export interface AvatarRigSettings {
  mouthX: number;
  mouthY: number;
  mouthWidth: number;
  mouthOpen: number;
  mouthSoftness: number;
  mouthSmile: number;
  mouthTilt: number;
  mouthColor: string;
  browX: number;
  browY: number;
  browSpacing: number;
  browArch: number;
  browTilt: number;
  browThickness: number;
  browColor: string;
  eyeOpen: number;
  eyeSquint: number;
  headTilt: number;
  headScale: number;
  glow: number;
  popupWidth: number;
  popupHeight: number;
}

export interface PersonalityProfile {
  name: string;
  voice: string;
  tone: string;
  humor_level: number;
  aggression_level: number;
  friendliness: number;
  verbosity: number;
  streamer_relationship: string;
  response_style: string;
  lore: string;
  taboo_topics: string[];
  catchphrases: string[];
  reply_rules: string[];
  chat_behavior_rules: string[];
  viewer_interaction_rules: string[];
  master_prompt_override: string;
}

export interface TtsVoiceSettings {
  enabled: boolean;
  voiceName?: string | null;
  volumePercent?: number | null;
}

export interface VoiceInputFrame {
  sessionId: string;
  mode: 'owner' | 'public';
  engine: 'assemblyai-realtime' | 'local-fallback' | 'none';
  transcript: string;
  normalizedTranscript: string;
  commandHint?: string | null;
  nameHint?: string | null;
  timeContextIso: string;
  finalLatencyMs?: number | null;
}

export interface VoiceRuntimeCheck {
  name: string;
  status: 'pass' | 'warn' | 'fail' | string;
  details: string;
}

export interface VoiceRuntimeReport {
  generatedAt: string;
  overall: 'pass' | 'warn' | 'fail' | string;
  sttReady: boolean;
  ttsReady: boolean;
  checks: VoiceRuntimeCheck[];
}

export interface MicDebugView {
  backend: string;
  wavPath: string;
  transcript: string;
  durationMs: number;
}

export interface SttAutoConfigResult {
  applied: boolean;
  message: string;
  sttEnabled: boolean;
  sttBinaryPath?: string | null;
  sttModelPath?: string | null;
}

export interface SttSetupProgress {
  stage: string;
  progress: number;
  message: string;
}

export interface HeadlessStatusView {
  configPath: string;
  model: string;
  voiceEnabled: boolean;
  sttBackend: string;
  ttsBackend: string;
  memoryLog: string;
}

export interface BackendModuleView {
  name: string;
  light: ModuleLight;
  message: string;
  restarts: number;
  lastStartedAt?: string | null;
  lastFinishedAt?: string | null;
  lastDurationMs?: number | null;
}

export interface BackendControlSnapshot {
  connected: boolean;
  addr: string;
  status?: HeadlessStatusView | null;
  modules: BackendModuleView[];
  error?: string | null;
}

export interface BackendConsoleResult {
  ok: boolean;
  output?: string | null;
  error?: string | null;
  snapshot: BackendControlSnapshot;
}

export interface VoiceSessionState {
  sessionId: string;
  engine: 'assemblyai-realtime' | 'local-fallback' | 'none';
  status: 'idle' | 'starting' | 'listening' | 'processing' | 'replying' | 'error';
  interimText: string;
  lastFinalText: string;
  firstInterimLatencyMs: number | null;
  finalLatencyMs: number | null;
  aiLatencyMs: number | null;
  ttsLatencyMs: number | null;
  restartCount: number;
  droppedCount: number;
  lastError: string | null;
  speakingBlocked: boolean;
  micEnabled: boolean;
}

export interface AssemblyAiStreamingToken {
  token: string;
  expiresInSeconds: number;
}

export interface LiveSttEvent {
  kind: 'status' | 'interim' | 'final' | 'error' | string;
  text?: string | null;
  detail?: string | null;
  backend?: string | null;
}
