export interface RuntimeCapabilityReport {
  workerSupported: boolean;
  offscreenCanvasSupported: boolean;
  webGpuSupportedMain: boolean;
  webGpuSupportedWorker: boolean;
  webGpuInitializedMain: boolean;
  webGpuDeviceReadyMain: boolean;
  webGpuAdapterName?: string | null;
  hardwareConcurrency: number;
  deviceMemoryGb?: number | null;
  secureContext: boolean;
}
