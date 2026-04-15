<p align="center">
  <img src="docs/screenshots/hero-command-center.png" alt="GreyOK Twitch Co-Host screenshot" width="1200" />
</p>

# GreyOK Twitch Co-Host

Desktop Twitch co-host built with Tauri, Rust, and React.

## Downloads

- Releases: https://github.com/greyok00/codex-twitch-cohost/releases
- Linux: `greyok-cohost-<version>-linux-x64.AppImage`
- Windows: `greyok-cohost-<version>-windows-x64.exe`
- macOS: `greyok-cohost-<version>-macos.dmg`

Windows and macOS builds are published, but they are still considered lightly tested compared to the Linux workflow.

## Overview

GreyOK Twitch Co-Host is a desktop control surface for:

- local co-host chat sessions
- Twitch bot + streamer OAuth and chat connection
- curated Ollama model selection
- direct voice selection with live tone sliders
- TTS replies and mic-driven interaction
- avatar stage and face rig controls
- runtime monitoring for STT, TTS, model, and Twitch state

## Current UI

The current app uses a neutral grey glass-style desktop UI.

Main sections:

- `Chat`
- `Twitch`
- `Models`
- `Voice`
- `Settings`

## Quick Start

### Run From Source

```bash
npm install
npm run tauri dev
```

### Run A Release Build

Linux AppImage:

```bash
chmod +x greyok-cohost-<version>-linux-x64.AppImage
./greyok-cohost-<version>-linux-x64.AppImage
```

## First Setup

### Model Setup

1. Open `Models`.
2. Paste your Ollama API key.
3. Click `Check Cloud Models`.
4. Pick a model from the curated list.
5. Click `Enable Cloud-Only Mode`.

### Twitch Setup

1. Open `Twitch`.
2. Save your Twitch app `Client ID`.
3. Use the default redirect URL unless you have a reason to change it:
   `http://127.0.0.1:37219/callback`
4. Connect in this order:
   - `Connect Bot`
   - `Connect Streamer`
   - `Connect Chat`

Use separate Twitch accounts for the bot and the streamer.

### Voice Setup

Open `Voice`.

- pick one of the built-in voices
- adjust:
  - `Warmth`
  - `Humor`
  - `Flirt`
  - `Edge`
  - `Energy`
  - `Story`
- optional: use `Extra direction` to push the model harder in a specific direction

These controls write directly into the live model prompt. The app is no longer built around long preset personality lists.

## Main Tabs

### Chat

- combined feed
- local IRC feed
- timeline feed
- `Send To AI`
- `Send To Twitch`
- `Mic On`

### Twitch

- Twitch OAuth configuration
- bot account auth
- streamer account auth
- chat connect/disconnect

### Models

- curated conversational and uncensored model list
- Ollama cloud account check
- cloud-only mode toggle path

### Voice

- voice selector
- direct tone sliders
- avatar stage
- rig controls

### Settings

- voice replies
- keep talking mode
- bot posting control
- auto comments
- voice volume
- diagnostics

## Development

Install dependencies:

```bash
npm install
```

Run the desktop app:

```bash
npm run tauri dev
```

Validate:

```bash
npm run check
npm run test:harness
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

Refresh the README screenshot:

```bash
npm run docs:capture
```

The capture script checks these local URLs in order:

- `http://127.0.0.1:1420/`
- `http://127.0.0.1:4180/`
- `http://127.0.0.1:5173/`

## Roadmap

### Near Term

- stabilize browser speech vs local fallback selection
- tighten TTS reliability while the mic is active
- improve mouth, eye, and glow rig feedback
- keep voice switching and persistence predictable across restarts

### Next System Pass

- reduce reliance on legacy `character` naming in config and commands
- continue simplifying the app around `voice + tone + model`
- improve diagnostics so browser STT failures report exact causes

### Future Capture / Vision Work

- browser-companion capture path instead of forcing everything through the Linux Tauri webview
- optional tab audio / browser media capture
- vision-capable model support for contextual video commentary
- YouTube / live-video co-host mode

Extension-based capture is still under consideration, but it is not committed as the immediate next step.

## Free Cloud Direction

The most realistic free cloud-ish path for speech remains browser speech in a normal Chromium session.

For the future capture roadmap, the current plan is:

- prefer a normal browser session for capture/speech if possible
- avoid locking the project into an extension-first design until the simpler bridge path is proven

## Social Links

- GitHub: https://github.com/greyok00
- Twitch: https://twitch.tv/greyok__
- YouTube: https://www.youtube.com/@GreyOK_0
