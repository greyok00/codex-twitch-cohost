import { writable } from 'svelte/store';
import type { AppStatus, AuthSessions, ChatMessage, DebugBundleResult, DiagnosticsState, EventMessage, PersonalityProfile, SelfTestReport, ServiceHealthReport } from '../types';

export const statusStore = writable<AppStatus>({
  model: 'llama3.1:8b-instruct',
  voiceEnabled: false,
  lurkMode: false,
  twitchState: 'disconnected'
});

export const chatStore = writable<ChatMessage[]>([]);
export const botLogStore = writable<ChatMessage[]>([]);
export const eventStore = writable<EventMessage[]>([]);

export const diagnosticsStore = writable<DiagnosticsState>({
  twitchState: 'disconnected',
  providerState: 'disconnected',
  uptimeSeconds: 0
});

export const personalityStore = writable<PersonalityProfile>({
  name: 'Direct Control',
  voice: 'Guy',
  tone: 'balanced, lightly funny, confident, steady',
  humor_level: 4,
  aggression_level: 3,
  friendliness: 6,
  verbosity: 5,
  streamer_relationship: 'direct conversational cohost',
  response_style: 'balanced, lightly funny, confident, steady',
  lore: 'Directly tuned conversational cohost settings.',
  taboo_topics: ['hate speech', 'private data'],
  catchphrases: [],
  reply_rules: ['Stay on the latest topic', 'Do not repeat stock phrases', 'Keep replies conversational and context-aware'],
  chat_behavior_rules: ['Answer the latest point directly before adding a joke or aside.'],
  viewer_interaction_rules: ['Address viewers like real people', 'Use recent context before improvising'],
  master_prompt_override: ''
});

export const errorBannerStore = writable<string | null>(null);

export const authSessionsStore = writable<AuthSessions>({
  botUsername: '',
  botTokenPresent: false,
  channel: '',
  broadcasterLogin: null,
  streamerTokenPresent: false
});

export const selfTestReportStore = writable<SelfTestReport | null>(null);
export const serviceHealthStore = writable<ServiceHealthReport | null>(null);
export const debugBundleStore = writable<DebugBundleResult | null>(null);
