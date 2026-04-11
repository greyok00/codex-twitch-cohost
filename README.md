# Twitch Cohost Bot

Cloud-first, low-latency Twitch AI co-host desktop app built with **Tauri (Rust backend)** and **Svelte (frontend)**.

## Download

- **AppImage (Linux, available now):** `GreyOK Twitch Co-Host Bot_0.1.0_amd64.AppImage`
- **Build path:** `src-tauri/target/release/bundle/appimage/`
- **Releases page:** https://github.com/greyok00/codex-twitch-cohost/releases
- **Windows `.exe` and macOS `.dmg`:** published from GitHub Actions release builds (currently untested).

## Features

- Browser-based Twitch login using Twitch Device Code flow
- Twitch IRC connect/read/send with reconnect loop
- Real EventSub websocket subscriptions for follow/sub/gift/raid/reward/online/offline events
- Automatic streamer API smoke checks on startup/login/connect
- Personality-driven prompt engine from JSON profile
- Memory store for recent chat and bot responses (sled-backed)
- Provider routing with cloud-only mode support from UI
- Local speech-to-text input (whisper-cli) with auto-send to bot prompt
- Explicit URL open controls with validation
- Lurk mode, model switching, memory clear, chat summarization
- In-app health self-test panel (auth/session/chat/eventsub/provider checks)
- Real-time diagnostics and status events streamed to UI
- GitHub Actions matrix workflow for Windows/Linux/macOS builds

## Tech Stack

- Desktop framework: `tauri` (`=2.10.3`)
- Backend: Rust + Tokio async runtime
- Frontend: Svelte + Vite
- Storage: `sled`
- Secret storage: OS keychain via `keyring`

## Repository Layout

- `src/` Svelte frontend
- `src-tauri/src/` Rust backend (commands/services/pipeline)
- `src-tauri/assets/piper-*` platform TTS resource folders
- `config.example.json` runtime configuration template
- `personality.example.json` personality profile template
- `.github/workflows/release.yml` CI/CD packaging workflow

## Prerequisites

- Node.js 20+
- npm 10+
- Rust stable toolchain
- Linux Tauri prereqs: WebKitGTK, GTK3, rsvg2, appindicator libs

Official Tauri prerequisites: https://tauri.app/start/prerequisites/

## Quick Start

1. Copy templates:

```bash
cp config.example.json config.json
cp personality.example.json personality.json
```

2. Edit `config.json`:

- Set Twitch OAuth `client_id` (and optionally `redirect_url`)
- Do **not** hardcode provider keys in config; use in-app key storage (OS keychain)

3. Install dependencies:

```bash
npm install
```

4. Run app:

```bash
npm run tauri dev
```

## Quick Walkthrough

1. Open app and go to **Twitch Login**.
2. Click **Connect Bot** and finish OAuth in browser.
3. Click **Connect Streamer** and finish OAuth in browser.
4. Click **Connect Chat** (join is blocked until both Bot + Streamer are authenticated).
5. In **Cloud AI Setup**, paste provider key once and pick model preset.
6. Use **Main Session Chat Control** for local prompts and live bot responses.
7. Optional: open **Avatar Popup** and align mouth placement.

## Twitch Login Flow

1. Click **Connect Bot** and authorize bot account.
2. Click **Connect Streamer** and authorize streamer account.
3. App opens Twitch verification URL in browser and polls device auth result.
4. Tokens are stored in local keychain/local secure store.
5. Chat connect requires both sessions when EventSub is enabled.

## Runtime Controls

- **Join Channel**: starts Twitch IRC and EventSub session
- **Leave Channel**: disconnects IRC and EventSub
- **Model Picker**: switches active provider model
- **Health Self-Test**: runs local checks for auth/session/chat/EventSub/provider health
- **Settings + Tools**: optional web search, explicit URL open, provider key storage
- **Memory Panel**: summarize or clear memory

### In-Chat Bot Commands

- `!search <query>`
- `!say <text>`
- `!model <name>`
- `!lurk on`
- `!lurk off`

## EventSub Subscriptions

Configured websocket subscriptions:

- `channel.follow` (v2)
- `channel.subscribe`
- `channel.subscription.gift`
- `channel.raid`
- `channel.channel_points_custom_reward_redemption.add`
- `stream.online`
- `stream.offline`

## Voice Input

- Voice input uses local STT (`whisper-cli`) and can auto-send transcript as a streamer prompt.
- Configure in `config.json`:
  - `voice.stt_enabled: true`
  - `voice.stt_binary_path: "whisper-cli"` (or full path)
  - `voice.stt_model_path: "/absolute/path/to/model.bin"`
- TTS remains optional and controlled separately.

## Packaging

```bash
npm run build
npm run tauri build
```

Configured targets:

- Linux AppImage
- macOS DMG
- Windows NSIS installer

### Run AppImage

```bash
chmod +x src-tauri/target/release/bundle/appimage/*.AppImage
./src-tauri/target/release/bundle/appimage/*.AppImage
```

## Integration Smoke Test

Run a local integration smoke test (checks + bounded dev boot):

```bash
npm run smoke:dev
```

## CI/CD

Workflow builds Linux/macOS/Windows matrix, uploads artifacts, and publishes on tags.

## Security Defaults

- URL opening restricted to `http/https`
- explicit user intent required for opening links
- moderation phrase blocking before response generation
- OAuth callback state validation
- Twitch/provider credentials stored in local OS keychain
- `config.json` writes are sanitized to avoid persisting secrets (tokens/client secrets/provider keys)

## Troubleshooting

- `No Twitch token available`: run OAuth connect first
- `provider request timed out`: check local model endpoint and timeout
- Piper not used: verify piper binary/model/config in asset paths or config overrides
- STT failed: verify `whisper-cli` in PATH and configured local model file
