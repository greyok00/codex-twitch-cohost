import { writable } from 'svelte/store';
import type { VoiceSessionState } from './types';

function makeSessionId(): string {
  return `vs_${Math.random().toString(36).slice(2, 10)}`;
}

export const voiceSessionStore = writable<VoiceSessionState>({
  sessionId: makeSessionId(),
  mode: 'owner',
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

export function resetVoiceSessionState(mode: 'owner' | 'public' = 'owner') {
  voiceSessionStore.set({
    sessionId: makeSessionId(),
    mode,
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
}
