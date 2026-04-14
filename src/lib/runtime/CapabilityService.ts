import { runtimeCapabilityStore } from '../stores/runtime';
import type { RuntimeCapabilityReport } from '../types/runtime';

let capabilityProbePromise: Promise<RuntimeCapabilityReport> | null = null;

export async function detectRuntimeCapabilities(): Promise<RuntimeCapabilityReport> {
  if (capabilityProbePromise) {
    return capabilityProbePromise;
  }

  capabilityProbePromise = detectRuntimeCapabilitiesInternal();
  return capabilityProbePromise;
}

async function detectRuntimeCapabilitiesInternal(): Promise<RuntimeCapabilityReport> {
  const nav = typeof navigator !== 'undefined'
    ? (navigator as Navigator & {
        gpu?: {
          requestAdapter?: () => Promise<{
            requestDevice?: () => Promise<{ destroy?: () => void }>;
            info?: { description?: string };
          } | null>;
        };
        deviceMemory?: number;
      })
    : null;
  const mainGpu = typeof navigator !== 'undefined' && 'gpu' in navigator && !!(navigator as Navigator & { gpu?: unknown }).gpu;
  const workerSupported = typeof Worker !== 'undefined';
  const offscreenCanvasSupported = typeof OffscreenCanvas !== 'undefined';
  const hardwareConcurrency = typeof navigator !== 'undefined' ? navigator.hardwareConcurrency || 1 : 1;
  const deviceMemoryGb = typeof navigator !== 'undefined' && 'deviceMemory' in navigator
    ? Number((navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? 0) || null
    : null;
  const secureContext = typeof window !== 'undefined' ? window.isSecureContext : false;

  let webGpuSupportedWorker = false;
  if (workerSupported) {
    try {
      const worker = new Worker(new URL('../workers/voiceRuntimeWorker.ts', import.meta.url), { type: 'module' });
      webGpuSupportedWorker = await new Promise<boolean>((resolve) => {
        const timeout = window.setTimeout(() => {
          worker.terminate();
          resolve(false);
        }, 1200);
        worker.onmessage = (event) => {
          if (event.data?.type === 'probe_result') {
            window.clearTimeout(timeout);
            worker.terminate();
            resolve(!!event.data?.payload?.webGpuSupportedWorker);
          }
        };
        worker.postMessage({ type: 'probe' });
      });
    } catch {
      webGpuSupportedWorker = false;
    }
  }

  let webGpuInitializedMain = false;
  let webGpuDeviceReadyMain = false;
  let webGpuAdapterName: string | null = null;
  if (mainGpu && nav?.gpu?.requestAdapter) {
    try {
      const adapter = await nav.gpu.requestAdapter();
      if (adapter) {
        webGpuInitializedMain = true;
        webGpuAdapterName = typeof adapter.info?.description === 'string'
          ? adapter.info.description
          : null;
        try {
          const device = await adapter.requestDevice?.();
          webGpuDeviceReadyMain = !!device;
          device?.destroy?.();
        } catch {
          webGpuDeviceReadyMain = false;
        }
      }
    } catch {
      webGpuInitializedMain = false;
      webGpuDeviceReadyMain = false;
      webGpuAdapterName = null;
    }
  }

  const report: RuntimeCapabilityReport = {
    workerSupported,
    offscreenCanvasSupported,
    webGpuSupportedMain: mainGpu,
    webGpuSupportedWorker,
    webGpuInitializedMain,
    webGpuDeviceReadyMain,
    webGpuAdapterName,
    hardwareConcurrency,
    deviceMemoryGb,
    secureContext
  };
  runtimeCapabilityStore.set(report);
  return report;
}
