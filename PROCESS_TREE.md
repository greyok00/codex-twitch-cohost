# Process Tree

## Target Runtime Model

```text
single packaged app
|
+-- cohostd daemon
|    |
|    +-- session orchestrator
|    +-- config loader
|    +-- memory manager
|    +-- scene manager
|    +-- module supervisor
|         |
|         +-- llm worker         (spawn on demand, exit on completion)
|         +-- tts worker         (spawn on demand, exit on completion)
|         +-- stt worker         (spawn on demand, exit on completion)
|         +-- smoke worker       (spawn on demand, exit on completion)
|
+-- owner UI shell
|    |
|    +-- control-only view
|    +-- process lights
|    +-- session monitor
|
+-- utility UI shell
     |
     +-- backend health view
     +-- start/stop/recover tools
     +-- process diagnostics
```

## Failure Model

```text
If owner UI hangs:
  kill owner UI
  backend stays alive
  utility UI can still inspect runtime
  owner UI can relaunch and reconnect

If utility UI hangs:
  kill utility UI
  backend stays alive
  owner UI remains usable

If one worker hangs:
  supervisor kills worker only
  restart worker if policy allows
  backend stays alive
  UI stays alive

If backend dies:
  UIs go yellow/red
  start/recover action relaunches daemon
```

## Health Lights

```text
green  -> healthy and current
yellow -> degraded, restarting, waiting, or fallback mode
red    -> failed, unavailable, or disconnected
```

## Command Examples

```text
cohostd daemon
cohostd call status
cohostd worker llm ...
cohostd worker tts ...
cohostd worker stt-file ...
cohostd supervise llm ...
cohostd supervise tts ...
```
