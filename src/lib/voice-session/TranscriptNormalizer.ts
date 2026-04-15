import {
  chooseBestUtterance,
  isAmbientNoiseTranscript,
  isNonSpeechCaption,
  mergeTranscriptText,
  normalizeSpeechText,
  recordUtteranceCandidate,
  type UtteranceCandidate
} from '../voice/utterance';

export class TranscriptNormalizer {
  private interim = '';
  private finalized = '';
  private lastCommitted = '';
  private candidates: UtteranceCandidate[] = [];

  reset() {
    this.interim = '';
    this.finalized = '';
    this.candidates = [];
  }

  getInterim() {
    return this.interim;
  }

  pushInterim(text: string): string {
    if (isAmbientNoiseTranscript(text)) {
      this.interim = '';
      return '';
    }
    this.interim = mergeTranscriptText(this.finalized, text).trim();
    this.candidates = recordUtteranceCandidate(this.candidates, this.interim);
    return this.interim;
  }

  pushFinal(text: string): string | null {
    const clean = text.trim();
    if (!clean || isNonSpeechCaption(clean) || isAmbientNoiseTranscript(clean)) {
      this.interim = '';
      this.candidates = [];
      return null;
    }
    const priorInterim = this.interim.trim();
    const merged = mergeTranscriptText(priorInterim || this.finalized, clean).trim();
    this.candidates = recordUtteranceCandidate(this.candidates, merged);
    const selected = chooseBestUtterance(merged, this.candidates).trim();
    const normalizedInterim = normalizeSpeechText(priorInterim);
    const normalizedSelected = normalizeSpeechText(selected);
    const normalizedFinal = normalizeSpeechText(clean);
    const committedText = shouldPreferInterim(priorInterim, clean)
      ? priorInterim
      : (normalizedInterim && normalizedSelected && normalizedInterim.includes(normalizedSelected)
        ? priorInterim
        : selected);
    const normalized = normalizeSpeechText(committedText);
    if (!normalized) {
      this.interim = '';
      this.candidates = [];
      return null;
    }
    if (normalized === normalizeSpeechText(this.lastCommitted)) {
      this.interim = '';
      this.finalized = '';
      this.candidates = [];
      return null;
    }
    this.lastCommitted = committedText;
    this.finalized = '';
    this.interim = '';
    this.candidates = [];
    return committedText;
  }

  pushFallbackChunk(text: string): string | null {
    const clean = text.trim();
    if (!clean || isNonSpeechCaption(clean) || isAmbientNoiseTranscript(clean)) return null;
    const normalized = normalizeSpeechText(clean);
    if (!normalized || normalized === normalizeSpeechText(this.lastCommitted)) {
      return null;
    }
    this.lastCommitted = clean;
    return clean;
  }
}

function shouldPreferInterim(interim: string, finalText: string): boolean {
  const interimNorm = normalizeSpeechText(interim);
  const finalNorm = normalizeSpeechText(finalText);
  if (!interimNorm || !finalNorm) return false;
  if (interimNorm === finalNorm) return false;
  const interimWords = interimNorm.split(' ').filter(Boolean);
  const finalWords = finalNorm.split(' ').filter(Boolean);
  if (interimWords.length < 3 || finalWords.length === 0) return false;
  const overlap = finalWords.filter((word) => interimWords.includes(word)).length;
  return finalWords.length < interimWords.length && overlap <= 1;
}
