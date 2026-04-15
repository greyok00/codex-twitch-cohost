import { describe, expect, it } from 'vitest';
import { composeDirectProfile, defaultToneStudioSettings, findVoicePresetById } from '../../voice-tone';

describe('voice tone profile builder', () => {
  it('builds a direct-control profile anchored to the selected voice', () => {
    const preset = findVoicePresetById('guy');
    const profile = composeDirectProfile(defaultToneStudioSettings, preset.defaultVoice);

    expect(profile.name).toBe('Direct Control');
    expect(profile.voice).toBe('Guy');
    expect(profile.master_prompt_override).toContain('Voice: Guy.');
    expect(profile.master_prompt_override).toContain('Warmth 55/100');
    expect(profile.master_prompt_override).toContain('Avoid profanity in normal replies.');
    expect(profile.reply_rules).toContain('Stay on the latest topic');
  });

  it('folds extra direction into the high-priority prompt', () => {
    const preset = findVoicePresetById('roger');
    const profile = composeDirectProfile(
      { ...defaultToneStudioSettings, edge: 72, extraDirection: 'Keep the jokes mean but still topical.' },
      preset.defaultVoice
    );

    expect(profile.tone).toContain('sharp');
    expect(profile.chat_behavior_rules.some((rule) => rule.includes('uncensored'))).toBe(true);
    expect(profile.master_prompt_override).toContain('Extra direction: Keep the jokes mean but still topical.');
  });

  it('allows profanity when explicitly enabled', () => {
    const preset = findVoicePresetById('roger');
    const profile = composeDirectProfile(
      { ...defaultToneStudioSettings, profanityAllowed: true },
      preset.defaultVoice
    );

    expect(profile.master_prompt_override).toContain('Profanity is allowed when it improves the line naturally.');
    expect(profile.chat_behavior_rules.some((rule) => rule.includes('Profanity is allowed'))).toBe(true);
  });
});
