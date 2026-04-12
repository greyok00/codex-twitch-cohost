import type { TranscriptLoadInput, TranscriptSourceResult } from '../types';
import { buildMetadataFallbackSegments, parseTranscriptFile } from '../utils';
import { MockTranscriptProvider } from './MockTranscriptProvider';
import type { TranscriptProvider } from './YouTubeTimedTextProvider';
import { YouTubeTimedTextProvider } from './YouTubeTimedTextProvider';

function scoreCoverage(durationSeconds: number | undefined, segmentCount: number, coveredSeconds: number): number {
  if (durationSeconds && durationSeconds > 0) {
    return Math.max(0, Math.min(1, coveredSeconds / durationSeconds));
  }
  if (segmentCount >= 80) return 0.92;
  if (segmentCount >= 35) return 0.7;
  if (segmentCount >= 12) return 0.48;
  if (segmentCount > 0) return 0.24;
  return 0;
}

function qualityFromCoverage(score: number): TranscriptSourceResult['quality'] {
  if (score >= 0.72) return 'high';
  if (score >= 0.36) return 'medium';
  return 'low';
}

export class TranscriptSourceService {
  private readonly providers: TranscriptProvider[];

  constructor(providers?: TranscriptProvider[]) {
    this.providers = providers ?? [new YouTubeTimedTextProvider()];
  }

  async resolve(input: TranscriptLoadInput): Promise<TranscriptSourceResult> {
    if (input.transcriptFileText?.trim()) {
      const segments = parseTranscriptFile(input.transcriptFileText);
      if (segments.length > 0) {
        const covered = segments.reduce((sum, segment) => sum + Math.max(0, segment.endTime - segment.startTime), 0);
        const coverageScore = scoreCoverage(input.durationSeconds, segments.length, covered);
        return {
          mode: 'user_file',
          segments,
          quality: qualityFromCoverage(coverageScore),
          coverageScore,
          providerName: 'user-upload',
          message: `Loaded ${segments.length} transcript segments from uploaded file.`
        };
      }
    }

    const providerErrors: string[] = [];
    for (const provider of this.providers) {
      let result;
      try {
        result = await provider.load(input.videoId);
      } catch (error) {
        providerErrors.push(String(error));
        continue;
      }
      if (result.segments.length > 0) {
        const covered = result.segments.reduce((sum, segment) => sum + Math.max(0, segment.endTime - segment.startTime), 0);
        const coverageScore = scoreCoverage(input.durationSeconds, result.segments.length, covered);
        return {
          mode: 'provider',
          segments: result.segments,
          quality: qualityFromCoverage(coverageScore),
          coverageScore,
          providerName: result.providerName,
          message: result.message || `Loaded ${result.segments.length} segments from provider.`
        };
      }
    }

    // Deterministic mock stays available for tests and offline local dev when explicitly injected,
    // but production fallback should remain metadata-driven rather than hallucinating transcript text.
    const metadataSegments = buildMetadataFallbackSegments(input.title || '', input.description || '');
    const covered = metadataSegments.reduce((sum, segment) => sum + Math.max(0, segment.endTime - segment.startTime), 0);
    const metadataBaseScore = scoreCoverage(input.durationSeconds, metadataSegments.length, covered);
    const coverageScore = metadataSegments.length > 0
      ? Math.max(0.42, Math.min(0.62, metadataBaseScore * 0.75))
      : 0;
    return {
      mode: 'metadata',
      segments: metadataSegments,
      quality: qualityFromCoverage(coverageScore),
      coverageScore,
      providerName: 'metadata-fallback',
      message: metadataSegments.length > 0
        ? `Captions unavailable. Using title/metadata fallback with reduced interruption confidence.${providerErrors.length > 0 ? ` Provider errors: ${providerErrors.join(' | ')}` : ''}`
        : `No captions or usable metadata were available. Co-host remarks will stay conservative.${providerErrors.length > 0 ? ` Provider errors: ${providerErrors.join(' | ')}` : ''}`
    };
  }
}

export function createTestTranscriptSourceService(): TranscriptSourceService {
  return new TranscriptSourceService([new MockTranscriptProvider()]);
}
