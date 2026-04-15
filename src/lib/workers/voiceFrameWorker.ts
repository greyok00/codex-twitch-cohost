import { normalizeSpeechText } from '../voice/utterance';

type BuildMessage = {
  type: 'build';
  requestId: number;
  payload: {
    sessionId: string;
    mode: 'owner' | 'public';
    engine: 'assemblyai-realtime' | 'local-fallback' | 'none';
    transcript: string;
    finalLatencyMs?: number | null;
  };
};

function extractCommandHint(text: string): string | null {
  const lowered = text.toLowerCase();
  if (lowered.startsWith('_') || lowered.startsWith('!') || lowered.startsWith('/')) {
    return text.trim().split(/\s+/)[0] || null;
  }
  if (lowered.includes('search for ') || lowered.includes('web search ')) return 'search';
  if (lowered.includes('what time') || lowered.includes('current time')) return 'time';
  return null;
}

function extractNameHint(text: string): string | null {
  const lowered = text.toLowerCase();
  const patterns = ['my name is ', 'call me ', 'you can call me ', 'refer to me as '];
  for (const pattern of patterns) {
    const idx = lowered.indexOf(pattern);
    if (idx >= 0) {
      return text
        .slice(idx + pattern.length)
        .trim()
        .split(/[.!?,\n]/)[0]
        .trim()
        .split(/\s+/)
        .slice(0, 6)
        .join(' ');
    }
  }
  return null;
}

self.onmessage = (event: MessageEvent<BuildMessage>) => {
  if (event.data?.type !== 'build') return;
  const payload = event.data.payload;
  const transcript = payload.transcript.trim();
  const frame = {
    sessionId: payload.sessionId,
    mode: payload.mode,
    engine: payload.engine,
    transcript,
    normalizedTranscript: normalizeSpeechText(transcript),
    commandHint: extractCommandHint(transcript),
    nameHint: extractNameHint(transcript),
    timeContextIso: new Date().toISOString(),
    finalLatencyMs: payload.finalLatencyMs ?? null
  };
  self.postMessage({ type: 'built', requestId: event.data.requestId, payload: frame });
};
