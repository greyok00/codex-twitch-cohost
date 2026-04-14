import { TranscriptNormalizer } from '../voice-session/TranscriptNormalizer';

type WorkerNavigatorLike = Navigator & {
  gpu?: unknown;
  deviceMemory?: number;
};

type ProbeMessage = { type: 'probe' };
type ResetMessage = { type: 'reset' };
type InterimMessage = { type: 'interim'; text: string; startedAt: number };
type FinalMessage = { type: 'final'; text: string; startedAt: number };

type Incoming = ProbeMessage | ResetMessage | InterimMessage | FinalMessage;

const normalizer = new TranscriptNormalizer();
let firstInterimMarked = false;

function workerGpuSupported(): boolean {
  const nav = self.navigator as WorkerNavigatorLike;
  return typeof nav !== 'undefined' && 'gpu' in nav && !!nav.gpu;
}

self.onmessage = (event: MessageEvent<Incoming>) => {
  const msg = event.data;
  if (msg.type === 'probe') {
    const nav = self.navigator as WorkerNavigatorLike;
    self.postMessage({
      type: 'probe_result',
      payload: {
        webGpuSupportedWorker: workerGpuSupported(),
        hardwareConcurrency: nav.hardwareConcurrency || 1
      }
    });
    return;
  }

  if (msg.type === 'reset') {
    normalizer.reset();
    firstInterimMarked = false;
    self.postMessage({ type: 'reset_result' });
    return;
  }

  if (msg.type === 'interim') {
    const interim = normalizer.pushInterim(msg.text);
    const firstInterimLatencyMs = firstInterimMarked ? null : Math.max(0, Date.now() - msg.startedAt);
    firstInterimMarked = true;
    self.postMessage({
      type: 'interim_result',
      payload: {
        interim,
        firstInterimLatencyMs
      }
    });
    return;
  }

  if (msg.type === 'final') {
    const committed = normalizer.pushFinal(msg.text);
    self.postMessage({
      type: 'final_result',
      payload: {
        committed,
        finalLatencyMs: Math.max(0, Date.now() - msg.startedAt)
      }
    });
  }
};
