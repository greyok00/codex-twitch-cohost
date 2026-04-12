import type { CommentDecision, TopicContextWindow, YoutubeCohostSettings } from '../types';
import { clamp01, hashText, seededRandom } from '../utils';

export interface SchedulerRuntimeState {
  remarksSpokenThisMinute: number;
  secondsSinceLastRemark: number;
  repetitionMemory: string[];
  skippedOpportunities: number;
  nowSecond: number;
  callbackSuppressedUntilSeconds?: number;
  topicHistory?: string[];
}

export class CommentScheduler {
  private readonly random: () => number;

  constructor(seed = Date.now()) {
    this.random = seededRandom(seed);
  }

  shouldInterrupt(
    context: TopicContextWindow,
    settings: YoutubeCohostSettings,
    runtime: SchedulerRuntimeState,
    currentTime: number
  ): CommentDecision {
    if ((runtime.callbackSuppressedUntilSeconds || 0) > currentTime) {
      return this.skip('callback suppression window', 8, 0.45);
    }
    const minGap = this.estimateMinGap(settings.remarksPerMinute, context.transcriptCoverageScore);
    if (currentTime < 15) {
      return this.skip('warmup window', minGap, 0.45);
    }
    if (runtime.secondsSinceLastRemark < minGap) {
      return this.skip(`min-gap ${minGap.toFixed(1)}s`, minGap, 0.45);
    }

    const noveltyScore = this.computeNovelty(context, runtime.repetitionMemory, runtime.topicHistory || []);
    const userSliderPressure = clamp01(settings.remarksPerMinute / 4);
    const repetitionPenalty = this.computeRepetitionPenalty(runtime.repetitionMemory, context.topicSummary);
    const seriousnessPenalty = clamp01(context.seriousnessScore * 0.8);

    const score =
      context.pauseConfidence * 0.25 +
      context.humorOpportunityScore * 0.25 +
      noveltyScore * 0.2 +
      context.transcriptCoverageScore * 0.15 +
      userSliderPressure * 0.15 -
      repetitionPenalty -
      seriousnessPenalty;

    const strictnessBoost = clamp01(settings.relevanceStrictness / 100) * 0.1;
    const transcriptConfidenceBoost = context.currentSegment && context.transcriptCoverageScore >= 0.55 ? 0.06 : 0;
    const threshold = 0.42 + strictnessBoost - transcriptConfidenceBoost;

    const naturalBreakGate = settings.interruptOnlyAtNaturalBreaks
      ? context.pauseConfidence >= (context.currentSegment ? 0.34 : 0.45)
      : true;
    const sensitiveGate = context.seriousnessScore < 0.72;
    const stochasticGate = this.random() <= clamp01(score + 0.18 + context.transcriptCoverageScore * 0.08);

    const shouldInterrupt = naturalBreakGate && sensitiveGate && score >= threshold && stochasticGate;

    const reason = shouldInterrupt
      ? 'fired: score above threshold with adequate context'
      : !sensitiveGate
        ? 'skipped: sensitive segment'
        : !naturalBreakGate
          ? 'skipped: no natural break'
          : score < threshold
            ? `skipped: score ${score.toFixed(2)} below ${threshold.toFixed(2)}`
            : 'skipped: probability gate';

    return {
      shouldInterrupt,
      reason,
      minGapSeconds: minGap,
      threshold,
      components: {
        pauseConfidence: context.pauseConfidence,
        humorOpportunity: context.humorOpportunityScore,
        noveltyScore,
        transcriptCoverage: context.transcriptCoverageScore,
        userSliderPressure,
        repetitionPenalty,
        seriousnessPenalty,
        total: score
      }
    };
  }

  private skip(reason: string, minGapSeconds: number, threshold: number): CommentDecision {
    return {
      shouldInterrupt: false,
      reason,
      minGapSeconds,
      threshold,
      components: {
        pauseConfidence: 0,
        humorOpportunity: 0,
        noveltyScore: 0,
        transcriptCoverage: 0,
        userSliderPressure: 0,
        repetitionPenalty: 0,
        seriousnessPenalty: 0,
        total: 0
      }
    };
  }

  private estimateMinGap(remarksPerMinute: number, coverage: number): number {
    const rpm = Math.max(0, Math.min(4, remarksPerMinute));
    if (rpm <= 0.01) return 9999;
    const targetSeconds = 60 / rpm;
    const coveragePenalty = 1 + (1 - clamp01(coverage)) * 0.65;
    return Math.max(8, targetSeconds * coveragePenalty);
  }

  private computeNovelty(context: TopicContextWindow, memory: string[], topicHistory: string[]): number {
    const source = `${context.topicSummary}|${context.currentSegment?.text || ''}`;
    const sig = hashText(source);
    if (memory.some((m) => hashText(m).slice(0, 6) === sig.slice(0, 6))) {
      return 0.2;
    }
    const normalizedTopic = context.topicSummary.toLowerCase();
    if (topicHistory.some((topic) => topic.toLowerCase() === normalizedTopic)) {
      return 0.32;
    }
    return clamp01(0.65 + this.random() * 0.35);
  }

  private computeRepetitionPenalty(memory: string[], topic: string): number {
    if (memory.length === 0) return 0;
    const normTopic = topic.toLowerCase();
    const hits = memory
      .slice(0, 8)
      .filter((line) => line.toLowerCase().includes(normTopic.slice(0, 28)))
      .length;
    return clamp01(hits / 5) * 0.35;
  }
}
