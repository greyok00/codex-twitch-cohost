import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppStatus,
  AuthSessions,
  AvatarImage,
  AvatarRigSettings,
  BackendConsoleResult,
  BackendControlSnapshot,
  BehaviorSettings,
  CharacterStudioSettings,
  ChatMessage,
  EventMessage,
  PersonalityProfile,
  SttAutoConfigResult,
  TwitchOauthSettings,
  TtsVoiceSettings,
  VoiceInputFrame,
  VoiceRuntimeReport
} from './frontend-types';

export function onChatMessage(handler: (payload: ChatMessage) => void): Promise<UnlistenFn> {
  return listen<ChatMessage>('chat_message', (event) => handler(event.payload));
}

export function onBotResponse(handler: (payload: ChatMessage) => void): Promise<UnlistenFn> {
  return listen<ChatMessage>('bot_response', (event) => handler(event.payload));
}

export function onTimelineEvent(handler: (payload: EventMessage) => void): Promise<UnlistenFn> {
  return listen<EventMessage>('timeline_event', (event) => handler(event.payload));
}

export function onStatusUpdated(handler: (payload: AppStatus) => void): Promise<UnlistenFn> {
  return listen<AppStatus>('status_updated', (event) => handler(event.payload));
}

export function onErrorBanner(handler: (payload: string) => void): Promise<UnlistenFn> {
  return listen<string>('error_banner', (event) => handler(String(event.payload || '')));
}

export function getStatus(): Promise<AppStatus> {
  return invoke('get_status');
}

export function getAuthSessions(): Promise<AuthSessions> {
  return invoke('get_auth_sessions');
}

export function clearAuthSessions(): Promise<void> {
  return invoke('clear_auth_sessions');
}

export function clearBotSession(): Promise<void> {
  return invoke('clear_bot_session');
}

export function clearStreamerSession(): Promise<void> {
  return invoke('clear_streamer_session');
}

export function getTwitchOauthSettings(): Promise<TwitchOauthSettings> {
  return invoke('get_twitch_oauth_settings');
}

export function setTwitchOauthSettings(input: {
  clientId: string;
  clientSecret?: string | null;
  botUsername?: string | null;
  channel?: string | null;
  broadcasterLogin?: string | null;
  redirectUrl?: string | null;
}): Promise<void> {
  return invoke('set_twitch_oauth_settings', { input });
}

export function getBehaviorSettings(): Promise<BehaviorSettings> {
  return invoke('get_behavior_settings');
}

export function setBehaviorSettings(input: BehaviorSettings): Promise<void> {
  return invoke('set_behavior_settings', {
    cohostMode: input.cohostMode,
    scheduledMessagesMinutes: input.scheduledMessagesMinutes ?? null,
    minimumReplyIntervalMs: input.minimumReplyIntervalMs ?? null,
    postBotMessagesToTwitch: input.postBotMessagesToTwitch ?? false,
    topicContinuationMode: input.topicContinuationMode ?? false
  });
}

export function getCharacterStudioSettings(): Promise<CharacterStudioSettings> {
  return invoke('get_character_studio_settings');
}

export function setCharacterStudioSettings(input: CharacterStudioSettings): Promise<void> {
  return invoke('set_character_studio_settings', { input });
}

export function getTtsVoice(): Promise<TtsVoiceSettings> {
  return invoke('get_tts_voice');
}

export function setTtsVoice(voiceName: string | null): Promise<void> {
  return invoke('set_tts_voice', { voiceName });
}

export function setTtsVolume(volumePercent: number): Promise<void> {
  return invoke('set_tts_volume', { volumePercent });
}

export function verifyVoiceRuntime(): Promise<VoiceRuntimeReport> {
  return invoke('verify_voice_runtime');
}

export function autoConfigureSttFast(): Promise<SttAutoConfigResult> {
  return invoke('auto_configure_stt_fast');
}

export function setVoiceEnabled(enabled: boolean): Promise<void> {
  return invoke('set_voice_enabled', { enabled });
}

export function setLurkMode(enabled: boolean): Promise<void> {
  return invoke('set_lurk_mode', { enabled });
}

export function startTwitchOauth(forceReauth = false, authProfile: string | null = null, oauthRole: 'bot' | 'streamer' = 'bot'): Promise<void> {
  return invoke('start_twitch_oauth', { forceReauth, authProfile, oauthRole });
}

export function connectTwitchChat(): Promise<void> {
  return invoke('connect_twitch_chat');
}

export function disconnectTwitchChat(): Promise<void> {
  return invoke('disconnect_twitch_chat');
}

export function sendChatMessage(content: string): Promise<void> {
  return invoke('send_chat_message', { content });
}

export function submitVoiceSessionPrompt(text: string, callerName?: string | null): Promise<void> {
  return invoke('submit_voice_session_prompt', { text, callerName });
}

export function submitVoiceSessionFrame(frame: VoiceInputFrame, callerName?: string | null): Promise<void> {
  return invoke('submit_voice_session_frame', { frame, callerName });
}

export function getBackendControlSnapshot(): Promise<BackendControlSnapshot> {
  return invoke('get_backend_control_snapshot');
}

export function startBackendDaemon(): Promise<BackendControlSnapshot> {
  return invoke('start_backend_daemon');
}

export function launchBackendTerminal(): Promise<void> {
  return invoke('launch_backend_terminal');
}

export function saveAvatarImage(dataUrl: string, fileName: string | null): Promise<AvatarImage> {
  return invoke('save_avatar_image', { dataUrl, fileName });
}

export function getSavedAvatarImage(): Promise<AvatarImage | null> {
  return invoke('get_saved_avatar_image');
}

export function getAvatarRigSettings(): Promise<AvatarRigSettings> {
  return invoke('get_avatar_rig_settings');
}

export function setAvatarRigSettings(input: AvatarRigSettings): Promise<void> {
  return invoke('set_avatar_rig_settings', { input });
}

export function getProviderApiKey(providerName: string): Promise<string | null> {
  return invoke('get_provider_api_key', { providerName });
}

export function setProviderApiKey(providerName: string, apiKey: string): Promise<void> {
  return invoke('set_provider_api_key', { providerName, apiKey });
}

export function getProviderModels(providerName: string): Promise<string[]> {
  return invoke('get_provider_models', { providerName });
}

export function configureCloudOnlyMode(model: string): Promise<void> {
  return invoke('configure_cloud_only_mode', { model });
}

export function openExternal(url: string): Promise<void> {
  return invoke('open_external_url', { url });
}

export function savePersonality(profile: PersonalityProfile): Promise<void> {
  return invoke('set_personality_profile', { profile });
}

export function runBackendConsoleCommand(command: string, text?: string | null, path?: string | null, label?: string | null, content?: string | null): Promise<BackendConsoleResult> {
  return invoke('run_backend_console_command', { command, text, path, label, content });
}

export function synthesizeTtsCloud(text: string, voiceName?: string | null): Promise<string> {
  return invoke('synthesize_tts_cloud', { text, voiceName: voiceName ?? null });
}
