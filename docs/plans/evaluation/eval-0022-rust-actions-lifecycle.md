# EVAL-0022: Rust 독립 액션과 방 실행

## Metadata
- Run: [RUN-20261023-03](../run/run-20261023-03-rust-actions-lifecycle.md); Attempt: `1`.
- Feature: [FEAT-0023](../feature/feat-0023-rust-actions-lifecycle.md); Spec: [SPEC-0022](../spec/spec-0022-rust-actions-lifecycle.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Contract
- Result: PASS; Evidence Coverage: complete for the independent action/session/lifecycle comparison set; Acceptance impact: no remaining blocker within SPEC-0022. Whole-port parity remains Unknown/Provisional.
- Gem recognition owns unchanged atlas/35 icons and exact variant/bank/glyph order; column-first projected planning, blocked slots, weighted three-sample counts, tracked fallback/delta>6, cube occupancy and next-level icon thresholds preserve source selection. Loot preserves fixed/farm candidate order, their distinct dedup markers, Unicode11 casefold, first passing candidate and ordered failure behavior across three joined scan workers. Cached images release on config update; no frame history is retained.
- ActionContext is the sole owner of held-key/button intent, stop binding and capture lifetime. It records intent before sending, retries failed release, permits release after stop/focus loss, prevents new work after closing, reports primary/cleanup failures and continues other cleanup even after a cleanup panic. Native desktop primitives reuse the safe platform boundary without field-controller timing. NumLock arrow release attempts every payload even if a prior event fails; send counts and mapping failures remain errors.
- ActionSession connects the four action launchers to one owned worker and bounded diagnostic events. A live worker remains running/stopping after timeout, rejects overlap and can be joined again; completion/failure/panic remains observable. This deliberately repairs old gem/loot capture leaks and premature stopped state under the user's resource-safety instruction. Log capacity256 reports omitted older events. No Stop-hotkey behavior correction is included pending its separate decision.
- Town planning preserves phase advancement before external dispatch and requires later positive evidence rather than an invented acknowledgement. Current geometry/identity/focus, pointer readback without resend, hover/stop race and held mouse cleanup remain separate guards. Source 15-minute/phase/camera/probe/NPC/approach/map-restoration limits, native proof, difficulty preparation vs completed runs, arrival-only, return and exit confirmation retain their predicates and outcomes. Saved state preserves known fields, explicit null camera readings and unknown extension fields.
- Town/menu recognition retains template/width-scale order, offset/blue checks, unique paired-brazier evidence and title required-part checks. Whole-matrix Q8 Gaussian kernels and explicit-fx label resizing compare exact pixels; attribution/licenses are retained. The optional native hub callback must consume the current native frame; its Arcane algorithm is FEAT-0024 work. Positive title recognition here does not claim the full common automap occlusion/toggle runtime.
- Room entry/exit preserves actual menu/loading/absence waits, reference coordinates, jitter, click/key pauses and events. Evidence writes frames, ordered JSONL events, planner/result/geometry and timing files. Stop/capture failure compare decoded frame hashes and JSON outputs; a result-file write failure remains an error while timings, stop diagnostics and owned cleanup continue. Native OS-clock random sampling is nondeterministic; deterministic traces inject the same selected random values.

## Source Mapping
| Existing producer/consumer | Rust owner and checked boundary |
| --- | --- |
| gem constructor/private recognition/scan_counts/worker and GUI start/stop | `actions/gem_summing{,/recognition,/execution}`; `actions/session{,/launch}`; atlas/count/policy/error and worker traces |
| loot scanner/try_pickup/worker, conditional-loot scan_frame and GUI config/start/stop | `actions/loot_pickup`; shared session/context; ordered candidates/first hit/duplicate/cache/borrowed-capture boundary |
| lifecycle create_room/exit_room/menu and template waits | `actions/run_lifecycle`; injected current-frame recognition; create→difficulty→loading→exit trace |
| town vision/planner, lifecycle guarded dispatch and evidence | `actions/town_loop/{vision,title,planner,state}`; `actions/run_lifecycle/town`; image/decision/action/state/file comparisons |
| action input/capture/hotkey/time/random effects | `actions/runtime`, `actions/desktop`; common capture/controller/Unicode/workers; fake OS effects and cleanup |

The Python town calibrate/calibrate_scene writers and module CLI bridge are retained tool interfaces assigned to FEAT-0027; no runtime caller was found. GUI controls and current Summoner callers remain later integration work. Changing Python class method signatures into typed Rust functions/session launchers is a language/ownership change, not a claim that GUI wiring is already present.

## Functional
- Result: PASS; Evidence Coverage: complete for the specified deterministic/fake and fixed-image packet. fmt, locked all-target check, warnings-denied all-target Clippy, unfiltered all-target tests and locked release build passed.
- Full packet:125 tests passed,0 failed,4 explicitly native tests ignored across18 targets. This includes all15 vision-operation tests under the approved JPEG exception and the lossless asset audit. No filtered JPEG tests are used for this result.
- Separate release packet:29 passed across action recognition5, action runtime12, action session2, town planner6 and town vision4. It covers gem success/bad cube occupancy/bad result; loot quiet deadline and priority; installed100ms decorated PDI pauses; partial key/cleanup failure; bounded event backlog; room create/exit trace; no-resend pointer timeout; release-after-stop/focus and stop-during-hover; stopped/capture-failed evidence, failed result write and launched-worker resource cleanup.
- Planner comparisons include full round trip/current arrival and menus, Nightmare/Hell preparation, bounded movement/camera/NPC/probes, stalled left/right waypoint detours, hidden Arcane map/native reacquisition/original deadline, arrival-only and strict deadline boundaries. Image comparisons include reduced Korean font width, deleted title thirds/required parts, wrong regions, flat/multiple blue pairs and real consecutive native PNG camera motion/reseeding/stability.
- All102 frozen manifest/lock/source/test/support/oracle SHA256 values matched before/after. Separate Black checks and in-memory compilation of action-oracle.py and the renamed Unicode generator passed. Cargo's incremental hard-link fallback warning only changed cache copying; every command exited0. The full vision-operation target took81.74s on this host; no performance acceptance claim follows.

## Limits And Cleanup
No physical key/mouse input, focus change, game session, native ignored test, rendered GUI, full Summoner execution, long-session performance or distribution acceptance ran. The executable remains bootstrap/diagnostic until later features wire the app. Existing Python runtime/config JSON/assets remain unchanged. The user-approved JPEG exception is owned by SPEC-0018; normal capture/PNG and detector thresholds are unchanged. Stop-key correction and the pre-existing human game-recovery gate remain separate.

Test-owned comparison directories self-clean after writers/oracles exit; the checked Cargo tmp directory contains no remaining fixture directories. Shared Cargo dependencies/build output remain for the continuing port. No durable output depends on temporary oracle media.

## Route
Pass FEAT-0023 in this input-free/fake-input boundary; proceed to FEAT-0024's current Summoner runtime and remaining observers. Keep CLI/calibration tools, GUI, recording, distribution and live acceptance with their owning later features.
