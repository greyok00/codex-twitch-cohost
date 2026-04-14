import { isAmbientNoiseTranscript, isNonSpeechCaption, mergeTranscriptText, normalizeSpeechText } from '../voice/utterance';

export class TranscriptNormalizer {
  private interim = '';
  private finalized = '';
  private lastCommitted = '';

  reset() {
    this.interim = '';
    this.finalized = '';
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
    return this.interim;
  }

  pushFinal(text: string): string | null {
    const clean = text.trim();
    if (!clean || isNonSpeechCaption(clean) || isAmbientNoiseTranscript(clean)) {
      this.interim = '';
      return null;
    }
    const merged = mergeTranscriptText(this.finalized, clean).trim();
    const normalized = normalizeSpeechText(merged);
    if (!normalized) {
      this.interim = '';
      return null;
    }
    if (normalized === normalizeSpeechText(this.lastCommitted)) {
      this.interim = '';
      this.finalized = '';
      return null;
    }
    this.lastCommitted = merged;
    this.finalized = '';
    this.interim = '';
    return merged;
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
