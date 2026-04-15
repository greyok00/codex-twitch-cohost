import type { VoiceSessionDiagnostics } from '../types';

export type VoiceSessionMode = 'owner' | 'public';
export type VoiceEngineKind = 'local-fallback' | 'none';
export type VoiceSessionStatus = 'idle' | 'starting' | 'listening' | 'processing' | 'replying' | 'error';

export interface VoiceSessionState extends VoiceSessionDiagnostics {
  speakingBlocked: boolean;
  micEnabled: boolean;
}

export interface VoiceSessionCallbacks {
  onInterim: (text: string) => void;
  onFinal: (text: string) => Promise<void>;
  onStatus: (status: VoiceSessionStatus, detail?: string) => void;
  onError: (message: string) => void;
  onSpeechStart?: () => void;
  onSpeechEnd?: () => void;
}

export interface SpeechEngine {
  kind: VoiceEngineKind;
  start(): Promise<void>;
  stop(): Promise<void>;
  dispose(): Promise<void>;
}

export interface VoiceSessionStartOptions {
  mode: VoiceSessionMode;
  callerName?: string;
}
