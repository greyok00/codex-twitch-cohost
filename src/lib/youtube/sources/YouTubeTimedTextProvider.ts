import { fetchYoutubeTimedtext } from '../../api/tauri';
import type { TranscriptSegment } from '../types';
import { normalizeText } from '../utils';

export interface TranscriptProviderResult {
  providerName: string;
  segments: TranscriptSegment[];
  message?: string;
}

export interface TranscriptProvider {
  load(videoId: string): Promise<TranscriptProviderResult>;
}

function parseSeconds(raw: string | null): number {
  const num = Number(raw || 0);
  return Number.isFinite(num) ? num : 0;
}

function parseTimedTextXml(xmlText: string): TranscriptSegment[] {
  if (!xmlText.trim()) return [];
  const parser = new DOMParser();
  const xml = parser.parseFromString(xmlText, 'text/xml');
  const nodes = Array.from(xml.getElementsByTagName('text'));
  return nodes
    .map((node) => {
      const startTime = parseSeconds(node.getAttribute('start'));
      const duration = parseSeconds(node.getAttribute('dur'));
      const rawText = normalizeText(node.textContent || '');
      return {
        startTime,
        endTime: startTime + Math.max(0.25, duration),
        text: rawText
      };
    })
    .filter((segment) => segment.endTime > segment.startTime && segment.text.length > 0);
}

export class YouTubeTimedTextProvider implements TranscriptProvider {
  async load(videoId: string): Promise<TranscriptProviderResult> {
    if (!videoId) {
      return { providerName: 'youtube-timedtext', segments: [], message: 'Missing YouTube video id.' };
    }
    try {
      const xmlText = await fetchYoutubeTimedtext(videoId);
      const segments = parseTimedTextXml(xmlText);
      return {
        providerName: 'youtube-timedtext',
        segments,
        message: segments.length > 0 ? `Loaded ${segments.length} caption segments from YouTube.` : 'Caption track was empty.'
      };
    } catch (error) {
      return {
        providerName: 'youtube-timedtext',
        segments: [],
        message: String(error)
      };
    }
  }
}
