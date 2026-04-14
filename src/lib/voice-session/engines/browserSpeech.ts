import type { SpeechEngine, VoiceSessionCallbacks } from '../types';
import { isAmbientNoiseTranscript, seemsWeakTranscript } from '../../voice/utterance';

type SpeechRecognitionAlternativeLike = {
  transcript: string;
  confidence?: number;
};

type SpeechRecognitionResultLike = {
  isFinal: boolean;
  length: number;
  [index: number]: SpeechRecognitionAlternativeLike;
};

type SpeechRecognitionEventLike = Event & {
  resultIndex: number;
  results: {
    length: number;
    [index: number]: SpeechRecognitionResultLike;
  };
};

type SpeechRecognitionLike = EventTarget & {
  continuous: boolean;
  interimResults: boolean;
  maxAlternatives: number;
  lang: string;
  onstart: ((event: Event) => void) | null;
  onend: ((event: Event) => void) | null;
  onerror: ((event: Event & { error?: string; message?: string }) => void) | null;
  onresult: ((event: SpeechRecognitionEventLike) => void) | null;
  onspeechstart: ((event: Event) => void) | null;
  onspeechend: ((event: Event) => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
};

type SpeechRecognitionCtor = new () => SpeechRecognitionLike;

function getCtor(): SpeechRecognitionCtor | null {
  if (typeof window === 'undefined') return null;
  const candidate = (window as typeof window & {
    SpeechRecognition?: SpeechRecognitionCtor;
    webkitSpeechRecognition?: SpeechRecognitionCtor;
  }).SpeechRecognition
    || (window as typeof window & { webkitSpeechRecognition?: SpeechRecognitionCtor }).webkitSpeechRecognition;
  return candidate ?? null;
}

export function browserSpeechSupported(): boolean {
  return !!getCtor();
}

export class BrowserSpeechEngine implements SpeechEngine {
  kind: SpeechEngine['kind'] = 'browser-speech';
  private recognition: SpeechRecognitionLike | null = null;
  private active = false;
  private restarting = false;
  private readonly callbacks: VoiceSessionCallbacks;

  constructor(callbacks: VoiceSessionCallbacks) {
    this.callbacks = callbacks;
  }

  private shouldIgnoreInput(transcript: string, confidence?: number, isFinal = false): boolean {
    const clean = transcript.trim();
    if (!clean) return true;
    const runtime = typeof window !== 'undefined'
      ? (window as Window & {
          __cohost_tts_speaking?: boolean;
          __cohost_tts_suppressed_until?: number;
        })
      : null;
    if (runtime?.__cohost_tts_speaking) return true;
    if ((runtime?.__cohost_tts_suppressed_until ?? 0) > Date.now()) return true;
    if (isAmbientNoiseTranscript(clean)) return true;
    if (confidence !== undefined && confidence > 0 && confidence < 0.34 && seemsWeakTranscript(clean)) {
      return true;
    }
    if (isFinal && seemsWeakTranscript(clean) && clean.split(/\s+/).length <= 2) {
      return true;
    }
    return false;
  }

  async start(): Promise<void> {
    const Ctor = getCtor();
    if (!Ctor) {
      throw new Error('Browser speech recognition is unavailable.');
    }
    this.active = true;
    this.recognition = new Ctor();
    this.recognition.continuous = true;
    this.recognition.interimResults = true;
    this.recognition.maxAlternatives = 3;
    this.recognition.lang = 'en-US';
    this.recognition.onstart = () => {
      this.callbacks.onStatus('listening', 'Browser speech active.');
    };
    this.recognition.onspeechstart = () => {
      this.callbacks.onSpeechStart?.();
    };
    this.recognition.onspeechend = () => {
      this.callbacks.onSpeechEnd?.();
    };
    this.recognition.onerror = (event) => {
      const message = event.error || event.message || 'Speech recognition error.';
      const recoverable = !['not-allowed', 'service-not-allowed'].includes(String(event.error || ''));
      this.callbacks.onError(message);
      if (!recoverable) {
        this.active = false;
      }
    };
    this.recognition.onresult = (event) => {
      let interim = '';
      let interimConfidence: number | undefined;
      const finals: string[] = [];
      for (let i = event.resultIndex; i < event.results.length; i += 1) {
        const result = event.results[i];
        const best = Array.from({ length: result.length })
          .map((_, index) => result[index])
          .filter(Boolean)
          .sort((a, b) => (b.confidence ?? 0) - (a.confidence ?? 0))[0];
        const transcript = best?.transcript?.trim() || result[0]?.transcript?.trim() || '';
        const confidence = best?.confidence;
        if (!transcript) continue;
        if (this.shouldIgnoreInput(transcript, confidence, result.isFinal)) {
          continue;
        }
        if (result.isFinal) finals.push(transcript);
        else {
          interim = transcript;
          interimConfidence = confidence;
        }
      }
      if (interim && !this.shouldIgnoreInput(interim, interimConfidence, false)) {
        this.callbacks.onInterim(interim);
      }
      if (finals.length > 0) {
        void this.callbacks.onFinal(finals.join(' ').trim());
      }
    };
    this.recognition.onend = () => {
      if (!this.active) {
        this.callbacks.onStatus('idle', 'Browser speech stopped.');
        return;
      }
      if (this.restarting) return;
      this.restarting = true;
      this.callbacks.onStatus('starting', 'Restarting browser speech...');
      window.setTimeout(() => {
        this.restarting = false;
        if (!this.active || !this.recognition) return;
        try {
          this.recognition.start();
        } catch (error) {
          this.callbacks.onError(String(error));
        }
      }, 180);
    };
    this.callbacks.onStatus('starting', 'Starting browser speech...');
    this.recognition.start();
  }

  async stop(): Promise<void> {
    this.active = false;
    if (this.recognition) {
      try {
        this.recognition.onend = null;
        this.recognition.stop();
      } catch {
        // no-op
      }
    }
    this.callbacks.onStatus('idle', 'Browser speech stopped.');
  }

  async dispose(): Promise<void> {
    this.active = false;
    if (this.recognition) {
      try {
        this.recognition.abort();
      } catch {
        // no-op
      }
    }
    this.recognition = null;
  }
}
