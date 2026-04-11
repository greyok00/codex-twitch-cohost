use std::{fs, path::PathBuf, process::Command};

use tauri::{AppHandle, Manager};
use url::Url;

use crate::{config::BrowserConfig, error::{AppError, AppResult}};

pub fn validate_and_open(config: &BrowserConfig, url: &str) -> AppResult<()> {
    if !config.allow_open_url {
        return Err(AppError::Url("opening URLs is disabled by config".to_string()));
    }

    let parsed = Url::parse(url).map_err(|e| AppError::Url(format!("invalid URL: {e}")))?;
    match parsed.scheme() {
        "http" | "https" => {
            open::that(parsed.as_str())
                .map_err(|e| AppError::Url(format!("failed opening URL: {e}")))?;
            Ok(())
        }
        _ => Err(AppError::Url(
            "only http and https URLs are allowed".to_string(),
        )),
    }
}

fn sanitize_profile_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

fn browser_profiles_root(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("./data"))
        .join("browser-profiles")
}

pub fn open_isolated_twitch_url(app: &AppHandle, profile_name: &str, url: &str) -> AppResult<()> {
    let parsed = Url::parse(url).map_err(|e| AppError::Url(format!("invalid URL: {e}")))?;
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if parsed.scheme() != "https" || !(host.ends_with("twitch.tv") || host.ends_with("id.twitch.tv")) {
        return Err(AppError::Url(
            "isolated auth only allows https URLs on twitch.tv".to_string(),
        ));
    }

    let profile = sanitize_profile_name(profile_name);
    let profile_dir = browser_profiles_root(app).join(profile);
    fs::create_dir_all(&profile_dir)
        .map_err(|e| AppError::Url(format!("failed creating profile dir: {e}")))?;

    #[cfg(target_os = "linux")]
    {
        let profile_str = profile_dir.to_string_lossy().to_string();

        for bin in ["firefox", "firefox-esr"] {
            let mut cmd = Command::new(bin);
            let launched = cmd
                .arg("-no-remote")
                .arg("--profile")
                .arg(&profile_str)
                .arg("--new-window")
                .arg(url)
                .spawn()
                .is_ok();
            if launched {
                return Ok(());
            }
        }

        for bin in [
            "brave-browser",
            "brave-browser-stable",
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
        ] {
            let mut cmd = Command::new(bin);
            let launched = cmd
                .arg(format!("--user-data-dir={}", profile_str))
                .arg("--new-window")
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg(url)
                .spawn()
                .is_ok();
            if launched {
                return Ok(());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if spawn_with_profile(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            &["--user-data-dir", "--new-window"],
            &profile_dir,
            url,
        )
        .is_ok()
        {
            return Ok(());
        }
        if spawn_with_profile(
            "/Applications/Firefox.app/Contents/MacOS/firefox",
            &["--profile", "--new-window"],
            &profile_dir,
            url,
        )
        .is_ok()
        {
            return Ok(());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let attempts = [
            ("chrome", vec!["--user-data-dir", "--new-window"]),
            ("msedge", vec!["--user-data-dir", "--new-window"]),
            ("firefox", vec!["--profile", "--new-window"]),
        ];
        for (bin, args) in attempts {
            if spawn_with_profile(bin, &args, &profile_dir, url).is_ok() {
                return Ok(());
            }
        }
    }

    Err(AppError::Url(
        "could not launch an isolated browser profile (install Chrome/Firefox/Edge)".to_string(),
    ))
}
