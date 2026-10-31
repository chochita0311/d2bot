# RUN-20261023-03: Rust 독립 action과 방 lifecycle

## Metadata
- ID: `run-20261023-03`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0023](../feature/feat-0023-rust-actions-lifecycle.md); Spec: [SPEC-0022](../spec/spec-0022-rust-actions-lifecycle.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Created/Updated: `2026-10-23`.

## Objective And Boundary
ORB's input-free foundation passed under [EVAL-0021](../evaluation/eval-0021-rust-background-motion.md). This sole builder loop completed ordered loot/gem recognition, owned sessions, town/menu vision and planning, room/town effects and evidence. [EVAL-0022](../evaluation/eval-0022-rust-actions-lifecycle.md) passes that bounded contract. JPEG's measured exception was approved and its complete operation packet passed separately. No game input or focus change was authorized by this run; existing human recovery confirmation remains pending.

## Source Findings
Loot scanners return the first passing configured candidate, including when a later candidate scores higher. Fixed items precede eligible farm templates and their dedup markers differ. Conditional loot reuses only scan_frame, while GUI sessions own independent pickup workers. The old try_pickup public helper reports a click as picked_up; porting must not invent actual item acquisition proof.

Gem atlas entries and count votes preserve insertion/tie order. Three weighted count samples can retain tracked counts on missing icon evidence or a resync delta>6. Planning propagates expected upgrades column-first and skips blocked slots. A transmute is successful only after one occupied cube cell and next-level icon checks; otherwise block/rescan/restash. Existing gem tests cover assets/crops/schema, with little worker-trace coverage, so new fake traces must freeze current behavior explicitly.

Both old standalone gem/loot workers omit explicit capture.close and may report stopped after a join timeout while their thread survives. Safe Rust ownership will close owned capture and preserve a stopping/alive worker state, as required by the user's resource-safety/overlap instruction. This resource correction is scoped here; it does not accept the separately proposed Stop-key policy change.

## Work Plan And Evidence
Input-free recognition/planning→deterministic fake session traces→town/lifecycle wiring→Contract/Functional. Primary agent owns architecture, code and final review; named scouts supply bounded read-only source/test evidence.

Implemented the ordered/cached loot scanner and complete standalone loot loop; gem atlas/OCR/plan/consensus/result recognition and standalone loop; injected effects/time/random backend with explicit installed PyDirectInput pauses; bounded diagnostic events; intent-before-input and retryable cleanup on error/panic. Normal success and uncertain gem results compare input/wait/event traces with the unchanged Python worker; the Rust-owned capture additionally closes. Unicode 11 casefold data moved unchanged from the platform title helper to common for both title and loot matching; the generator follows the new owner.

First-slice packet: action recognition 5, action runtime 7, gem library 2, motion library 2 and Windows I/O 12 passed (one default native test ignored). Separate Black/in-memory compile for the action oracle and renamed casefold generator passed. All-target Clippy passed before adding the town planner. No native ignored tests or live actions ran; no complete-feature evaluation exists.

Added a typed town planner preserving source phases/transition-before-dispatch and current positive-evidence requirements. Six development-oracle tests compare full action/error/state traces for room round trip, difficulty preparation, release/camera/NPC/probe bounds, stalled waypoint detours, Arcane map inspection/reacquisition/native proof, arrival-only and strict deadlines. JSON integer/float spellings compare by equal numerical value; branch/action/state values remain exact. Town images, explicit-fx/Q8 processing, current-frame hub injection, native-safe action adapter, owned launchers, room/town loop and evidence writer now have their scoped comparisons. Calibration/module CLI tools are retained for FEAT-0027; the injected Arcane hub algorithm and main integration belong to FEAT-0024.

Final frozen packet passed fmt/locked all-target check/Clippy,125 full Rust tests (4 native ignored),29 separate release action/session/town tests, locked release build and separate oracle/generator Black/in-memory compile. All102 frozen code/manifest/test/fixture hashes matched. Cargo tmp fixture directories were empty after the packet. Primary Contract/Functional evaluation and detailed mappings/limits are in EVAL-0022. No source runtime Python, config JSON or assets changed.

## Human Review Outcome
Proceed under the existing full-port delegation. Do not silently waive JPEG comparisons, substitute acquisition/arrival success or treat partial porting as a usable full app.
