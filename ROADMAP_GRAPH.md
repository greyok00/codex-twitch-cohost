# Roadmap Graph

## Release Path

```text
0.4.0  Detached Runtime Foundation
  |
  +-- cohostd daemon exists
  +-- worker/supervisor subprocess model exists
  +-- utility frontend exists
  +-- owner shell reads backend health
  +-- headless voice smoke passes
  |
  v
0.5.0  Thin Client Transition
  |
  +-- main frontend stops owning runtime orchestration
  +-- backend socket API becomes stable
  +-- child-process health lights fully wired
  +-- UI kill/relaunch does not affect active backend session
  |
  v
0.6.0  Voice Reliability Pass
  |
  +-- browser STT primary path stable
  +-- self-hearing suppression stable
  +-- TTS interruption rules deterministic
  +-- voice latency instrumentation complete
  |
  v
0.7.0  Character / Memory System
  |
  +-- real character packages
  +-- pinned and heuristic memory stable
  +-- story/continuity retrieval stable
  +-- avatar state unified under backend config
  |
  v
0.8.0  Public Call Runtime
  |
  +-- lightweight public call page
  +-- shared backend voice session
  +-- call link management stable
  +-- owner and public routes coexist
  |
  v
0.9.0  Scene System
  |
  +-- solo mode stable
  +-- dual-character debate stable
  +-- topic mode stable
  +-- turn orchestration owned by backend
  |
  v
0.9.2  Character Renderer
  |
  +-- nested character stage is primary desktop runtime
  +-- detached popout is optional
  +-- rig settings live in unified backend config
  +-- DOM/CSS rig is replaced or augmented by GPU-backed deformation
  +-- WebGPU / wgpu renderer path exists for lighting, mesh warp, and higher realism
  |
  v
0.9.4  External Live Context And Operator UX
  |
  +-- backend-owned external live context pipeline exists
  +-- web search ingestion exists with strict recency tagging
  +-- Twitch EventSub / IRC events feed operator widgets
  +-- operator command palette exists
  +-- stats / notifications / timeline widgets are driven by real backend signals
  |
  v
0.9.6  Screen Vision Observer
  |
  +-- separate screen-observer subprocess exists
  +-- sampled frame analysis is used instead of continuous full-frame inference
  +-- structured JSON observation events feed scene and memory layers
  +-- local vision model path exists for qwen2.5vl, gemma3, or llama3.2-vision
  +-- screen observations never post raw captions directly into chat
  |
  v
1.0.0  Stable
  |
  +-- backend-first runtime
  +-- thin clients only
  +-- local and Twitch modes reliable
  +-- no self-looping
  +-- voice path reliable across long sessions
  +-- diagnostics/self-test authoritative
  +-- release builds match dev behavior
```

## Gate Conditions For 1.0.0

```text
Voice          -> final
Memory         -> final
Backend daemon -> final
Thin client UI -> final
Public call    -> final
Scene runtime  -> final
Diagnostics    -> final
Packaging      -> final
Character FX   -> final
```

## Deferred Product Tracks

```text
External Live Context
  -> backend-only ingestion
  -> strict timestamps and source tagging
  -> can feed weather, web search, stream summaries, Twitch events, and sampled screen observations
  -> never pushes raw external data straight into chat UI

Screen Vision Observer
  -> dedicated subprocess, never UI-thread vision inference
  -> target models: qwen2.5vl, gemma3, llama3.2-vision
  -> default strategy: sampled screenshots, not full video flooding
  -> active scene emits structured JSON observation events with timestamps and confidence
  -> scene and memory consume summaries, not raw captions

Operator Widgets
  -> Stats Widgets:
     viewers, followers, subs, bits, chat rate, mention rate,
     STT latency, TTS latency, reply latency, restart counts
  -> Notification:
     follows, subs, raids, redeems, mentions, backend failures, restarts
  -> Timeline:
     stream events, backend restarts, scene/topic changes, major session events
  -> Command Palette:
     connect Twitch, switch character, toggle posting, open avatar popup,
     restart module, clear memory, run self-test
  -> Gauge:
     chat activity, reply intensity, backend load

Future Optional Widgets
  -> Weather Widgets
  -> Clock Widgets
  -> Stocks Widgets
  -> Dock
  -> Morph Card
  -> Ripple
```
