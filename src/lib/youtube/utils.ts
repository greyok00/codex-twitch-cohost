import type { PlaylistInfo, TranscriptSegment, YoutubeHistoryItem } from './types';

export function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(1, value));
}

export function normalizeText(input: string): string {
  return (input || '')
    .replace(/\s+/g, ' ')
    .trim();
}

export function hashText(input: string): string {
  let h = 2166136261;
  const text = input || '';
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0).toString(16);
}

export function seededRandom(seed: number): () => number {
  let state = (seed >>> 0) || 1;
  return () => {
    state = (1664525 * state + 1013904223) >>> 0;
    return state / 4294967296;
  };
}

export function parseYouTubeInput(input: string): PlaylistInfo | null {
  const raw = input.trim();
  if (!raw) return null;

  // direct video id
  if (/^[a-zA-Z0-9_-]{11}$/.test(raw)) {
    return { videoId: raw };
  }

  try {
    const url = new URL(raw);
    const host = url.hostname.replace(/^www\./, '');
    let videoId = '';
    let playlistId: string | undefined;

    if (host === 'youtu.be') {
      videoId = url.pathname.replace(/^\//, '').slice(0, 11);
    } else if (host === 'youtube.com' || host === 'm.youtube.com' || host === 'music.youtube.com') {
      if (url.pathname === '/watch') {
        videoId = (url.searchParams.get('v') || '').slice(0, 11);
      } else if (url.pathname.startsWith('/shorts/')) {
        videoId = url.pathname.split('/')[2]?.slice(0, 11) || '';
      } else if (url.pathname.startsWith('/embed/')) {
        videoId = url.pathname.split('/')[2]?.slice(0, 11) || '';
      }
    }

    playlistId = url.searchParams.get('list') || undefined;
    const tRaw = url.searchParams.get('t') || url.searchParams.get('start') || '';
    const startSeconds = parseStartTime(tRaw);

    if (!videoId || !/^[a-zA-Z0-9_-]{11}$/.test(videoId)) return null;
    return { videoId, playlistId, startSeconds };
  } catch {
    return null;
  }
}

function parseStartTime(raw: string): number | undefined {
  if (!raw) return undefined;
  if (/^\d+$/.test(raw)) return Number(raw);
  const m = raw.match(/^(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?$/i);
  if (!m) return undefined;
  const h = Number(m[1] || 0);
  const min = Number(m[2] || 0);
  const s = Number(m[3] || 0);
  return h * 3600 + min * 60 + s;
}

export function parseTranscriptFile(content: string): TranscriptSegment[] {
  const text = content.replace(/\r/g, '\n');

  // JSON transcript support
  try {
    const parsed = JSON.parse(text);
    if (Array.isArray(parsed)) {
      const segments = parsed
        .map((s) => ({
          startTime: Number(s.startTime ?? s.start ?? s.from ?? 0),
          endTime: Number(s.endTime ?? s.end ?? s.to ?? 0),
          text: normalizeText(String(s.text ?? '')),
          confidence: s.confidence == null ? undefined : Number(s.confidence)
        }))
        .filter((s) => s.endTime > s.startTime && s.text.length > 0);
      if (segments.length > 0) return segments;
    }
  } catch {
    // not json, continue
  }

  // SRT/VTT-ish parser
  const blocks = text.split(/\n\n+/);
  const segments: TranscriptSegment[] = [];
  const timeRe = /(\d{1,2}:)?\d{1,2}:\d{2}[,.]\d{1,3}\s*-->\s*(\d{1,2}:)?\d{1,2}:\d{2}[,.]\d{1,3}/;
  for (const block of blocks) {
    const lines = block
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean);
    if (lines.length < 2) continue;
    const timingLine = lines.find((l) => timeRe.test(l));
    if (!timingLine) continue;
    const [startRaw, endRaw] = timingLine.split(/\s*-->\s*/);
    const startTime = parseTimestamp(startRaw);
    const endTime = parseTimestamp(endRaw);
    if (!(endTime > startTime)) continue;
    const textLineStart = lines.indexOf(timingLine) + 1;
    const body = normalizeText(lines.slice(textLineStart).join(' '));
    if (!body) continue;
    segments.push({ startTime, endTime, text: body });
  }
  return segments;
}

function parseTimestamp(ts: string): number {
  const raw = ts.trim().replace(',', '.');
  const parts = raw.split(':').map((v) => Number(v));
  if (parts.some((v) => Number.isNaN(v))) return 0;
  if (parts.length === 3) return parts[0] * 3600 + parts[1] * 60 + parts[2];
  if (parts.length === 2) return parts[0] * 60 + parts[1];
  return parts[0] || 0;
}

export function buildMetadataFallbackSegments(title: string, description: string): TranscriptSegment[] {
  const merged = normalizeText(`${title || ''} ${description || ''}`);
  if (!merged) return [];
  const chunks = merged.match(/[^.!?]{25,180}[.!?]?/g) || [merged];
  return chunks.slice(0, 18).map((chunk, idx) => ({
    startTime: idx * 20,
    endTime: idx * 20 + 18,
    text: normalizeText(chunk),
    confidence: 0.35
  }));
}

export function parseYouTubeHistory(content: string): YoutubeHistoryItem[] {
  try {
    const parsed = JSON.parse(content);
    if (!Array.isArray(parsed)) return [];
    const out: YoutubeHistoryItem[] = [];
    for (const row of parsed) {
      const title = String(row.title ?? row.titleUrl ?? '').trim();
      const watchedAt = String(row.time ?? row.watchedAt ?? '').trim();
      const titleUrl = String(row.titleUrl ?? row.url ?? '').trim();
      const fromTitle = String(row.subtitles?.[0]?.name ?? row.channelTitle ?? '').trim();
      const parsedUrl = titleUrl ? parseYouTubeInput(titleUrl) : null;
      if (!parsedUrl?.videoId) continue;
      out.push({
        videoId: parsedUrl.videoId,
        title: title || parsedUrl.videoId,
        watchedAt: watchedAt || new Date(0).toISOString(),
        channelTitle: fromTitle || undefined
      });
    }
    out.sort((a, b) => (a.watchedAt < b.watchedAt ? 1 : -1));
    return out.slice(0, 300);
  } catch {
    return [];
  }
}
