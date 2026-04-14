# Stability Audit

This file is the current reality check. It is not a feature wish list.
Each section records:
- accomplished
- current risk
- what blocks FINAL status
- what can be automated

## 1. Config and state authority

Accomplished:
- Core behavior settings are unified under backend config.
- Legacy personality file usage was reduced to migration-only behavior.
- Scene and character-studio settings are backend-backed instead of browser-only.

Current risk:
- Avatar runtime still uses browser localStorage for rig state and image cache.
- Some frontend runtime flags still exist outside config by design.

Blocks FINAL:
- move remaining avatar runtime state into backend config or explicitly classify it as non-authoritative cache
- verify no stale local UI state can override backend behavior after restart

Automation:
- Rust config roundtrip tests: yes
- startup migration checks: yes
- frontend/backend drift detection: partial

## 2. Voice session orchestration

Accomplished:
- Shared voice session controller exists.
- Browser speech is the primary engine.
- Voice session store tracks status, timing, and engine.

Current risk:
- session can still drift into bad runtime states under long conversation churn
- replying/speaking suppression interactions are still fragile

Blocks FINAL:
- explicit state machine tests for start -> listen -> process -> reply -> recover
- forced recovery behavior for every failure path

Automation:
- unit-level orchestration checks: partial
- full session loop headless: partial

## 3. Browser STT primary path

Accomplished:
- Browser speech engine exists and supports interim/final handling.
- Weak transcript and ambient filters exist.
- restart-on-end behavior exists.

Current risk:
- browser STT quality and behavior vary by environment
- self-hearing may still happen from echo tail or OS routing
- browser speech is difficult to fully trust in headless automation

Blocks FINAL:
- stronger echo suppression rules
- explicit “do not commit if near recent bot reply audio window” logic verified in practice
- better visibility into why an interim/final was accepted or dropped

Automation:
- support detection: yes
- full browser recognition quality: no
- manual device validation still required

## 4. Local STT fallback

Accomplished:
- Vosk fallback exists.
- larger Vosk model is present.
- helper script path exists.

Current risk:
- Vosk accuracy is still well below browser speech for conversational use
- fallback may be acceptable for resilience but not for primary UX

Blocks FINAL:
- headless STT smoke script
- clear acceptance threshold for fallback quality
- make fallback clearly secondary in diagnostics and UX

Automation:
- yes, local fallback can be smoke-tested without launching the app

## 5. TTS output

Accomplished:
- edge-tts integration exists
- browser fallback exists
- speech normalization exists
- reusable native TTS module exists under the backend, not only behind Tauri commands
- headless `cohostd` voice smoke passed 3 consecutive runs through the backend-first path

Current risk:
- TTS can still end up in a bad state where chat continues but audio feels dead
- TTS timing and interruption behavior need harder validation

Blocks FINAL:
- repeat/interruption soak testing
- explicit failure recovery when synthesis fails
- packaged executable verification for temp paths and child-process launch

Automation:
- yes, TTS synthesis can be tested headlessly
- current result: passing on the detached `cohostd` voice smoke path, but still not FINAL

## 6. Self-hearing and ambient suppression

Accomplished:
- ambient-noise phrase filters exist
- bot-origin Twitch messages are ignored in backend processing
- TTS suppression window exists

Current risk:
- actual self-hearing can still happen through browser speech + system audio path
- suppression windows may be too short or badly timed
- local app events may still create loop-like behavior indirectly

Blocks FINAL:
- extend and validate suppression timing
- instrument dropped transcript reasons
- prove no bot self-loop in repeated real sessions

Automation:
- partial
- real echo conditions still need manual validation

## 7. Chat and reply engine

Accomplished:
- local and Twitch input paths are separated
- bot-post-to-Twitch toggle exists
- keep-talking mode exists
- anti-repeat logic exists

Current risk:
- user reports that controls sometimes do not affect behavior
- auto comments may still not align with configured state
- reply style may still drift into repetitive filler

Blocks FINAL:
- control-to-runtime verification
- cadence/auto-comment deterministic tests
- stronger output normalization

Automation:
- harness tests exist but are not enough yet

## 8. Memory and continuity

Accomplished:
- structured fact extraction exists
- pinned memory exists
- story_state and relationship-style memory exist

Current risk:
- memory may still behave like a decorated log instead of a reliable fact system
- retrieval relevance is not fully proven
- correction precedence still needs deliberate testing

Blocks FINAL:
- contradiction tests
- long-session relevance tests
- stricter memory ranking and eviction policy review

Automation:
- partial
- needs dedicated memory regression tests

## 9. Character system

Accomplished:
- character cards replaced the old dropdown
- presets map to voices
- slider-based tuning exists

Current risk:
- not all presets may produce distinct or stable behavior
- voice mapping is not the same as true persona coherence

Blocks FINAL:
- preset identity validation
- default-voice consistency checks
- user-editable duplication/versioning of presets

Automation:
- persistence checks: yes
- subjective persona quality: no

## 10. Avatar system

Accomplished:
- shared avatar runtime now exists
- embedded and detached avatar use the same code path
- jar styling exists

Current risk:
- transition from popup-first to embedded-first is still in flight
- some detached-window logic may still be hanging around in the session layer
- lip sync timing quality is still perceptual, not verified

Blocks FINAL:
- remove any remaining popup-first assumptions
- make embedded avatar the default live path
- verify no duplicate controls remain

Automation:
- render/build checks: yes
- perceptual animation quality: limited

## 11. Twitch integration

Accomplished:
- auth separation logic exists
- self-test checks role separation
- IRC and EventSub health checks exist

Current risk:
- real-world account routing bugs have happened already
- dev vs packaged behavior has diverged before

Blocks FINAL:
- account-role integration tests
- reconnect loop tests
- packaged-build auth verification

Automation:
- partial
- live service validation still required

## 12. Diagnostics and self-test

Accomplished:
- self-test exists
- service health exists
- voice runtime report exists

Current risk:
- some diagnostics have historically reported misleading warnings
- diagnostics are only as good as the checks behind them

Blocks FINAL:
- diagnostics must be proven to reflect reality
- headless voice smoke needs to exist outside the UI

Automation:
- yes, most of this can be automated

## 13. Headless backend and control plane

Accomplished:
- a real `cohostd` binary now exists
- the detached runtime can run prompt, TTS, STT-file, pinned-memory, and voice-smoke operations without opening the UI
- `cohostd worker ...` provides isolated child-process module execution
- `cohostd supervise ...` can relaunch failed or hung module jobs without taking down the parent shell
- headless voice smoke now targets the backend directly instead of a side script path

Current risk:
- the frontend is still not just a thin client; it still owns too much runtime logic
- module supervision is one-shot job supervision, not a full persistent RPC service yet
- UI process-health lights are not wired to the detached runtime yet
- Twitch runtime is not detached into the `cohostd` control plane yet

Blocks FINAL:
- move frontend control paths onto backend commands/status rather than browser-owned orchestration
- expose child-process health and restart state in a stable backend API
- prove backend survives UI kill/relaunch with the same live session state

Automation:
- backend boot: yes
- backend prompt path: yes
- backend voice smoke: yes
- persistent controller/client reconnect behavior: not yet
- the Tauri window is still effectively the runtime shell
- there is no standalone daemon/CLI mode yet
- frontend and backend are still coupled at process-launch level

Blocks FINAL:
- add a true headless runtime binary or mode
- expose a clean local control/status interface
- prove the runtime survives UI restart or absence

Automation:
- partial
- full backend-detached validation does not exist yet

## 14. Performance and anti-freeze

Accomplished:
- worker-backed transcript/frame processing exists
- capability detection exists
- config/state unification reduced some cross-store churn

Current risk:
- long-session stability has not been proven
- browser STT + TTS + UI updates may still contend badly
- too many open code paths and legacy features still add drag

Blocks FINAL:
- sustained-session profiling
- memory growth measurement
- UI rerender profiling
- eliminate dead or low-value code paths

Automation:
- build/test: yes
- long interactive soak: partial

## 15. Public call link

Accomplished:
- public route and token/config path exist

Current risk:
- not yet a true production-ready public voice endpoint

Blocks FINAL:
- hosted/reachable deployment path
- public session hardening

Automation:
- route behavior: yes
- real public internet access: no

## Summary

Already genuinely advanced:
- config consolidation
- shared avatar runtime architecture
- browser-speech-first architecture
- worker-backed transcript/frame processing
- self-test/service-health foundations

Not FINAL yet:
- browser STT reliability
- TTS headless smoke reliability
- anti-self-hearing behavior
- control reliability
- long-session stability
- memory correctness
- packaged-build parity
- detached backend runtime

Rule going forward:
- no chunk is marked FINAL until its current-risk list is empty or explicitly accepted as non-blocking
