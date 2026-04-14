import { writable } from 'svelte/store';

export type CohostModelMode = 'fast' | 'medium' | 'long_context';

export interface CohostControls {
  modelMode: CohostModelMode;
  autonomousReplies: boolean;
  videoRemarksPerMinute: number;
}

export const cohostControlsStore = writable<CohostControls>({
  modelMode: 'medium',
  autonomousReplies: false,
  videoRemarksPerMinute: 0.6
});
