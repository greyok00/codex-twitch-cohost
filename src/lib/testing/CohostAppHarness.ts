export interface HarnessMessage {
  channel: 'local' | 'twitch' | 'event' | 'voice';
  user: string;
  content: string;
}

export interface HarnessState {
  chatLog: HarnessMessage[];
  lastReply: string;
  lastSkipReason: string | null;
  commandHistory: string[];
  sttTranscripts: string[];
  ttsPlayback: string[];
  twitchOutbound: string[];
  replyCount: number;
}

export interface HarnessDeps {
  generateReply: (input: { source: 'twitch' | 'voice' | 'event'; user: string; content: string }) => Promise<string>;
  search: (query: string) => Promise<string>;
}

export class MockTwitchTransport {
  readonly inbound: HarnessMessage[] = [];
  readonly outbound: string[] = [];

  receive(user: string, content: string): HarnessMessage {
    const message = { channel: 'twitch' as const, user, content };
    this.inbound.push(message);
    return message;
  }

  send(content: string): void {
    this.outbound.push(content);
  }
}

export class MockSttRuntime {
  readonly transcripts: string[] = [];

  pushTranscript(text: string): string {
    this.transcripts.push(text);
    return text;
  }
}

export class MockTtsRuntime {
  readonly spoken: string[] = [];

  async speak(text: string): Promise<void> {
    this.spoken.push(text);
  }
}

function normalizeCommand(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  if (/^[_!./]/.test(trimmed)) return trimmed.slice(1).trim();

  const lowered = trimmed.toLowerCase();
  const voicePrefixes = ['command ', 'underscore '];
  for (const prefix of voicePrefixes) {
    if (lowered.startsWith(prefix)) {
      return trimmed.slice(prefix.length).trim();
    }
  }
  return null;
}

function containsWakeWord(input: string): boolean {
  const lowered = input.toLowerCase();
  return ['chatbot', 'chat bot', 'hey chatbot', 'hey robot'].some((key) => lowered.includes(key));
}

function extractSearchQuery(input: string): string | null {
  const trimmed = input.trim();
  const lowered = trimmed.toLowerCase();
  const prefixes = ['_search ', '!search ', '.search ', '/search ', 'command search ', 'command web search ', 'web search ', 'do a web search ', 'do web search '];
  for (const prefix of prefixes) {
    if (lowered.startsWith(prefix)) {
      return trimmed.slice(prefix.length).trim() || null;
    }
  }
  return null;
}

function helpText(): string {
  return [
    'Command menu',
    '_menu: show this help',
    '_search <query>: run a web search',
    'Say "command search ..." or "underscore menu" for voice commands'
  ].join('\n');
}

export class CohostAppHarness {
  readonly twitch = new MockTwitchTransport();
  readonly stt = new MockSttRuntime();
  readonly tts = new MockTtsRuntime();

  private readonly deps: HarnessDeps;
  private readonly state: HarnessState = {
    chatLog: [],
    lastReply: '',
    lastSkipReason: null,
    commandHistory: [],
    sttTranscripts: [],
    ttsPlayback: [],
    twitchOutbound: [],
    replyCount: 0
  };

  constructor(deps: HarnessDeps) {
    this.deps = deps;
  }

  getState(): HarnessState {
    return {
      ...this.state,
      chatLog: [...this.state.chatLog],
      commandHistory: [...this.state.commandHistory],
      sttTranscripts: [...this.state.sttTranscripts],
      ttsPlayback: [...this.state.ttsPlayback],
      twitchOutbound: [...this.state.twitchOutbound]
    };
  }

  async receiveTwitchChat(user: string, content: string): Promise<void> {
    const message = this.twitch.receive(user, content);
    this.state.chatLog.push(message);
    await this.processInbound('twitch', user, content, true);
  }

  async receiveVoiceTranscript(content: string): Promise<void> {
    const transcript = this.stt.pushTranscript(content);
    this.state.sttTranscripts.push(transcript);
    this.state.chatLog.push({ channel: 'voice', user: 'mic', content: transcript });
    await this.processInbound('voice', 'mic', transcript, false);
  }

  async receiveEvent(kind: string, content: string): Promise<void> {
    this.state.chatLog.push({ channel: 'event', user: kind, content });
    await this.processInbound('event', kind, content, false);
  }

  private async processInbound(source: 'twitch' | 'voice' | 'event', user: string, content: string, allowTwitchSend: boolean): Promise<void> {
    const command = normalizeCommand(content);
    if (command) {
      this.state.commandHistory.push(command);
      await this.handleCommand(command, allowTwitchSend);
      return;
    }

    const directSearch = extractSearchQuery(content);
    if (directSearch) {
      const reply = await this.deps.search(directSearch);
      await this.publishReply(reply, allowTwitchSend);
      return;
    }

    const shouldReply = source === 'event' || containsWakeWord(content);
    if (!shouldReply) {
      this.state.lastSkipReason = 'no wake word or event trigger';
      return;
    }

    const reply = await this.deps.generateReply({ source, user, content });
    await this.publishReply(reply, false);
  }

  private async handleCommand(command: string, allowTwitchSend: boolean): Promise<void> {
    const lowered = command.toLowerCase();
    if (lowered === 'menu' || lowered === 'help' || lowered === 'commands') {
      await this.publishReply(helpText(), allowTwitchSend);
      return;
    }
    if (lowered.startsWith('search ')) {
      const query = command.slice(7).trim();
      const reply = await this.deps.search(query);
      await this.publishReply(reply, allowTwitchSend);
      return;
    }
    this.state.lastSkipReason = `unknown command: ${command}`;
  }

  private async publishReply(reply: string, sendToTwitch: boolean): Promise<void> {
    const clean = reply.trim();
    if (!clean) {
      this.state.lastSkipReason = 'empty reply';
      return;
    }
    this.state.lastReply = clean;
    this.state.replyCount += 1;
    this.state.chatLog.push({ channel: 'local', user: 'bot', content: clean });
    await this.tts.speak(clean);
    this.state.ttsPlayback.push(clean);
    if (sendToTwitch) {
      this.twitch.send(clean);
      this.state.twitchOutbound.push(clean);
    }
    this.state.lastSkipReason = null;
  }
}
