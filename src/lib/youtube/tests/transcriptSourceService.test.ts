import { describe, expect, it } from 'vitest';
import { TranscriptSourceService } from '../sources/TranscriptSourceService';
import type { TranscriptProvider, TranscriptProviderResult } from '../sources/YouTubeTimedTextProvider';

class EmptyProvider implements TranscriptProvider {
  async load() {
    return {
      providerName: 'empty-provider',
      segments: [],
      message: 'No transcript here.'
    };
  }
}

class ThrowingProvider implements TranscriptProvider {
  async load(_videoId: string): Promise<TranscriptProviderResult> {
    throw new Error('network unavailable');
  }
}

describe('TranscriptSourceService', () => {
  it('uses uploaded transcript files ahead of provider fallback', async () => {
    const service = new TranscriptSourceService([new EmptyProvider()]);
    const result = await service.resolve({
      videoId: 'abc123',
      durationSeconds: 30,
      transcriptFileText: '00:00:01,000 --> 00:00:04,000\nhello there\n\n00:00:05,000 --> 00:00:09,000\nthis is a transcript'
    });

    expect(result.mode).toBe('user_file');
    expect(result.segments.length).toBeGreaterThan(0);
    expect(result.providerName).toBe('user-upload');
    expect(result.coverageScore).toBeGreaterThan(0);
  });

  it('falls back to conservative metadata mode when providers fail', async () => {
    const service = new TranscriptSourceService([new ThrowingProvider(), new EmptyProvider()]);
    const result = await service.resolve({
      videoId: 'abc123',
      title: 'Deep dive into building a faster app',
      description: 'A host explains bottlenecks and compares approaches.',
      durationSeconds: 180
    });

    expect(result.mode).toBe('metadata');
    expect(result.providerName).toBe('metadata-fallback');
    expect(result.message.toLowerCase()).toContain('fallback');
    expect(result.coverageScore).toBeLessThanOrEqual(0.5);
  });
});
