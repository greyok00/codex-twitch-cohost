# Architecture

## High-Level Graph

```text
                                  +------------------------+
                                  |    Main Frontend UI    |
                                  |  Svelte 5 + Flowbite   |
                                  |   owner control shell  |
                                  +-----------+------------+
                                              |
                                              | tauri invoke/events
                                              v
+-----------------------+          +----------+-----------+          +----------------------+
| Utility Frontend UI   | <------> |    Native App Host   | <------> |     config.json      |
| backend-only console  |          |   tauri command API  |          | unified app config   |
| health / recovery     |          | thin client bridge   |          +----------------------+
+-----------+-----------+          +----------+-----------+
            |                                 |
            | backend status / start          | shared library calls
            v                                 v
                        +---------------------+----------------------+
                        |                   cohostd                  |
                        |          detached CLI / daemon runtime     |
                        |          authoritative backend core        |
                        +---------------------+----------------------+
                                              |
                       +----------------------+----------------------+
                       |                                             |
                       | JSON control plane                          |
                       | local socket / RPC                          |
                       v                                             v
             +---------+----------+                        +---------+----------+
             | session orchestrator|                        | module supervisor  |
             | memory / prompt /   |                        | launch / retry /   |
             | scene / runtime     |                        | kill child jobs    |
             +---------+----------+                        +---------+----------+
                       |                                             |
                       | submit work                                 |
                       v                                             v
      +----------------+------------------+       +------------------+-----------------+
      |                                   |       |                                    |
      |         worker subprocesses        |       |         external providers         |
      |    isolated one-job child tasks    |       |                                    |
      |                                   |       |                                    |
      |  cohostd worker llm               |------>| Ollama / cloud providers           |
      |  cohostd worker tts               |------>| edge-tts / browser TTS             |
      |  cohostd worker stt-file          |------>| browser speech / local fallback    |
      |  cohostd worker voice-smoke       |       | verification tools                 |
      +-----------------------------------+       +------------------------------------+
```

## Responsibility Boundaries

### Frontend
- Render state.
- Dispatch commands.
- Show health lights and diagnostics.
- Never own durable runtime state.

### Native App Host
- Bridge between UI and backend runtime.
- Expose commands/events to the frontend.
- Start backend if needed.
- Stay thin.

### `cohostd`
- Own runtime orchestration.
- Own config loading.
- Own memory access.
- Own scene and session state.
- Supervise module workers.

### Worker Subprocesses
- Run one bounded job.
- Emit structured JSON result.
- Exit immediately.
- Can be killed/restarted independently.

## Data Ownership

```text
config.json          -> authoritative settings/rules
memory store/jsonl   -> long-lived memory and transcript history
secret store         -> oauth tokens / API keys
temp runtime files   -> transient debug/smoke/audio artifacts
```

## Stability Rules

- Frontend must never be the runtime source of truth.
- UI can die and restart without losing backend state.
- High-frequency speech updates must not rerender the whole app.
- Expensive tasks must run off the UI thread or as child processes.
- Every worker task returns a structured payload, not ad hoc strings.
