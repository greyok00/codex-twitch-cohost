import { describe, expect, it } from 'vitest';
import { CommentScheduler } from '../services/CommentScheduler';
import type { TopicContextWindow, YoutubeCohostSettings } from '../types';

const settings: YoutubeCohostSettings = {
  remarksPerMinute: 1.5,
  relevanceStrictness: 65,
  humorStyle: 'sarcastic',
  maxRemarkLengthSeconds: 8,
  interruptOnlyAtNaturalBreaks: true,
  captionsDebugOverlay: false,
  autoResumeAfterRemark: true,
  developerMode: false
};

function makeContext(overrides: Partial<TopicContextWindow> = {}): TopicContextWindow {
  return {
    currentTime: 44,
    currentSegment: { startTime: 42, endTime: 49, text: 'Speaker lands a weird claim and pauses.', confidence: 0.8 },
    previousSegments: [],
    nextSegments: [],
    topicSummary: 'Current topic around: speaker, claim, pause',
    entities: ['Speaker'],
    tone: 'playful',
    pauseConfidence: 0.72,
    seriousnessScore: 0.1,
    humorOpportunityScore: 0.75,
    transcriptCoverageScore: 0.9,
    ...overrides
  };
}

describe('CommentScheduler', () => {
  it('is deterministic with a fixed seed', () => {
    const runtime = {
      remarksSpokenThisMinute: 0,
      secondsSinceLastRemark: 99,
      repetitionMemory: [],
      skippedOpportunities: 0,
      nowSecond: 90
    };
    const a = new CommentScheduler(1337);
    const b = new CommentScheduler(1337);
    const outA = a.shouldInterrupt(makeContext(), settings, runtime, 48);
    const outB = b.shouldInterrupt(makeContext(), settings, runtime, 48);
    expect(outA.shouldInterrupt).toBe(outB.shouldInterrupt);
    expect(outA.reason).toBe(outB.reason);
    expect(outA.components.total).toBeCloseTo(outB.components.total, 6);
  });

  it('skips for sensitive segments', () => {
    const scheduler = new CommentScheduler(11);
    const runtime = {
      remarksSpokenThisMinute: 0,
      secondsSinceLastRemark: 99,
      repetitionMemory: [],
      skippedOpportunities: 0,
      nowSecond: 90
    };
    const decision = scheduler.shouldInterrupt(
      makeContext({ seriousnessScore: 0.93, humorOpportunityScore: 0.8 }),
      settings,
      runtime,
      60
    );
    expect(decision.shouldInterrupt).toBe(false);
    expect(decision.reason).toContain('sensitive');
  });
});
