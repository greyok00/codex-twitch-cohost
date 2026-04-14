#[tokio::main]
async fn main() {
    if let Err(err) = twitch_cohost_bot_lib::headless::run_cli().await {
        eprintln!("cohostd failed: {err}");
        std::process::exit(1);
    }
}
