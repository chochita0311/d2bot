# SPEC-0024: 기존 제어판의 Rust 이전

## Metadata
- ID: `spec-0024`; Status: `approved`; Run ID: `run-20261023-05`; Attempt: `1`.
- Feature: [FEAT-0025](../feature/feat-0025-rust-desktop-control.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface/Profile: `frontend` / `frontend-product`; Evaluators: Contract, Functional, Design, UX; Date: `2026-10-23`.

## Authority And Dependencies
The user delegated the complete port through a usable EXE. FEAT-0024 passed its explicitly fake-input/stored-image boundary in EVAL-0027. This is the sole active builder. FEAT-0026 remains a dependency for final recording integration, not for independent panel/model/action-session work. The user subsequently selected direct Rust MPEG-4, whose stored/fake boundary passed EVAL-0028; use that actual recording owner without adding a codec runtime. GUI framework implementation is a delegated technical choice, subject to actual rendering/interaction validation and later human transition review. No game capture/focus/input is authorized here.

## Source Contract
Use `diablo2/ui/gui.py`, `tests/test_gui_summoner.py`, `docs/project/gui-maintenance.md`, config and the passed action/Summoner sessions. Preserve1120x820 startup/min1020x800 **physical pixels** (Tk geometry, not egui logical points), title, expanding log and naturally sized right controls, header/window/backend choices, recording/snapshot/refresh row, all four game actions, blank repeat distinctions, three Korean difficulty labels, selected character display/key mapping, town-loop choice and footer.

Window selection preserves exact casefolded helper exclusion, current exact/substring, preferred Diablo-pattern order and source fallbacks. Ordinary refresh changes selection only when blank; startup may resolve the existing value. Duplicate character display names retain last-key lookup while choices retain insertion order. Runtime config changes remain in memory; no config file rewrite. Preserve per-action validation/messages and no active-worker overlap/config change. Recording remains independent of game-action exclusivity.

Stop Action/F10 requests cancellation. Buttons/selectors remain disabled until the actual owned worker exits. Preserve source poll order (recording, lifecycle, gem, loot, Summoner), unread event/status precedence, pending Stopping, durable Error/Incomplete/Summoner complete/Stopped and recording/Idle fallback. Queued logs never access UI from workers. Keep visible-line trimming and its notice. Stop/close must not wait on capture/encoder/vision/input joins on the UI thread. Close is idempotent, rejects new work, requests all stops and closes only after all owned workers/cleanup complete; cleanup faults remain visible and retryable.

Snapshot retains `assets/private/gui_snapshot.png`, native geometry without reference resize, explicit capture close and visible save/error outcome. Final recording controls use the actual FEAT-0026 implementation. No placeholder control establishes acceptance.

## Architecture And Framework Probe
Use `src/ui.rs` and role-focused `ui/{gui,model,...}` files: pure panel state/config projections separate from actual session ownership and rendering. Reuse existing bounded event queues/cancellation/explicit shutdown rather than adding a runtime framework or sharing mutable UI through worker locks. Keep unsafe in the existing narrow Windows boundary.

Probe exact `eframe/egui0.37.0` with default features disabled and explicit native renderer, font and accessibility features. Official docs confirm Rust1.95 minimum (installed1.99.1), `App::ui`, viewport sizing and system-font fallback. Inspect downloaded exact-version manifests/source before coding version-sensitive APIs. Verify Windows linking, actual GPU startup, Korean fonts and DPI; if unavailable, record the actual failure and revise the implementation choice without claiming render success. Existing layout/interaction is the comparison target, not a new visual design. Human review of material visible differences remains due at the concrete GUI checkpoint.

## Verification And Exit
Compare pure state/selection/event/control traces against source mock fixtures, then exercise fake slow/cancelled/failed sessions and actual renderer UI without game effects. Use the computer-use skill to inspect returned exact helper windows, rendering, enabled states, resizing and close; never click game-action starts against real backends. Preserve snapshots/metrics only while needed; promote any durable evidence to its explicit owner before cleaning temporary artifacts.

Required final checks: fmt/locked all-target check/Clippy/relevant release tests/release linking, real helper rendering and direct interaction, dependency/font audit, source/config/asset preservation and no task scratch left after diagnosis. Evidence coverage and required human review must remain distinct from evaluator PASS. Default app startup wiring and whole distribution remain FEAT-0027/0028. Final FEAT-0025 acceptance waits for actual recording integration and concrete appearance/interaction review.
