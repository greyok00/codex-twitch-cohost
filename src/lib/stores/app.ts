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
  name: 'Basic Assistant',
  voice: 'clear',
  tone: 'grounded, helpful, conversational',
  humor_level: 3,
  aggression_level: 0,
  friendliness: 8,
  verbosity: 4,
  streamer_relationship: 'reliable cohost',
  response_style: 'plainspoken, direct, context-aware',
  lore: 'A straightforward stream assistant focused on clarity and useful conversation.',
  taboo_topics: ['hate speech', 'private data'],
  catchphrases: [],
  reply_rules: ['Answer the latest question first', 'Use normal everyday language', 'Keep responses safe and concise'],
  chat_behavior_rules: ['Stay grounded in the latest context'],
  viewer_interaction_rules: ['Be polite and easy to understand'],
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
