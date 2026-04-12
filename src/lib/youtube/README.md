# YouTube Co-Host Mode

This module provides the YouTube co-host subsystem.

- `PlayerController`: wraps YouTube IFrame API load/play/pause/seek/tick.
- `TranscriptContextService`: builds local context windows + topic/tone/pause scores.
- `CommentScheduler`: computes weighted `commentScore` and skip/fire decisions.
- `RemarkGenerator`: requests strict JSON remarks from the backend LLM command.
- `TTSPlaybackQueue`: pause -> synthesize -> speak -> resume flow with cancellation.
- `SessionStateStore`: central runtime/debug state store for UI panels.
- `YoutubeCohostSession`: orchestrates state machine and service interaction.
- `TranscriptSourceService`: provider transcript -> user file -> metadata fallback with transcript quality scoring.
- `YouTubeTimedTextProvider`: real provider path for caption retrieval.

## Playback / Context Rules

- The player uses **pause/resume**, not stop/reload.
- Scheduler evaluation runs every second while playback is active.
- Seek events invalidate stale context and suppress immediate callbacks.
- Transcript coverage directly influences interruption frequency.
- Playlist continuity carries short topic memory forward while suppressing repeated callback structures.
- Developer mode exposes the live transcript window, score components, and fire/skip reason.

## Transcript Sources

Resolution order:

1. user-uploaded transcript file
2. provider captions from `YouTubeTimedTextProvider`
3. metadata fallback from title/description

Each resolve returns:

- transcript mode
- provider name
- quality band
- coverage score
- user-facing status message

## Test Commands

- `npm run test:youtube` for YouTube module tests.
- `npm run test:harness` for the broader mocked harness suite.
- `npm run lint` for Svelte/TypeScript checks.
- `npm run build` for production build validation.
