import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { get } from 'svelte/store';
import type { AppStatus, AuthSessions, AvatarImage, BackendControlSnapshot, BehaviorSettings, CharacterStudioSettings, ChatMessage, DebugBundleResult, DiagnosticsState, EventMessage, MemorySnapshot, MicDebugView, PersonalityProfile, PublicCallSettings, SceneSettings, SelfTestReport, ServiceHealthReport, SttAutoConfigResult, SttConfig, TtsVoiceSettings, VoiceInputFrame, VoiceRuntimeReport } from '../types';
import type { RemarkGenerationRequest, RemarkResponse } from '../youtube/types';
import { authSessionsStore, botLogStore, chatStore, debugBundleStore, diagnosticsStore, errorBannerStore, eventStore, personalityStore, selfTestReportStore, serviceHealthStore, statusStore } from '../stores/app';

let runtimeTtsVoiceName: string | null = null;
let runtimeTtsVolume = 100;
let runtimeTtsAudio: HTMLAudioElement | null = null;
let ttsQueue = Promise.resolve();
let ttsGeneration = 0;

function getRuntimeFlags(): {
  __cohost_tts_speaking?: boolean;
  __cohost_last_bot_reply_at?: number;
  __cohost_recording_active?: boolean;
  __cohost_tts_suppressed_until?: number;
} {
  return window as unknown as {
    __cohost_tts_speaking?: boolean;
    __cohost_last_bot_reply_at?: number;
    __cohost_recording_active?: boolean;
    __cohost_tts_suppressed_until?: number;
  };
}

export function stopBotSpeech(): void {
  if (typeof window === 'undefined') return;
  ttsGeneration += 1;
  ttsQueue = Promise.resolve();
  const runtime = getRuntimeFlags();
  runtime.__cohost_tts_speaking = false;
  runtime.__cohost_tts_suppressed_until = Date.now() + 1800;
  try {
    window.speechSynthesis.cancel();
  } catch {
    // no-op
  }
  if (runtimeTtsAudio) {
    try {
      runtimeTtsAudio.pause();
      runtimeTtsAudio.currentTime = 0;
    } catch {
      // no-op
    }
    runtimeTtsAudio = null;
  }
  for (const media of Array.from(document.querySelectorAll<HTMLMediaElement>('audio, video'))) {
    try {
      media.pause();
    } catch {
      // no-op
    }
  }
  try {
    const avatarChannel = typeof BroadcastChannel !== 'undefined' ? new BroadcastChannel('cohost-avatar-events') : null;
    avatarChannel?.postMessage({ type: 'speak_stop', ts: Date.now() });
    avatarChannel?.close();
  } catch {
    // no-op
  }
}

export function setRecordingSpeechBlock(active: boolean): void {
  if (typeof window === 'undefined') return;
  const runtime = getRuntimeFlags();
  runtime.__cohost_recording_active = active;
  runtime.__cohost_tts_suppressed_until = active ? Date.now() + 60_000 : Date.now() + 400;
  if (active) stopBotSpeech();
}

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

export async function getSceneSettings(): Promise<SceneSettings> {
  return invoke<SceneSettings>('get_scene_settings');
}

export async function setSceneSettings(
  mode: SceneSettings['mode'],
  maxTurnsBeforePause: number,
  allowExternalTopicChanges: boolean,
  secondaryCharacterSlug: string
): Promise<void> {
  await invoke('set_scene_settings', { mode, maxTurnsBeforePause, allowExternalTopicChanges, secondaryCharacterSlug });
}

export async function getCharacterStudioSettings(): Promise<CharacterStudioSettings> {
  return invoke<CharacterStudioSettings>('get_character_studio_settings');
}

export async function setCharacterStudioSettings(input: CharacterStudioSettings): Promise<void> {
  await invoke('set_character_studio_settings', { input });
}

export async function setBehaviorSettings(
  cohostMode: boolean,
  scheduledMessagesMinutes: number | null,
  minimumReplyIntervalMs?: number | null,
  postBotMessagesToTwitch?: boolean | null,
  topicContinuationMode?: boolean | null
): Promise<void> {
  await invoke('set_behavior_settings', { cohostMode, scheduledMessagesMinutes, minimumReplyIntervalMs, postBotMessagesToTwitch, topicContinuationMode });
}

export async function getPublicCallSettings(): Promise<PublicCallSettings> {
  return invoke<PublicCallSettings>('get_public_call_settings');
}

export async function setPublicCallSettings(enabled: boolean, defaultCharacterSlug?: string | null): Promise<PublicCallSettings> {
  return invoke<PublicCallSettings>('set_public_call_settings', { enabled, defaultCharacterSlug });
}

export async function rotatePublicCallToken(): Promise<PublicCallSettings> {
  return invoke<PublicCallSettings>('rotate_public_call_token');
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

export async function getMemorySnapshot(): Promise<MemorySnapshot> {
  return invoke<MemorySnapshot>('get_memory_snapshot');
}

export async function openMemoryLog(): Promise<void> {
  await invoke('open_memory_log');
}

export async function upsertPinnedMemory(label: string, content: string): Promise<void> {
  await invoke('upsert_pinned_memory', { input: { label, content } });
}

export async function deletePinnedMemory(label: string): Promise<void> {
  await invoke('delete_pinned_memory', { label });
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

export async function submitVoiceSessionPrompt(text: string, callerName?: string | null): Promise<void> {
  await invoke('submit_voice_session_prompt', { text, callerName });
}

export async function submitVoiceSessionFrame(frame: VoiceInputFrame, callerName?: string | null): Promise<void> {
  await invoke('submit_voice_session_frame', { frame, callerName });
}

export async function transcribeLocalAudio(base64Audio: string, mimeType: string): Promise<string> {
  return invoke<string>('transcribe_local_audio', { base64Audio, mimeType });
}

export async function transcribeMicChunk(durationMs: number, preferLocal = false): Promise<string> {
  return invoke<string>('transcribe_mic_chunk', { durationMs, preferLocal });
}

export async function transcribeMicChunkLocal(durationMs: number): Promise<string> {
  return invoke<string>('transcribe_mic_chunk_local', { durationMs });
}

export async function captureMicDebug(durationMs: number): Promise<MicDebugView> {
  return invoke<MicDebugView>('capture_mic_debug', { durationMs });
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

export async function synthesizeTtsReaction(reaction: string, voiceName: string | null = null): Promise<string> {
  return invoke<string>('synthesize_tts_reaction', { reaction, voiceName });
}

export async function getBackendControlSnapshot(): Promise<BackendControlSnapshot> {
  return invoke<BackendControlSnapshot>('get_backend_control_snapshot');
}

export async function startBackendDaemon(): Promise<BackendControlSnapshot> {
  return invoke<BackendControlSnapshot>('start_backend_daemon');
}

export async function launchBackendTerminal(): Promise<void> {
  await invoke('launch_backend_terminal');
}

export async function playLocalSpeech(text: string, voiceName: string | null = null, volumePercent?: number): Promise<void> {
  const clean = (text || '').trim();
  if (!clean) return;
  stopBotSpeech();
  const dataUrl = await synthesizeTtsCloud(clean, voiceName);
  await new Promise<void>((resolve) => {
    const audio = new Audio(dataUrl);
    runtimeTtsAudio = audio;
    audio.volume = Math.max(0, Math.min(1, (volumePercent ?? runtimeTtsVolume ?? 100) / 100));
    audio.onended = () => {
      runtimeTtsAudio = null;
      resolve();
    };
    audio.onerror = () => {
      runtimeTtsAudio = null;
      resolve();
    };
    void audio.play().catch(() => resolve());
  });
}

export async function playTtsReaction(reaction: string, voiceName: string | null = null, volumePercent?: number): Promise<void> {
  stopBotSpeech();
  const dataUrl = await synthesizeTtsReaction(reaction, voiceName);
  await new Promise<void>((resolve) => {
    const audio = new Audio(dataUrl);
    runtimeTtsAudio = audio;
    audio.volume = Math.max(0, Math.min(1, (volumePercent ?? runtimeTtsVolume ?? 100) / 100));
    audio.onended = () => {
      runtimeTtsAudio = null;
      resolve();
    };
    audio.onerror = () => {
      runtimeTtsAudio = null;
      resolve();
    };
    void audio.play().catch(() => resolve());
  });
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
      .replace(/\bgreyok\b/gi, 'Grey Okay')
      .replace(/\bgreyok__\b/gi, 'Grey Okay')
      .replace(/\bgrey ok\b/gi, 'Grey Okay')
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
    text = text
      .replace(/\b([A-Za-z]+)\s+(s|re|ve|ll|d|m)\b(?=(?:\s+[A-Za-z])|[.,!?]|$)/gi, "$1'$2")
      .replace(/\b(can)\s+t\b/gi, "can't")
      .replace(/\b(won)\s+t\b/gi, "won't")
      .replace(/\b(shan)\s+t\b/gi, "shan't")
      .replace(/\b([A-Za-z]+n)\s+t\b(?=(?:\s+[A-Za-z])|[.,!?]|$)/gi, "$1't")
      .replace(/\s+/g, ' ')
      .trim();
    return text;
  }

  function resolveBrowserTtsProfile(voiceName: string | null): {
    engineVoiceHint: string | null;
    rate: number;
    pitch: number;
  } {
    return {
      engineVoiceHint: voiceName,
      rate: 0.97,
      pitch: 1.02
    };
  }

  async function speakBotText(text: string, generation: number) {
    if (typeof window === 'undefined' || !('speechSynthesis' in window)) return;
    if (generation !== ttsGeneration) return;
    const runtime = getRuntimeFlags();
    const waitStart = Date.now();
    while (runtime.__cohost_recording_active || (runtime.__cohost_tts_suppressed_until ?? 0) > Date.now()) {
      if (Date.now() - waitStart > 12000) return;
      if (generation !== ttsGeneration) return;
      await new Promise((resolve) => setTimeout(resolve, 120));
    }
    if (generation !== ttsGeneration) return;
    // Expose speaking state to mic capture loop so it can avoid feedback loops.
    runtime.__cohost_tts_speaking = true;
    runtime.__cohost_tts_suppressed_until = Date.now() + 30_000;
    const clean = normalizeForSpeech(text);
    if (!clean) {
      runtime.__cohost_tts_speaking = false;
      return;
    }
    const liveVolume = runtimeTtsVolume ?? ttsVolume;
    const liveVoice = runtimeTtsVoiceName ?? ttsVoiceName;
    const browserProfile = resolveBrowserTtsProfile(liveVoice);

    try {
      const dataUrl = await synthesizeTtsCloud(clean, liveVoice && liveVoice !== 'auto' ? liveVoice : null);
      await new Promise<void>((resolve) => {
        const audio = new Audio(dataUrl);
        runtimeTtsAudio = audio;
        let started = false;
        audio.volume = Math.max(0, Math.min(1, liveVolume / 100));
        audio.onplay = () => {
          if (started) return;
          started = true;
          avatarChannel?.postMessage({ type: 'speak_start', text: clean, ts: Date.now() });
        };
        audio.onended = () => {
          runtimeTtsAudio = null;
          runtime.__cohost_tts_suppressed_until = Date.now() + 1800;
          resolve();
        };
        audio.onerror = () => {
          runtimeTtsAudio = null;
          runtime.__cohost_tts_suppressed_until = Date.now() + 1800;
          resolve();
        };
        void audio.play().catch(() => resolve());
      });
      runtime.__cohost_tts_speaking = false;
      runtime.__cohost_tts_suppressed_until = Date.now() + 1800;
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
    if (browserProfile.engineVoiceHint && liveVoice && liveVoice !== 'auto') {
      selected = voices.find((v) => v.name.toLowerCase().includes(browserProfile.engineVoiceHint!.toLowerCase())) ?? null;
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
    utterance.rate = browserProfile.rate;
    utterance.pitch = browserProfile.pitch;
    utterance.onend = () => {
      runtime.__cohost_tts_speaking = false;
      runtime.__cohost_tts_suppressed_until = Date.now() + 1800;
      avatarChannel?.postMessage({ type: 'speak_stop', ts: Date.now() });
    };
    utterance.onerror = () => {
      runtime.__cohost_tts_speaking = false;
      runtime.__cohost_tts_suppressed_until = Date.now() + 1800;
      avatarChannel?.postMessage({ type: 'speak_stop', ts: Date.now() });
    };
    window.speechSynthesis.cancel();
    avatarChannel?.postMessage({ type: 'speak_start', text: clean, ts: Date.now() });
    window.speechSynthesis.speak(utterance);
  }

  await listen<ChatMessage>('chat_message', (event) => {
    chatStore.update((items) => [event.payload, ...items].slice(0, 250));
  });
  await listen<ChatMessage>('bot_response', (event) => {
    const runtime = getRuntimeFlags();
    runtime.__cohost_last_bot_reply_at = Date.now();
    botLogStore.update((items) => [event.payload, ...items].slice(0, 250));
    const generation = ttsGeneration;
    ttsQueue = ttsQueue
      .then(() => speakBotText(event.payload.content, generation))
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
