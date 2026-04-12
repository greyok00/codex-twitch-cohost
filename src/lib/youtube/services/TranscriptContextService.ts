import type { TopicContextWindow, TranscriptSegment } from '../types';
import { clamp01, normalizeText } from '../utils';

const SERIOUS_KEYWORDS = [
  'death',
  'suicide',
  'assault',
  'violence',
  'abuse',
  'war',
  'trauma',
  'grief',
  'self harm'
];

const HUMOR_SIGNALS = ['awkward', 'oops', 'wait what', 'no way', 'bruh', 'lol', 'haha', 'wild', 'chaos'];

export class TranscriptContextService {
  private segments: TranscriptSegment[] = [];
  private durationSeconds = 0;

  setSegments(segments: TranscriptSegment[], durationSeconds: number): void {
    this.segments = [...segments]
      .filter((s) => s.endTime > s.startTime && normalizeText(s.text).length > 0)
      .sort((a, b) => a.startTime - b.startTime)
      .map((s) => ({ ...s, text: normalizeText(s.text) }));
    this.durationSeconds = Math.max(0, durationSeconds || 0);
  }

  getWindow(currentTime: number, backSeconds: number, forwardSeconds: number): TopicContextWindow {
    const from = Math.max(0, currentTime - backSeconds);
    const to = currentTime + forwardSeconds;

    const previousSegments = this.segments.filter((s) => s.endTime >= from && s.endTime <= currentTime);
    const nextSegments = this.segments.filter((s) => s.startTime >= currentTime && s.startTime <= to);
    const currentSegment = this.segments.find((s) => s.startTime <= currentTime && s.endTime >= currentTime) || null;

    const bucket = [...previousSegments.slice(-5), ...(currentSegment ? [currentSegment] : []), ...nextSegments.slice(0, 3)];

    const topicSummary = this.summarizeCurrentTopic(bucket);
    const entities = this.extractEntities(bucket);
    const pauseConfidence = this.detectNaturalPause(bucket, currentSegment, currentTime);
    const seriousnessScore = this.estimateSeriousness(bucket);
    const humorOpportunityScore = this.estimateHumorOpportunity(bucket);
    const transcriptCoverageScore = this.estimateCoverage(currentTime, from, to, bucket);
    const tone = this.estimateTone(bucket, seriousnessScore, humorOpportunityScore);

    return {
      currentTime,
      currentSegment,
      previousSegments,
      nextSegments,
      topicSummary,
      entities,
      tone,
      pauseConfidence,
      seriousnessScore,
      humorOpportunityScore,
      transcriptCoverageScore
    };
  }

  summarizeCurrentTopic(window: TranscriptSegment[]): string {
    if (window.length === 0) return 'No transcript context yet';
    const text = window.map((s) => s.text).join(' ');
    const words = text
      .toLowerCase()
      .replace(/[^a-z0-9\s]/g, ' ')
      .split(/\s+/)
      .filter((w) => w.length > 3);

    const freq = new Map<string, number>();
    for (const w of words) freq.set(w, (freq.get(w) || 0) + 1);
    const top = [...freq.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 6)
      .map(([w]) => w);

    return top.length > 0 ? `Current topic around: ${top.join(', ')}` : text.slice(0, 180);
  }

  extractEntities(window: TranscriptSegment[]): string[] {
    const text = window.map((s) => s.text).join(' ');
    const entities = text.match(/\b([A-Z][a-zA-Z]{2,}|[A-Z]{2,}|\d{4})\b/g) || [];
    return [...new Set(entities)].slice(0, 12);
  }

  detectNaturalPause(window: TranscriptSegment[], currentSegment: TranscriptSegment | null, currentTime: number): number {
    if (window.length === 0) return 0.12;
    const secondsToEnd = currentSegment ? currentSegment.endTime - currentTime : Number.POSITIVE_INFINITY;
    const punctuationBoost = (currentSegment?.text.match(/[.!?]+/g)?.length || 0) * 0.12;
    const shortLineBoost = currentSegment && currentSegment.text.length < 65 ? 0.18 : 0;
    const pauseGap = this.estimateGap(window);
    const nearEndBoost = secondsToEnd <= 1.8 ? 0.16 : secondsToEnd <= 3 ? 0.08 : 0;
    return clamp01(0.16 + punctuationBoost + shortLineBoost + pauseGap + nearEndBoost);
  }

  estimateSeriousness(window: TranscriptSegment[]): number {
    const text = window.map((s) => s.text).join(' ').toLowerCase();
    let hits = 0;
    for (const kw of SERIOUS_KEYWORDS) {
      if (text.includes(kw)) hits += 1;
    }
    return clamp01(hits / 3);
  }

  estimateHumorOpportunity(window: TranscriptSegment[]): number {
    if (window.length === 0) return 0.08;
    const text = window.map((s) => s.text).join(' ').toLowerCase();
    let score = 0.2;
    for (const sig of HUMOR_SIGNALS) {
      if (text.includes(sig)) score += 0.14;
    }
    if (/\b(why|how|what|wait)\b/.test(text)) score += 0.1;
    return clamp01(score);
  }

  private estimateCoverage(currentTime: number, from: number, to: number, bucket: TranscriptSegment[]): number {
    if (this.durationSeconds <= 0) return bucket.length > 0 ? 0.5 : 0;
    const windowSpan = Math.max(1, to - from);
    const covered = bucket.reduce((acc, s) => acc + Math.max(0, Math.min(to, s.endTime) - Math.max(from, s.startTime)), 0);
    const localCoverage = clamp01(covered / windowSpan);
    const globalHint = clamp01(this.segments.length / Math.max(8, this.durationSeconds / 14));
    const atLeastCurrent = this.segments.some((s) => s.startTime <= currentTime && s.endTime >= currentTime) ? 0.2 : 0;
    return clamp01(localCoverage * 0.65 + globalHint * 0.25 + atLeastCurrent);
  }

  private estimateGap(window: TranscriptSegment[]): number {
    if (window.length < 2) return 0;
    const sorted = [...window].sort((a, b) => a.startTime - b.startTime);
    let maxGap = 0;
    for (let i = 1; i < sorted.length; i += 1) {
      maxGap = Math.max(maxGap, sorted[i].startTime - sorted[i - 1].endTime);
    }
    return clamp01(maxGap / 2.4) * 0.25;
  }

  private estimateTone(window: TranscriptSegment[], seriousness: number, humor: number): TopicContextWindow['tone'] {
    if (window.length === 0) return 'neutral';
    if (seriousness > 0.62) return 'serious';
    const text = window.map((s) => s.text).join(' ').toLowerCase();
    if (/\b(angry|fight|mad|rant|hate)\b/.test(text)) return 'tense';
    if (/\b(excited|amazing|incredible|lets go|wow)\b/.test(text)) return 'excited';
    if (humor > 0.6) return 'playful';
    return 'neutral';
  }
}
