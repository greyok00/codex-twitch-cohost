import { describe, expect, it } from 'vitest';
import { chooseBestUtterance, mergeTranscriptText, recordUtteranceCandidate, stripWakeWords } from '../../voice/utterance';

describe('utterance helpers', () => {
  it('merges overlapping transcript fragments', () => {
    expect(mergeTranscriptText('tell me what', 'what just happened')).toBe('tell me what just happened');
  });

  it('strips wake words without removing the rest of the request', () => {
    expect(stripWakeWords('hey chatbot tell me the weather')).toBe('tell me the weather');
  });

  it('prefers the most stable longer candidate when finalizing', () => {
    let candidates = recordUtteranceCandidate([], 'tell me');
    candidates = recordUtteranceCandidate(candidates, 'tell me what just happened');
    candidates = recordUtteranceCandidate(candidates, 'tell me what just happened');
    expect(chooseBestUtterance('tell me what just', candidates)).toBe('tell me what just happened');
  });
});
