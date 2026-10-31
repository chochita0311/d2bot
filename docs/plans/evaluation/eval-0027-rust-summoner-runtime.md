# EVAL-0027: Rust Summoner 실행 흐름

## Metadata
- Run: [RUN-20261023-04](../run/run-20261023-04-rust-summoner-runtime.md); Attempt: `1`.
- Feature: [FEAT-0024](../feature/feat-0024-rust-summoner-runtime.md); Spec: [SPEC-0023](../spec/spec-0023-rust-summoner-runtime.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Date: `2026-10-23`.
- Primary agent owns the semantic judgment. Named read-only scouts supplied bounded call/test evidence; the bounded verifier supplied command results without completion authority.

## Contract
- Result: PASS; Evidence Coverage: complete for SPEC-0023's stored-image, pure-state, injected-effect and partial-initialization boundary. No remaining implementation blocker was found within that boundary. Whole-port parity remains Unknown/Provisional.
- Common observers retain native layout/ROI, threshold arithmetic and tie ordering, unknown/partial observations, immutable frame identity, original acquisition times and declared manifest/hash requirements. Run-specific scene/ground/camera, path/steering/graph and terminal/objective/appearance/life evidence retain their distinct current-proof and expiry rules. An observation or dispatched action does not establish movement, death, pickup or room closure by itself.
- Preparation, buff requests/renewal, emergency one-potion recovery, conditional labels/loot, continuous navigation, paused defense, encounter and Canyon/Act1 finishing preserve source order, guard/deadline values, report evidence and confirmed-versus-requested outcomes. Input sessions retain failed cleanup owners, independently release held input, refuse overlapping work and report timeout/panic. Mutable state has one owner; shared immutable assets do not create frame histories or locks over vision, OS calls, waits or joins.
- Main preserves validation-before-effects, selected character/survival/casting prerequisites, dry behavior, source-timed partial initialization and pending-pause report snapshots. The concrete owner composes the tested adapters and retries every cleanup owner before image writes. Its dry and pre-stopped initialization tests construct no Windows capture, listener or focus effects. Repeated rooms use fresh owners and the same external cancellation/clock; an initial recovery room does not count as a newly completed room.
- Independent search/search runtime/cycle remain separate public contracts. Search retains acknowledgements, motion/landing, survival preemption and ordered return proof; shared runtime uses the single field actor and current geometry/ground proof. Cycle retains positive phase confirmations, per-room/global timeouts, global command/frame/generation history, partial-key refusal and one review result without restarting. Thirty-room synthetic comparison creates no room31.
- GUI-session facade forwards the existing automatic continuous north options, distinguishes completed/incomplete/stopped/failed and retains worker ownership through actual exit. Stop/close requests cancellation without joining on the GUI thread; explicit shutdown verifies exit. Public metadata/profile helpers and compatibility re-exports remain available. Planned routes are immutable metadata; they are not new executors.
- Narrow implementation adaptations are explicit: typed enums/integers reject unrepresentable dynamic values at parsers; metadata owner paths point to Rust files; consolidated route records and terrain defaults preserve their consumers; source unbounded session logs become a256-event buffer with an omission warning under the user's memory/resource-safety requirement. Previously approved optional-pause repair/JPEG bounds remain unchanged. Existing application hotkeys, including their game-shortcut conflict and extra-held-key limitation, are preserved.

## Source Mapping
| Existing producer/consumer | Rust owner and evidence |
| --- | --- |
| common HUD, labels, automap, weapon/buff, scene motion and encounter observations | `common/{survival_vision,label_occlusion,automap_vision,buffing_vision,scene_motion,encounter_vision,encounters}`; actual image/intermediate/state comparisons |
| Summoner scene, ground, camera, terrain, terminal, objective, journal and life | `runs/summoner/{scene_vision,camera_ground,terrain_patterns,terminal_vision,objective_vision,journal_vision,appearance_vision,life_vision}`; stored native positives/negatives, synthetic boundary and tracking sequences |
| navigation/path/steering/route graph/continuation and dispatch | matching `runs/summoner` modules plus `navigation_execution`; grid/feature/state/call traces, original threshold and ordering checks |
| common preparation/recovery/conditional loot and held execution | `common/{field_preparation,emergency_recovery,conditional_loot,field_control,field_runtime,workers}`; reports/evidence and fake backend/worker faults |
| current continuous/legacy/encounter/paused/renewal/Canyon/room finish | matching `runs/summoner` modules and `main_run/native` adapters; actual observers with injected effects, deterministic procedure traces |
| main run_once/run/repeated-room and GUI-session/public definition | `main_run/{engine,cycles,entry,output,native}`, `options`, `gui_session`, `definition`, `runs/base` and re-exports;46 engine cases, concrete dry/partial owner checks,22 session and5 definition cases |
| retained search/run_search/search runtime and cycle | `search`, `search_runtime`, `cycle`;37 search state,14 search driver,20 shared runtime and36 cycle cases plus ownership/cleanup checks |

Detailed numerical tolerances, original data sources, individual checkpoint limitations and corrected fixture failures remain in RUN-20261023-04. The source audit found no other retained Summoner execution interface requiring a new executor. The removed legacy north executor stays refused; the six east/south/west metadata modules are consolidated without claiming live route acceptance.

## Functional And Regression Evidence
- Frozen Windows packet: fmt, locked all-target check, release all-target tests and release build exited0. All252 tests passed,0 failed,4 native tests ignored across40 targets:55 library and197 integration tests; the binary target had0 tests. Clippy passed the same source immediately before this packet and was not repeated.
- Separate Black checks and in-memory compilation for the two new cycle/session fixtures passed. All957 captured source/test/Python/config/asset/Cargo input hashes matched before/after; Git status was unchanged. Cargo's incremental hard-link fallback used file copies and produced no code diagnostic. `target/tmp` was empty.
- Source comparisons use the fixed project Python3.7.9/NumPy1.21.6/OpenCV4.13.0 baseline as a development oracle only. Production Rust uses no Python/OpenCV subprocess. Decisions, reasons, order, state, reports and fake input traces are compared in their recorded scopes; normal PNG/capture detector thresholds were not relaxed.

## Evidence Gaps And Acceptance Impact
- Physical game capture/focus/input, full native gameplay/startup, real death/pickup/room success and long-session performance were not observed. Non-blocking for SPEC-0023's explicitly input-free/fake-input boundary; they remain required only in their owning later Windows distribution or human gameplay boundaries. The existing recovery gate remains active.
- GUI controls/rendering, recording codecs, full CLI/replay/calibration commands and Python/OpenCV-free distribution remain FEAT-0025–0028. Blocking for whole-port completion. The linked EXE still exposes bootstrap entry commands until later wiring; this evaluation does not call it a usable full application.
- No full performance/parity or final human transition acceptance is inferred from these tests. No user-facing design/interaction rule candidate was produced by this backend-only run.

## Route And Cleanup
Pass FEAT-0024's scoped automated boundary and continue under the user's existing full-port delegation. Preserve the separate human acceptance boundaries. Recording's external codec choice is awaiting the owner; independent interface preparation can continue.

Test-created temporary comparison artifacts self-cleaned after their processes exited; no durable output depends on them. Shared Cargo build/dependency caches remain for the continuing work. Existing Python runtime, config JSON and runtime assets were not changed.
