import type { TranscriptSegment } from '../types';
import type { TranscriptProvider, TranscriptProviderResult } from './YouTubeTimedTextProvider';

export class MockTranscriptProvider implements TranscriptProvider {
  async load(videoId: string): Promise<TranscriptProviderResult> {
    const seed = videoId.slice(0, 5).toLowerCase();
    const segments: TranscriptSegment[] = [
      { startTime: 2, endTime: 11, text: `Host intro for ${seed} and a quick setup explanation.`, confidence: 0.86 },
      { startTime: 12, endTime: 24, text: 'The speaker compares two approaches and hints one is risky but funny.', confidence: 0.81 },
      { startTime: 25, endTime: 39, text: 'A bold claim lands and chat energy spikes with surprise.', confidence: 0.79 },
      { startTime: 40, endTime: 57, text: 'They backtrack and clarify details with awkward pacing.', confidence: 0.82 },
      { startTime: 58, endTime: 74, text: 'A practical tip appears and the tone gets upbeat.', confidence: 0.88 }
    ];
    return {
      providerName: 'mock-transcript',
      segments,
      message: `Loaded ${segments.length} deterministic mock segments for ${seed}.`
    };
  }
}
