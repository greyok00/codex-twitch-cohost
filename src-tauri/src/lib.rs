mod app;
mod browser;
mod commands;
mod config;
mod error;
mod llm;
mod memory;
mod personality;
mod search;
mod security;
mod state;
mod twitch;
mod utils;
mod voice;

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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_twitch_oauth_settings,
            commands::get_auth_sessions,
            commands::get_behavior_settings,
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
            commands::set_stt_config,
            commands::set_tts_voice,
            commands::set_tts_volume,
            commands::synthesize_tts_cloud,
            commands::save_avatar_image,
            commands::get_saved_avatar_image,
            commands::auto_configure_stt_fast,
            commands::connect_twitch_chat,
            commands::disconnect_twitch_chat,
            commands::send_chat_message,
            commands::set_model,
            commands::get_provider_api_key,
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
            commands::transcribe_local_audio,
            commands::transcribe_mic_chunk,
            commands::handle_voice_command,
            commands::submit_streamer_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
