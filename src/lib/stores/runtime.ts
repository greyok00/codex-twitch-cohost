import { writable } from 'svelte/store';
import type { RuntimeCapabilityReport } from '../types/runtime';

export const runtimeCapabilityStore = writable<RuntimeCapabilityReport>({
  workerSupported: false,
  offscreenCanvasSupported: false,
  webGpuSupportedMain: false,
  webGpuSupportedWorker: false,
  webGpuInitializedMain: false,
  webGpuDeviceReadyMain: false,
  webGpuAdapterName: null,
  hardwareConcurrency: 1,
  deviceMemoryGb: null,
  secureContext: false
});
