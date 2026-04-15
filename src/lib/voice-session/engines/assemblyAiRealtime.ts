import { createAssemblyAiStreamingToken } from '../../../frontend-api';
import type { SpeechEngine, VoiceSessionCallbacks } from '../types';

function downsampleTo16k(input: Float32Array, inputRate: number): Int16Array {
  if (inputRate === 16000) {
    const out = new Int16Array(input.length);
    for (let i = 0; i < input.length; i += 1) {
      const sample = Math.max(-1, Math.min(1, input[i] ?? 0));
      out[i] = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
    }
    return out;
  }

  const ratio = inputRate / 16000;
  const outLength = Math.max(1, Math.round(input.length / ratio));
  const out = new Int16Array(outLength);
  let offsetResult = 0;
  let offsetBuffer = 0;

  while (offsetResult < outLength) {
    const nextOffsetBuffer = Math.min(input.length, Math.round((offsetResult + 1) * ratio));
    let accum = 0;
    let count = 0;
    for (let i = offsetBuffer; i < nextOffsetBuffer; i += 1) {
      accum += input[i] ?? 0;
      count += 1;
    }
    const sample = Math.max(-1, Math.min(1, count > 0 ? accum / count : 0));
    out[offsetResult] = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
    offsetResult += 1;
    offsetBuffer = nextOffsetBuffer;
  }

  return out;
}

type AssemblyTurn = {
  type?: string;
  turn_order?: number;
  end_of_turn?: boolean;
  transcript?: string;
};

export class AssemblyAiRealtimeSpeechEngine implements SpeechEngine {
  kind: SpeechEngine['kind'] = 'assemblyai-realtime';
  private readonly callbacks: VoiceSessionCallbacks;
  private ws: WebSocket | null = null;
  private mediaStream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;
  private sourceNode: MediaStreamAudioSourceNode | null = null;
  private processorNode: ScriptProcessorNode | null = null;
  private muteNode: GainNode | null = null;
  private active = false;
  private lastCommittedKey = '';

  constructor(callbacks: VoiceSessionCallbacks) {
    this.callbacks = callbacks;
  }

  async start(): Promise<void> {
    if (!navigator.mediaDevices?.getUserMedia) {
      throw new Error('Microphone capture is unavailable in this runtime.');
    }

    this.active = true;
    this.lastCommittedKey = '';
    this.callbacks.onStatus('starting', 'Connecting AssemblyAI Live...');

    const { token } = await createAssemblyAiStreamingToken(60, 3600);
    const url = new URL('wss://streaming.assemblyai.com/v3/ws');
    url.searchParams.set('token', token);
    url.searchParams.set('sample_rate', '16000');
    url.searchParams.set('encoding', 'pcm_s16le');
    url.searchParams.set('speech_model', 'universal-streaming-english');
    url.searchParams.set('format_turns', 'false');
    url.searchParams.set('min_turn_silence', '320');
    url.searchParams.set('max_turn_silence', '900');

    try {
      this.mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true
        }
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(
        `Microphone permission failed. Allow mic access for the app/webview in the OS and browser runtime, then retry. Original error: ${message}`
      );
    }

    this.audioContext = new AudioContext({ sampleRate: 16000 });
    this.sourceNode = this.audioContext.createMediaStreamSource(this.mediaStream);
    this.processorNode = this.audioContext.createScriptProcessor(2048, 1, 1);
    this.muteNode = this.audioContext.createGain();
    this.muteNode.gain.value = 0;

    this.ws = await new Promise<WebSocket>((resolve, reject) => {
      const socket = new WebSocket(url.toString());
      socket.binaryType = 'arraybuffer';
      socket.onopen = () => resolve(socket);
      socket.onerror = () => reject(new Error('AssemblyAI websocket failed to connect.'));
      socket.onclose = () => reject(new Error('AssemblyAI websocket closed before becoming ready.'));
    });

    this.ws.onmessage = (event) => {
      if (!this.active) return;
      try {
        const payload = JSON.parse(String(event.data || '{}')) as AssemblyTurn;
        if (payload.type === 'Begin') {
          this.callbacks.onStatus('listening', 'AssemblyAI Live active.');
          return;
        }

        const transcript = String(payload.transcript || '').trim();
        if (!transcript) return;

        if (payload.end_of_turn) {
          const key = `${payload.turn_order ?? 'x'}:${transcript}`;
          if (key === this.lastCommittedKey) return;
          this.lastCommittedKey = key;
          void this.callbacks.onFinal(transcript);
          return;
        }

        this.callbacks.onInterim(transcript);
      } catch {
        // ignore malformed frames
      }
    };

    this.ws.onerror = () => {
      if (!this.active) return;
      this.callbacks.onError('AssemblyAI streaming connection failed.');
      this.callbacks.onStatus('error', 'AssemblyAI streaming connection failed.');
    };

    this.ws.onclose = () => {
      if (!this.active) return;
      this.callbacks.onError('AssemblyAI streaming session closed.');
      this.callbacks.onStatus('error', 'AssemblyAI streaming session closed.');
    };

    this.processorNode.onaudioprocess = (event) => {
      if (!this.active || !this.ws || this.ws.readyState !== WebSocket.OPEN || !this.audioContext) return;
      const channel = event.inputBuffer.getChannelData(0);
      const pcm16 = downsampleTo16k(channel, this.audioContext.sampleRate);
      if (pcm16.byteLength === 0) return;
      this.ws.send(pcm16.buffer);
    };

    this.sourceNode.connect(this.processorNode);
    this.processorNode.connect(this.muteNode);
    this.muteNode.connect(this.audioContext.destination);
  }

  async stop(): Promise<void> {
    this.active = false;
    this.callbacks.onStatus('idle', 'AssemblyAI Live stopped.');

    try {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({ type: 'Terminate' }));
      }
    } catch {
      // no-op
    }

    try {
      this.ws?.close();
    } catch {
      // no-op
    }
    this.ws = null;

    try {
      this.processorNode?.disconnect();
      this.sourceNode?.disconnect();
      this.muteNode?.disconnect();
    } catch {
      // no-op
    }
    this.processorNode = null;
    this.sourceNode = null;
    this.muteNode = null;

    if (this.audioContext) {
      await this.audioContext.close().catch(() => undefined);
      this.audioContext = null;
    }

    if (this.mediaStream) {
      for (const track of this.mediaStream.getTracks()) track.stop();
      this.mediaStream = null;
    }
  }

  async dispose(): Promise<void> {
    await this.stop();
  }
}
