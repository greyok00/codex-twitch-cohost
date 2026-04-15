mod app;
mod browser;
mod commands;
pub mod config;
pub mod error;
pub mod headless;
pub mod llm;
pub mod memory;
pub mod personality;
mod search;
pub mod security;
pub mod state;
pub mod tts;
mod twitch;
mod utils;
pub mod voice;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .level_for("sled", log::LevelFilter::Warn)
                .level_for("tokio_tungstenite", log::LevelFilter::Warn)
                .level_for("tungstenite", log::LevelFilter::Warn)
                .level_for("hyper", log::LevelFilter::Warn)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = app::bootstrap(app.handle().clone()).map_err(|err| {
                std::io::Error::new(std::io::ErrorKind::Other, format!("startup failed: {err}"))
            })?;
            let _ = app.manage(state);
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = commands::startup_spawn_backend_daemon(&handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_twitch_oauth_settings,
            commands::get_auth_sessions,
            commands::get_behavior_settings,
            commands::get_scene_settings,
            commands::get_character_studio_settings,
            commands::get_avatar_rig_settings,
            commands::get_public_call_settings,
            commands::get_stt_config,
            commands::get_tts_voice,
            commands::verify_voice_runtime,
            commands::get_service_health,
            commands::clear_auth_sessions,
            commands::clear_bot_session,
            commands::clear_streamer_session,
            commands::run_self_test,
            commands::export_debug_bundle,
            commands::start_twitch_oauth,
            commands::set_twitch_oauth_settings,
            commands::set_behavior_settings,
            commands::set_scene_settings,
            commands::set_character_studio_settings,
            commands::set_avatar_rig_settings,
            commands::set_public_call_settings,
            commands::set_stt_config,
            commands::set_tts_voice,
            commands::set_tts_volume,
            commands::synthesize_tts_cloud,
            commands::get_backend_control_snapshot,
            commands::start_backend_daemon,
            commands::run_backend_console_command,
            commands::launch_backend_terminal,
            commands::save_avatar_image,
            commands::get_saved_avatar_image,
            commands::auto_configure_stt_fast,
            commands::connect_twitch_chat,
            commands::disconnect_twitch_chat,
            commands::send_chat_message,
            commands::set_model,
            commands::get_provider_api_key,
            commands::create_assemblyai_streaming_token,
            commands::start_assemblyai_live_stt,
            commands::stop_assemblyai_live_stt,
            commands::set_assemblyai_live_stt_paused,
            commands::get_provider_models,
            commands::fetch_youtube_timedtext,
            commands::set_provider_api_key,
            commands::configure_cloud_only_mode,
            commands::set_voice_enabled,
            commands::set_lurk_mode,
            commands::search_web,
            commands::open_external_url,
            commands::open_isolated_twitch_window,
            commands::summarize_chat,
            commands::generate_youtube_remark,
            commands::get_personality_profile,
            commands::set_personality_profile,
            commands::clear_memory,
            commands::get_memory_snapshot,
            commands::open_memory_log,
            commands::upsert_pinned_memory,
            commands::delete_pinned_memory,
            commands::transcribe_local_audio,
            commands::transcribe_mic_chunk,
            commands::transcribe_mic_chunk_local,
            commands::capture_mic_debug,
            commands::handle_voice_command,
            commands::submit_streamer_prompt,
            commands::submit_voice_session_prompt,
            commands::submit_voice_session_frame,
            commands::rotate_public_call_token,
            commands::synthesize_tts_reaction,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
