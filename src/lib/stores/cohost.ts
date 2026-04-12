import { writable } from 'svelte/store';

export type CohostModelMode = 'fast' | 'medium' | 'long_context';

export interface CohostControls {
  modelMode: CohostModelMode;
  autonomousReplies: boolean;
  videoRemarksPerMinute: number;
}

const STORAGE_KEY = 'greyok-cohost-controls';

const defaults: CohostControls = {
  modelMode: 'medium',
  autonomousReplies: true,
  videoRemarksPerMinute: 1.2
};

function loadInitial(): CohostControls {
  if (typeof window === 'undefined') return defaults;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw) as Partial<CohostControls>;
    return {
      modelMode: parsed.modelMode === 'fast' || parsed.modelMode === 'long_context' ? parsed.modelMode : 'medium',
      autonomousReplies: parsed.autonomousReplies ?? defaults.autonomousReplies,
      videoRemarksPerMinute: Math.max(0, Math.min(4, Number(parsed.videoRemarksPerMinute ?? defaults.videoRemarksPerMinute)))
    };
  } catch {
    return defaults;
  }
}

function createCohostControlsStore() {
  const store = writable<CohostControls>(loadInitial());
  if (typeof window !== 'undefined') {
    store.subscribe((value) => {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
    });
  }
  return store;
}

export const cohostControlsStore = createCohostControlsStore();
