export interface UtteranceCandidate {
  text: string;
  normalized: string;
  hits: number;
  firstSeenAt: number;
  lastSeenAt: number;
}

export function normalizeSpeechText(value: string): string {
  return (value || '')
    .toLowerCase()
    .replace(/[^a-z0-9\s]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export function mergeTranscriptText(current: string, next: string): string {
  const curr = current.trim();
  const incoming = next.trim();
  if (!curr) return incoming;
  if (!incoming) return curr;
  const currNorm = normalizeSpeechText(curr);
  const nextNorm = normalizeSpeechText(incoming);
  if (!currNorm) return incoming;
  if (!nextNorm) return curr;
  if (currNorm === nextNorm) return curr.length >= incoming.length ? curr : incoming;
  if (currNorm.includes(nextNorm)) return curr;
  if (nextNorm.includes(currNorm)) return incoming;
  const currWords = currNorm.split(' ').filter(Boolean);
  const nextWords = nextNorm.split(' ').filter(Boolean);
  const maxOverlap = Math.min(6, currWords.length, nextWords.length);
  for (let size = maxOverlap; size >= 1; size -= 1) {
    const currTail = currWords.slice(-size).join(' ');
    const nextHead = nextWords.slice(0, size).join(' ');
    if (currTail === nextHead) {
      const remainder = incoming.split(/\s+/).slice(size).join(' ').trim();
      return remainder ? `${curr} ${remainder}`.trim() : curr;
    }
  }
  return `${curr} ${incoming}`.trim();
}

export function isNonSpeechCaption(value: string): boolean {
  const t = value.trim().toLowerCase();
  if (!t) return true;
  if (/^\(?\s*(dramatic music|music|applause|laughter|laughing|silence|background noise|noise|thank you|thanks for watching|you)\s*\)?[.!?]*$/.test(t)) {
    return true;
  }
  if (/^\[[^\]]{1,48}\]$/.test(t)) return true;
  if (/^\([^)]{1,48}\)$/.test(t)) return true;
  return false;
}

const AMBIENT_NOISE_PATTERNS = [
  /^(water|water splashing|splashing|splashing water|running water|dripping water|rain|rain falling)$/i,
  /^(wind|wind noise|fan noise|air conditioner|ac noise|static|white noise|background noise|noise)$/i,
  /^(keyboard|keyboard clicking|typing|mouse clicking|clicking|door|door closing|knocking|footsteps)$/i,
  /^(breathing|heavy breathing|snoring|coughing|sneezing|mumbling|whispering|background conversation)$/i,
  /^(music playing|music|applause|laughter|laughing|crowd noise)$/i
];

const LOW_SIGNAL_EXACT = /^(uh|um|huh|hmm+|mm+|ah|oh|er|uhh|umm|hm|mhm)$/i;

export function isAmbientNoiseTranscript(value: string): boolean {
  const text = (value || '').trim().toLowerCase();
  if (!text) return true;
  if (isNonSpeechCaption(text)) return true;
  const normalized = normalizeSpeechText(text);
  if (!normalized) return true;
  if (AMBIENT_NOISE_PATTERNS.some((pattern) => pattern.test(normalized))) {
    return true;
  }
  const words = normalized.split(/\s+/).filter(Boolean);
  if (words.length <= 2) {
    if (LOW_SIGNAL_EXACT.test(normalized)) return true;
    if (words.every((word) => ['water', 'splashing', 'noise', 'wind', 'rain', 'static', 'music', 'typing', 'clicking', 'door', 'footsteps', 'breathing'].includes(word))) {
      return true;
    }
  }
  return false;
}

export function seemsWeakTranscript(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  if (!normalized) return true;
  const words = normalized.split(/\s+/).filter(Boolean);
  if (words.length === 1 && normalized.length < 8) return true;
  if (words.length < 2 && normalized.length < 8) return true;
  if (/^(uh|um|huh|mm+|hm+|yeah|yep|nope|okay|ok|right|what)$/i.test(normalized)) return true;
  if (!/[aeiou]/i.test(normalized) && normalized.length < 12) return true;
  return false;
}

export function stripWakeWords(value: string): string {
  return value
    .replace(/\b(hey chatbot|hey chat bot|hey chat-bot|hey robot|yo chatbot|chatbot|chat bot|chat-bot)\b/gi, '')
    .replace(/\s+/g, ' ')
    .trim();
}

export function recordUtteranceCandidate(
  candidates: UtteranceCandidate[],
  text: string,
  now = Date.now()
): UtteranceCandidate[] {
  const trimmed = text.trim();
  const normalized = normalizeSpeechText(trimmed);
  if (!trimmed || !normalized) return candidates;
  const next = [...candidates];
  const existingIndex = next.findIndex((item) => item.normalized === normalized);
  if (existingIndex >= 0) {
    const current = next[existingIndex];
    next[existingIndex] = {
      ...current,
      text: trimmed.length >= current.text.length ? trimmed : current.text,
      hits: current.hits + 1,
      lastSeenAt: now
    };
  } else {
    next.push({
      text: trimmed,
      normalized,
      hits: 1,
      firstSeenAt: now,
      lastSeenAt: now
    });
  }
  return next
    .sort((a, b) => scoreUtteranceCandidate(b) - scoreUtteranceCandidate(a))
    .slice(0, 12);
}

function wordCount(value: string): number {
  return normalizeSpeechText(value).split(' ').filter(Boolean).length;
}

export function scoreUtteranceCandidate(candidate: UtteranceCandidate): number {
  const ageSpread = Math.max(0, candidate.lastSeenAt - candidate.firstSeenAt);
  return candidate.hits * 6 + wordCount(candidate.text) * 2 + Math.min(candidate.text.length, 180) / 12 + Math.min(ageSpread / 400, 4);
}

export function chooseBestUtterance(buffer: string, candidates: UtteranceCandidate[]): string {
  const bestCandidate = [...candidates].sort((a, b) => scoreUtteranceCandidate(b) - scoreUtteranceCandidate(a))[0];
  const bestText = bestCandidate?.text?.trim() || '';
  const bufferText = buffer.trim();
  if (!bestText) return bufferText;
  if (!bufferText) return bestText;
  const bufferWords = wordCount(bufferText);
  const bestWords = wordCount(bestText);
  if (bufferWords >= bestWords + 2 || bufferText.length >= bestText.length + 18) {
    return bufferText;
  }
  return bestText.length >= bufferText.length ? bestText : bufferText;
}
