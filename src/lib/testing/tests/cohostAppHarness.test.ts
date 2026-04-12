import { describe, expect, it } from 'vitest';
import { CohostAppHarness } from '../CohostAppHarness';

describe('CohostAppHarness', () => {
  it('routes voice wake-word transcripts through the local reply and TTS path', async () => {
    const harness = new CohostAppHarness({
      generateReply: async ({ content }) => `Replying to: ${content}`,
      search: async (query) => `Search result for ${query}`
    });

    await harness.receiveVoiceTranscript('chatbot tell me what just happened');

    const state = harness.getState();
    expect(state.replyCount).toBe(1);
    expect(state.lastReply).toContain('chatbot tell me what just happened');
    expect(state.ttsPlayback).toEqual([state.lastReply]);
    expect(state.twitchOutbound).toHaveLength(0);
  });

  it('executes command menu from twitch and mirrors the reply to twitch output', async () => {
    const harness = new CohostAppHarness({
      generateReply: async () => 'unused',
      search: async (query) => `Search result for ${query}`
    });

    await harness.receiveTwitchChat('viewer1', '_menu');

    const state = harness.getState();
    expect(state.commandHistory).toEqual(['menu']);
    expect(state.lastReply).toContain('_search <query>');
    expect(state.twitchOutbound).toEqual([state.lastReply]);
  });

  it('replies to event notifications without requiring a wake word', async () => {
    const harness = new CohostAppHarness({
      generateReply: async ({ user, content }) => `${user} event noted: ${content}`,
      search: async (query) => `Search result for ${query}`
    });

    await harness.receiveEvent('follow', 'new follower dropped in');

    const state = harness.getState();
    expect(state.replyCount).toBe(1);
    expect(state.lastReply).toContain('follow event noted');
    expect(state.twitchOutbound).toHaveLength(0);
  });
});
