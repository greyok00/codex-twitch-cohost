import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { get } from 'svelte/store';
import type { AppStatus, AuthSessions, AvatarImage, BehaviorSettings, ChatMessage, DebugBundleResult, DiagnosticsState, EventMessage, PersonalityProfile, SelfTestReport, ServiceHealthReport, SttAutoConfigResult, SttConfig, TtsVoiceSettings, VoiceRuntimeReport } from '../types';
import type { RemarkGenerationRequest, RemarkResponse } from '../youtube/types';
import { authSessionsStore, botLogStore, chatStore, debugBundleStore, diagnosticsStore, errorBannerStore, eventStore, personalityStore, selfTestReportStore, serviceHealthStore, statusStore } from '../stores/app';

let runtimeTtsVoiceName: string | null = null;
let runtimeTtsVolume = 100;

export async function loadStatus(): Promise<void> {
  try {
    const status = await invoke<AppStatus>('get_status');
    statusStore.set(status);
  } catch (error) {
    errorBannerStore.set(`Failed to load status: ${String(error)}`);
  }
}

export async function loadAuthSessions(): Promise<void> {
  try {
    const sessions = await invoke<AuthSessions>('get_auth_sessions');
    authSessionsStore.set(sessions);
  } catch (error) {
    errorBannerStore.set(`Failed to load auth sessions: ${String(error)}`);
  }
}

export async function clearAuthSessions(): Promise<void> {
  await invoke('clear_auth_sessions');
  await loadAuthSessions();
}

export async function clearBotSession(): Promise<void> {
  await invoke('clear_bot_session');
  await loadAuthSessions();
}

export async function clearStreamerSession(): Promise<void> {
  await invoke('clear_streamer_session');
  await loadAuthSessions();
}

export async function connectTwitch(
  forceReauth = false,
  authProfile: string | null = null,
  oauthRole: 'bot' | 'streamer' = 'bot'
): Promise<void> {
  await invoke('start_twitch_oauth', { forceReauth, authProfile, oauthRole });
}

export async function getTwitchOauthSettings(): Promise<{
  clientId: string;
  botUsername: string;
  channel: string;
  broadcasterLogin?: string | null;
  redirectUrl: string;
}> {
  return invoke('get_twitch_oauth_settings');
}

export async function setTwitchOauthSettings(input: {
  clientId: string;
  clientSecret?: string | null;
  botUsername?: string | null;
  channel?: string | null;
  broadcasterLogin?: string | null;
  redirectUrl?: string | null;
}): Promise<void> {
  await invoke('set_twitch_oauth_settings', { input });
}

export async function connectChat(): Promise<void> {
  await invoke('connect_twitch_chat');
}

export async function disconnectChat(): Promise<void> {
  await invoke('disconnect_twitch_chat');
}

export async function sendChat(content: string): Promise<void> {
  await invoke('send_chat_message', { content });
}

export async function setModel(model: string): Promise<void> {
  await invoke('set_model', { model });
}

export async function setProviderApiKey(providerName: string, apiKey: string): Promise<void> {
  await invoke('set_provider_api_key', { providerName, apiKey });
}

export async function getProviderApiKey(providerName: string): Promise<string | null> {
  return invoke<string | null>('get_provider_api_key', { providerName });
}

export async function getProviderModels(providerName: string): Promise<string[]> {
  return invoke<string[]>('get_provider_models', { providerName });
}

export async function fetchYoutubeTimedtext(videoId: string): Promise<string> {
  return invoke<string>('fetch_youtube_timedtext', { videoId });
}

export async function configureCloudOnlyMode(model: string): Promise<void> {
  await invoke('configure_cloud_only_mode', { model });
}

export async function setVoiceEnabled(enabled: boolean): Promise<void> {
  await invoke('set_voice_enabled', { enabled });
}

export async function setLurkMode(enabled: boolean): Promise<void> {
  await invoke('set_lurk_mode', { enabled });
}

export async function getBehaviorSettings(): Promise<BehaviorSettings> {
  return invoke<BehaviorSettings>('get_behavior_settings');
}

export async function setBehaviorSettings(cohostMode: boolean, scheduledMessagesMinutes: number | null): Promise<void> {
  await invoke('set_behavior_settings', { cohostMode, scheduledMessagesMinutes });
}

export async function searchWeb(query: string): Promise<string> {
  return invoke<string>('search_web', { query });
}

export async function openExternal(url: string): Promise<void> {
  await invoke('open_external_url', { url });
}

export async function openIsolatedTwitchWindow(profileName: string, url: string): Promise<void> {
  await invoke('open_isolated_twitch_window', { profileName, url });
}

export async function summarizeChat(): Promise<string> {
  return invoke<string>('summarize_chat');
}

export async function generateYoutubeRemark(input: RemarkGenerationRequest): Promise<RemarkResponse> {
  return invoke<RemarkResponse>('generate_youtube_remark', { input });
}

export async function loadPersonality(): Promise<void> {
  const p = await invoke<PersonalityProfile>('get_personality_profile');
  personalityStore.set(p);
}

export async function savePersonality(profile: PersonalityProfile): Promise<void> {
  await invoke('set_personality_profile', { profile });
  personalityStore.set(profile);
}

export async function clearMemory(): Promise<void> {
  await invoke('clear_memory');
}

export async function runSelfTest(): Promise<SelfTestReport> {
  const report = await invoke<SelfTestReport>('run_self_test');
  selfTestReportStore.set(report);
  return report;
}

export async function getServiceHealth(): Promise<ServiceHealthReport> {
  const report = await invoke<ServiceHealthReport>('get_service_health');
  serviceHealthStore.set(report);
  return report;
}

export async function exportDebugBundle(): Promise<DebugBundleResult> {
  const result = await invoke<DebugBundleResult>('export_debug_bundle');
  debugBundleStore.set(result);
  return result;
}

export async function handleVoiceCommand(input: string): Promise<string> {
  return invoke<string>('handle_voice_command', { input });
}

export async function transcribeLocalAudio(base64Audio: string, mimeType: string): Promise<string> {
  return invoke<string>('transcribe_local_audio', { base64Audio, mimeType });
}

export async function transcribeMicChunk(durationMs: number): Promise<string> {
  return invoke<string>('transcribe_mic_chunk', { durationMs });
}

export async function submitStreamerPrompt(text: string): Promise<void> {
  await invoke('submit_streamer_prompt', { text });
}

export async function getSttConfig(): Promise<SttConfig> {
  return invoke<SttConfig>('get_stt_config');
}

export async function setSttConfig(sttEnabled: boolean, sttBinaryPath: string | null, sttModelPath: string | null): Promise<void> {
  await invoke('set_stt_config', { sttEnabled, sttBinaryPath, sttModelPath });
}

export async function autoConfigureSttFast(): Promise<SttAutoConfigResult> {
  return invoke<SttAutoConfigResult>('auto_configure_stt_fast');
}

export async function getTtsVoice(): Promise<TtsVoiceSettings> {
  return invoke<TtsVoiceSettings>('get_tts_voice');
}

export async function verifyVoiceRuntime(): Promise<VoiceRuntimeReport> {
  return invoke<VoiceRuntimeReport>('verify_voice_runtime');
}

export async function setTtsVoice(voiceName: string | null): Promise<void> {
  await invoke('set_tts_voice', { voiceName });
  runtimeTtsVoiceName = voiceName;
}

export async function setTtsVolume(volumePercent: number): Promise<void> {
  await invoke('set_tts_volume', { volumePercent });
  runtimeTtsVolume = Math.max(0, Math.min(100, volumePercent));
}

export async function synthesizeTtsCloud(text: string, voiceName: string | null = null): Promise<string> {
  return invoke<string>('synthesize_tts_cloud', { text, voiceName });
}

export async function saveAvatarImage(dataUrl: string, fileName: string | null): Promise<AvatarImage> {
  return invoke<AvatarImage>('save_avatar_image', { dataUrl, fileName });
}

export async function getSavedAvatarImage(): Promise<AvatarImage | null> {
  return invoke<AvatarImage | null>('get_saved_avatar_image');
}

export async function registerEventListeners(): Promise<void> {
  const avatarChannel = typeof BroadcastChannel !== 'undefined' ? new BroadcastChannel('cohost-avatar-events') : null;
  let ttsVoiceName: string | null = runtimeTtsVoiceName;
  let ttsVolume = runtimeTtsVolume;
  if (typeof window !== 'undefined' && 'speechSynthesis' in window) {
    window.speechSynthesis.getVoices();
  }
  try {
    const cfg = await getTtsVoice();
    ttsVoiceName = cfg.voiceName || null;
    ttsVolume = cfg.volumePercent ?? 100;
    runtimeTtsVoiceName = ttsVoiceName;
    runtimeTtsVolume = ttsVolume;
  } catch {
    // non-fatal
  }

  function normalizeForSpeech(input: string): string {
    let text = (input || '').trim();
    if (!text) return '';
    text = text
      .replace(/\([^)]{1,120}\)/g, ' ')
      .replace(/\*[^*]{1,120}\*/g, ' ')
      .replace(/_[^_]{1,120}_/g, ' ')
      .replace(/```[\s\S]*?```/g, ' ')
      .replace(/`([^`]+)`/g, '$1')
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1')
      .replace(/:[a-z0-9_+\-]+:/gi, ' ')
      .replace(/[*_~#>|]/g, ' ')
      .replace(/[\p{Extended_Pictographic}\uFE0F]/gu, ' ')
      .replace(/\bhttps?:\/\/\S+/gi, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    text = text
      .split('')
      .map((ch) => (/^[a-z0-9 .,!?'-]$/i.test(ch) ? ch : ' '))
      .join('')
      .replace(/\s+/g, ' ')
      .trim();
    return text;
  }

  async function speakBotText(text: string) {
    if (typeof window === 'undefined' || !('speechSynthesis' in window)) return;
    const runtime = window as unknown as { __cohost_tts_speaking?: boolean };
    // Expose speaking state to mic capture loop so it can avoid feedback loops.
    runtime.__cohost_tts_speaking = true;
    const clean = normalizeForSpeech(text);
    if (!clean) {
      runtime.__cohost_tts_speaking = false;
      return;
    }
    avatarChannel?.postMessage({ type: 'speak_start', text: clean, ts: Date.now() });
    const liveVolume = runtimeTtsVolume ?? ttsVolume;
    const liveVoice = runtimeTtsVoiceName ?? ttsVoiceName;

    try {
      const dataUrl = await synthesizeTtsCloud(clean, liveVoice && liveVoice !== 'auto' ? liveVoice : null);
      await new Promise<void>((resolve) => {
        const audio = new Audio(dataUrl);
        audio.volume = Math.max(0, Math.min(1, liveVolume / 100));
        audio.onended = () => resolve();
        audio.onerror = () => resolve();
        void audio.play().catch(() => resolve());
      });
      runtime.__cohost_tts_speaking = false;
      avatarChannel?.postMessage({ type: 'speak_stop', ts: Date.now() });
      return;
    } catch {
      // fallback to browser speech synthesis below
    }

    const utterance = new SpeechSynthesisUtterance(clean);
    utterance.volume = Math.max(0, Math.min(1, liveVolume / 100));
    const voices = window.speechSynthesis.getVoices();
    const preferredHints = [
      'neural',
      'premium',
      'enhanced',
      'natural',
      'siri',
      'google us english',
      'microsoft aria',
      'microsoft guy',
      'samantha'
    ];
    let selected = null as SpeechSynthesisVoice | null;
    if (liveVoice && liveVoice !== 'auto') {
      selected = voices.find((v) => v.name.toLowerCase().includes(liveVoice.toLowerCase())) ?? null;
    }
    if (!selected) {
      selected = voices.find((v) => {
        const n = v.name.toLowerCase();
        if (n.includes('espeak') || n.includes('festival')) return false;
        return v.lang.toLowerCase().startsWith('en') && preferredHints.some((h) => n.includes(h));
      }) ?? null;
    }
    if (!selected) {
      selected = voices.find((v) => {
        const n = v.name.toLowerCase();
        return v.lang.toLowerCase().startsWith('en') && !n.includes('espeak') && !n.includes('festival');
      }) ?? null;
    }
    if (selected) utterance.voice = selected;
    utterance.rate = 0.97;
    utterance.pitch = 1.02;
    utterance.onend = () => {
      runtime.__cohost_tts_speaking = false;
      avatarChannel?.postMessage({ type: 'speak_stop', ts: Date.now() });
    };
    utterance.onerror = () => {
      runtime.__cohost_tts_speaking = false;
      avatarChannel?.postMessage({ type: 'speak_stop', ts: Date.now() });
    };
    window.speechSynthesis.cancel();
    window.speechSynthesis.speak(utterance);
  }

  await listen<ChatMessage>('chat_message', (event) => {
    chatStore.update((items) => [event.payload, ...items].slice(0, 250));
  });
  await listen<ChatMessage>('bot_response', (event) => {
    botLogStore.update((items) => [event.payload, ...items].slice(0, 250));
    ttsQueue = ttsQueue
      .then(() => speakBotText(event.payload.content))
      .catch(() => undefined);
  });
  await listen<EventMessage>('timeline_event', (event) => {
    eventStore.update((items) => [event.payload, ...items].slice(0, 250));
  });
  await listen<DiagnosticsState>('diagnostics_state', (event) => {
    diagnosticsStore.set(event.payload);
  });
  await listen<AppStatus>('status_updated', (event) => {
    statusStore.set(event.payload);
  });
  await listen<string>('error_banner', (event) => {
    const msg = String(event.payload || '').trim();
    if (!msg) return;
    eventStore.update((items) => [
      {
        id: `err-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        kind: 'error',
        content: msg,
        timestamp: new Date().toISOString()
      },
      ...items
    ].slice(0, 250));
    errorBannerStore.set('');
  });
  await listen('oauth_profile_updated', () => {
    void loadAuthSessions();
    void loadStatus();
  });
}
  let ttsQueue = Promise.resolve();
