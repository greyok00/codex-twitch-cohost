export type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';

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

export interface DiagnosticsState {
  lastError?: string;
  twitchState: ConnectionState;
  providerState: ConnectionState;
  uptimeSeconds: number;
}

export interface ServiceHealthItem {
  id: string;
  label: string;
  configured: boolean;
  available: boolean;
  authenticated: boolean;
  active: boolean;
  status: 'pass' | 'warn' | 'fail' | string;
  details: string[];
}

export interface ServiceHealthReport {
  generatedAt: string;
  overall: 'pass' | 'warn' | 'fail' | string;
  services: ServiceHealthItem[];
}

export interface DebugBundleResult {
  generatedAt: string;
  path: string;
  sections: string[];
}

export interface AppStatus {
  channel?: string;
  model: string;
  voiceEnabled: boolean;
  lurkMode: boolean;
  twitchState: ConnectionState;
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

export interface AuthSessions {
  botUsername: string;
  botTokenPresent: boolean;
  channel: string;
  broadcasterLogin?: string | null;
  streamerTokenPresent: boolean;
}

export interface SelfTestCheck {
  name: string;
  status: 'pass' | 'warn' | 'fail' | string;
  details: string;
}

export interface SelfTestReport {
  generatedAt: string;
  overall: 'pass' | 'warn' | 'fail' | string;
  checks: SelfTestCheck[];
}

export interface SttConfig {
  sttEnabled: boolean;
  sttBinaryPath?: string | null;
  sttModelPath?: string | null;
}

export interface SttAutoConfigResult {
  applied: boolean;
  message: string;
  sttEnabled: boolean;
  sttBinaryPath?: string | null;
  sttModelPath?: string | null;
}

export interface TtsVoiceSettings {
  enabled: boolean;
  voiceName?: string | null;
  volumePercent?: number | null;
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

export interface AvatarImage {
  dataUrl: string;
  fileName?: string | null;
}

export interface BehaviorSettings {
  cohostMode: boolean;
  scheduledMessagesMinutes?: number | null;
}
