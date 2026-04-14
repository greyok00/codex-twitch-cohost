export class WorkerBackedTranscriptService {
  private worker: Worker | null = null;
  private startedAt = 0;

  constructor() {
    if (typeof Worker !== 'undefined') {
      this.worker = new Worker(new URL('../workers/voiceRuntimeWorker.ts', import.meta.url), { type: 'module' });
    }
  }

  setStartedAt(value: number) {
    this.startedAt = value;
  }

  async reset(): Promise<void> {
    const worker = this.worker;
    if (!worker) return;
    await new Promise<void>((resolve) => {
      const handler = (event: MessageEvent) => {
        if (event.data?.type === 'reset_result') {
          worker.removeEventListener('message', handler);
          resolve();
        }
      };
      worker.addEventListener('message', handler);
      worker.postMessage({ type: 'reset' });
    });
  }

  async pushInterim(text: string): Promise<{ interim: string; firstInterimLatencyMs: number | null }> {
    const worker = this.worker;
    if (!worker) {
      return { interim: text.trim(), firstInterimLatencyMs: null };
    }
    return new Promise((resolve) => {
      const handler = (event: MessageEvent) => {
        if (event.data?.type === 'interim_result') {
          worker.removeEventListener('message', handler);
          resolve({
            interim: String(event.data?.payload?.interim || ''),
            firstInterimLatencyMs: event.data?.payload?.firstInterimLatencyMs ?? null
          });
        }
      };
      worker.addEventListener('message', handler);
      worker.postMessage({ type: 'interim', text, startedAt: this.startedAt });
    });
  }

  async pushFinal(text: string): Promise<{ committed: string | null; finalLatencyMs: number | null }> {
    const worker = this.worker;
    if (!worker) {
      return { committed: text.trim() || null, finalLatencyMs: null };
    }
    return new Promise((resolve) => {
      const handler = (event: MessageEvent) => {
        if (event.data?.type === 'final_result') {
          worker.removeEventListener('message', handler);
          resolve({
            committed: event.data?.payload?.committed ?? null,
            finalLatencyMs: event.data?.payload?.finalLatencyMs ?? null
          });
        }
      };
      worker.addEventListener('message', handler);
      worker.postMessage({ type: 'final', text, startedAt: this.startedAt });
    });
  }

  dispose() {
    this.worker?.terminate();
    this.worker = null;
  }
}
