<p align="center">
  <img src="https://raw.githubusercontent.com/greyok00/codex-twitch-cohost/main/public/top-logo.png" alt="GreyOK logo" width="920" />
</p>

<h1 align="center">GreyOK Twitch Co-Host</h1>

<p align="center">
  Desktop Twitch co-host built with Tauri, Rust, and Svelte.
</p>

## Downloads

- Releases: https://github.com/greyok00/codex-twitch-cohost/releases
- Linux: `greyok-cohost-<version>-linux-x64.AppImage`
- Windows: `greyok-cohost-<version>-windows-x64.exe`
- macOS: `greyok-cohost-<version>-macos.dmg`

## What It Does

GreyOK Twitch Co-Host is a desktop co-host app for:

- running a Twitch bot with separate `Bot` and `Streamer` accounts
- switching between `Local Only` testing and live Twitch chat
- listening with local speech-to-text
- speaking back with text-to-speech
- keeping a local chat timeline inside the app
- optionally relaying bot replies into Twitch chat
- running a floating avatar window for OBS-style overlay use
- storing durable memory facts separately from raw chat history
- checking STT, TTS, Twitch, and provider health from built-in diagnostics

## Screenshots

### Main Session

![Main session](docs/screenshots/01-main-session.png)

### Auth And Channel

![Auth and channel](docs/screenshots/02-auth-channel.png)

### Cloud AI Setup

![Cloud AI setup](docs/screenshots/03-cloud-ai.png)

### Settings And Diagnostics

![Settings and diagnostics](docs/screenshots/04-settings.png)

### About

![About](docs/screenshots/05-about.png)

## Quick Start

### 1. Run It

Linux AppImage:

```bash
chmod +x greyok-cohost-<version>-linux-x64.AppImage
./greyok-cohost-<version>-linux-x64.AppImage
```

From source:

```bash
npm install
npm run tauri dev
```

### 2. Connect Twitch

Open `Auth & Channel`.

Fill in:

- Twitch Client ID
- Bot username
- Streamer login

Then connect in this order:

1. `Connect Bot`
2. `Connect Streamer`
3. turn live Twitch mode on in the main session controls only when you actually want chat connected

Important:

- Bot and Streamer must be different Twitch accounts.
- The target channel should follow the Streamer account.
- You can stay logged in but keep the app in `Local Only` mode for testing.

### 3. Connect The AI

Open `Cloud AI Setup`.

Use the built-in flow:

1. `Open Ollama`
2. create an account if needed
3. generate an API key
4. paste the key into the app
5. `Check Cloud Models`
6. choose a model preset
7. `Enable Cloud-Only Mode`

The cloud picker is intentionally short:

- 4 conversational models
- 4 uncensored models

Each preset is labeled clearly instead of dumping a giant raw model list.

### 4. Set Up Voice

Open `Settings`.

The app auto-configures STT on startup when possible.

Then:

1. choose a TTS voice
2. set the volume
3. click `Apply Voice`
4. click `Verify STT/TTS`

If verification is green, the voice pipeline is ready.

## Main Workflow

The main session view is the primary workspace.

You can:

- type directly to the AI
- use the mic toggle for local speech input
- keep the app in `Local Only` mode for testing
- switch to live Twitch mode when you want the bot in chat
- choose whether bot replies stay local or post to Twitch
- adjust reply speed and cohost pacing
- open the floating avatar window

### Main Session Controls

The main session includes:

- `Fast`, `Medium`, and `Long` model modes
- `Auto comments`
- `Keep talking`
- `Mic`
- live transcript / phase readout
- Twitch/local mode controls
- bot posting control

How they work:

- `Local Only` keeps the app testable without using Twitch chat
- live Twitch mode connects the Twitch chat path
- bot posting control decides whether the bot only replies in the app or also posts to Twitch
- `Auto comments` allows ambient cohost chatter
- `Keep talking` pushes the bot to continue a topic instead of resetting into constant questions

## Personality

Open `Personality`.

The current system supports:

- short explainer presets instead of character-name clutter
- normal conversational presets
- stronger stylized presets
- random character generation
- instruction-based custom generation
- full custom profile editing

The active personality changes delivery and tone, but memory is meant to persist across personality or model changes.

## Memory

Open `Memory`.

The app now stores two kinds of memory:

1. raw memory log
2. pinned memory

Raw memory includes things like:

- recent chat
- bot replies
- extracted preferences
- goals
- corrections
- setup notes
- repeated important facts
- relationship framing
- address preferences

Pinned memory is stronger.

Use it for facts that must stay stable, such as:

- preferred name
- nickname
- relationship label
- role framing
- hard rules the bot should remember

Pinned memory is injected into prompt context before the regular memory log.

### Memory Log Tools

The memory tab lets you:

- refresh memory
- summarize chat
- reset memory
- copy the JSONL memory log path
- open the log file directly
- save or remove pinned memory entries

## Avatar

The avatar system is meant for OBS-style overlay use.

You can:

- upload an avatar image
- open the floating avatar window
- place the mouth and eyebrow rig
- resize/snap the popup around the image

The popup is separate from the main app so it can be used as its own layer.

## Settings, Diagnostics, And Voice

`Settings` contains:

- TTS voice selection
- volume
- STT/TTS verification
- mic debug capture
- diagnostics
- self-test
- debug export
- quick non-word TTS reaction previews

Current reaction previews include:

- `Soft hum`
- `Thinking hum`
- `Surprised`
- `Excited`

There is also a custom reaction box for short non-word sounds like `mmm...` or `oh!`.

## Troubleshooting

### The AI is not replying

Check:

1. `Cloud AI Setup` has a saved Ollama API key
2. a real model preset is selected
3. the provider is healthy in diagnostics
4. if testing locally, stay in `Local Only`
5. if expecting Twitch chat output, make sure bot posting is enabled

### The mic is not hearing you

Open `Settings` and run:

- `Verify STT/TTS`
- `Mic Debug Capture`

If STT is not ready:

- restart the app once
- let auto-configure finish
- confirm the bundled/local Vosk model exists
- inspect the mic debug transcript and WAV path

### The bot is using the wrong Twitch account

Clear sessions and reconnect in the correct order:

1. Bot
2. Streamer
3. then enable live Twitch mode only when needed

The bot and streamer accounts must remain separate.

## Development

Install and run:

```bash
npm install
npm run tauri dev
```

Verify:

```bash
npm run lint
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

## Packaging

Local build:

```bash
npm run build
npm run tauri build
```

Cross-platform releases are built by GitHub Actions on tag push:

- Ubuntu builds the AppImage
- Windows builds the portable EXE
- macOS builds the DMG

## Social Links

- GitHub: https://github.com/greyok00
- Twitch: https://twitch.tv/greyok__
- YouTube: https://www.youtube.com/@GreyOK_0
- Discord: https://discord.gg/TJcr6ZxJ

## Notes

- The app is optimized around live cohost conversation, not a command-heavy bot workflow.
- Web search exists in the codebase but is not the primary live workflow yet.
- The release page is the source of truth for packaged builds.
