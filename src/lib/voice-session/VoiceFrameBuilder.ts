import type { VoiceInputFrame } from '../types';

let sharedWorker: Worker | null = null;
let requestId = 0;

function getWorker(): Worker | null {
  if (typeof Worker === 'undefined') return null;
  if (!sharedWorker) {
    sharedWorker = new Worker(new URL('../workers/voiceFrameWorker.ts', import.meta.url), { type: 'module' });
  }
  return sharedWorker;
}

export async function buildVoiceInputFrame(input: {
  sessionId: string;
  mode: 'owner' | 'public';
  engine: 'assemblyai-realtime' | 'local-fallback' | 'none';
  transcript: string;
  finalLatencyMs?: number | null;
}): Promise<VoiceInputFrame> {
  if (typeof Worker === 'undefined') {
    return {
      sessionId: input.sessionId,
      mode: input.mode,
      engine: input.engine,
      transcript: input.transcript.trim(),
      normalizedTranscript: input.transcript.trim().toLowerCase(),
      commandHint: null,
      nameHint: null,
      timeContextIso: new Date().toISOString(),
      finalLatencyMs: input.finalLatencyMs ?? null
    };
  }

  const worker = getWorker();
  if (!worker) {
    throw new Error('Voice frame worker unavailable.');
  }
  const currentId = ++requestId;
  return new Promise<VoiceInputFrame>((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      worker.removeEventListener('message', handleMessage);
      worker.removeEventListener('error', handleError);
      reject(new Error('Voice frame build timed out.'));
    }, 1200);

    const handleMessage = (event: MessageEvent) => {
      if (event.data?.type !== 'built' || event.data?.requestId !== currentId) return;
      window.clearTimeout(timeout);
      worker.removeEventListener('message', handleMessage);
      worker.removeEventListener('error', handleError);
      const payload = event.data.payload as VoiceInputFrame;
      resolve(payload);
    };

    const handleError = (event: ErrorEvent) => {
      window.clearTimeout(timeout);
      worker.removeEventListener('message', handleMessage);
      worker.removeEventListener('error', handleError);
      reject(new Error(event.message || 'Voice frame worker failed.'));
    };

    worker.addEventListener('message', handleMessage);
    worker.addEventListener('error', handleError);
    worker.postMessage({ type: 'build', payload: input, requestId: currentId });
  });
}
