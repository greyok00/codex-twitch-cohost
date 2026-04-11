fn main() {
    // Best-effort GPU acceleration defaults by platform/runtime.
    if std::env::var_os("COHOST_STT_GPU").is_none() {
        std::env::set_var("COHOST_STT_GPU", "1");
    }

    #[cfg(target_os = "windows")]
    {
        if std::env::var_os("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").is_none() {
            std::env::set_var(
                "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
                "--enable-gpu --enable-gpu-rasterization --enable-zero-copy --use-angle=d3d11",
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "0");
        }
    }

    twitch_cohost_bot_lib::run();
}
