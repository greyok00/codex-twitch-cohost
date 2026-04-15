import { describe, expect, it } from 'vitest';
import { TranscriptNormalizer } from '../../voice-session/TranscriptNormalizer';

describe('TranscriptNormalizer', () => {
  it('prefers a stronger interim phrase over a weaker final fragment', () => {
    const normalizer = new TranscriptNormalizer();
    normalizer.pushInterim('hey can you understand');
    normalizer.pushInterim('hey can you understand me');
    expect(normalizer.pushFinal('sand me')).toBe('hey can you understand me');
  });

  it('drops duplicate finalized utterances after reset cycle', () => {
    const normalizer = new TranscriptNormalizer();
    expect(normalizer.pushFinal('can you hear me')).toBe('can you hear me');
    expect(normalizer.pushFinal('can you hear me')).toBeNull();
  });
});
