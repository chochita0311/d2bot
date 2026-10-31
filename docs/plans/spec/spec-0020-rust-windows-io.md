# SPEC-0020: Windows 캡처·입력·중단 경계

## Metadata
- ID: `spec-0020`; Status: `approved`; Run ID: `run-20261023-01`; Attempt: `1`.
- Parent Feature: [FEAT-0021](../feature/feat-0021-rust-windows-io.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface: `backend`; Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-23`.

## Source Set
User's full Rust/EXE implementation delegation, common capture/controller/movement, current action input consumers, frame geometry tests, policy and refactor invariants. FEAT-0017/0018/0022 provide settings, geometry and held-release/runtime contracts. Python implementations remain development references, never runtime dependencies.

## Implementation Goal
Provide safe owned wrappers around the current standard Windows APIs, plus testable capture selection and guarded input dispatch. Unsafe is confined to the platform module; mutable GDI resources remain on their owning capture thread. Input backends share a small cancellation/pause gate, not a controller/runtime ownership cycle.

## In-Scope Behavior
- Visible nonempty positive-size top-level window enumeration, first exact/contains title selection, outer/client/virtual-desktop/monitor bounds, unchanged split/maximized placement and minimized-only restore when focus is explicitly requested. DPI setup preserves per-monitor awareness with legacy fallback; report its actual outcome.
- Named-window/region/monitor precedence, follow-window and current geometry checks. Window capture uses PrintWindow(flags2), then BitBlt; invalid/all-black window results follow the existing forced-window error versus auto screen fallback. Screen capture uses the same GDI SRCCOPY|CAPTUREBLT BGRA source as installed mss7. BGR bytes own their buffers; normalization and wall-clock timestamps preserve current contracts. Re-query geometry before publishing and input mapping.
- GDI acquisition/selection/unselection/delete/release via RAII on every failure path. Follow Microsoft's GetDIBits requirement to unselect the bitmap before reading it; this corrects resource/API misuse while preserving output and fallback predicates. Explicit close is idempotent and reports cleanup errors; Drop is a fallback.
- Checked SendInput insertion counts; scan-code keyboard events matching current pydirectinput key map, normal mouse buttons, virtual-desktop pointer normalization/ties-even rounding and observed position confirmation (3px,80ms,5ms poll, no resend). Pause/stop/focus prevents new input, release remains available. Press attempts release after partial down failure and keeps50ms duration; cancel can shorten the wait while preserving cleanup.
- Interruptible shared input gate, field backend bridge, movement hold/press state with owned failure cleanup, configurable global pause/stop keys. Hotkey observation may use a standard system keyboard listener equivalent to the existing keyboard library, never a game-process hook; release/stop callbacks cannot block that listener.

## Out-Of-Scope Behavior
Game process internals/hooks, new capture API, gameplay changes, recordings/GUI/CLI wiring and actual game dispatch. No focus change, cursor movement, mouse/key input or game activation during checks. Native capture tests must retain no private screenshots or window-title dumps.

## Affected Surfaces
src/platform.rs and platform/windows modules, common capture/controller/movement, narrowly selected windows-sys features and tests. Platform types hide raw pointers from consumers; no unsafe Send/Sync implementation for GDI handles. Current Python/config JSON/assets are unchanged.

## State And Interaction Contract
Construction validates complete settings before allocating native resources where possible. A failed capture does not publish a new geometry/frame. Stop is sticky; pause/resume uses the existing owner; every issued down remains releasable after focus loss. Errors remain explicit and failed cleanup is not hidden. Backend pointer waits do not share release locks. Focus attachment and system listener handles are detached/unhooked on all exits.

## Data And Contract Assumptions
Native rectangles use checked signed coordinates/positive dimensions; public handles are non-owning window identities. Window ownership stays with Windows. Capture scratch and selected bitmap/DC live within one call/thread and are freed before publication. Screen frames are BGRA→BGR, with no JPEG decoder. Raw API wrappers use Microsoft's windows-sys0.61.2 and official API documentation; no hand-written FFI layouts.

## Contract Surfaces
Producer: Windows/window/capture settings and gate. Consumers: existing FrameGeometry, ScreenSource, HeldFieldInput and later action adapters. Tests substitute native operations at the safe trait boundary. Explicit ignored smoke checks query/capture the real desktop or a brief owned16px NOACTIVATE/TOOLWINDOW fixture, assert foreground focus remains unchanged and destroy/unregister the fixture. No test sends native keyboard/mouse input or activates a game window.

## Required Evaluators
Contract: selection/fallback/freshness predicates, native handle and callback lifetimes, cleanup after partial failure, separate stop/release paths, no cycles or locks across pointer waits. Functional: deterministic fake selection/capture/input/movement traces, failure/cancel/time/negative-origin boundaries, actual input-free capture, fmt/all-target Clippy/targeted tests/release. Preserve the two pending JPEG filters if broad tests run.

## Acceptance Mapping
Capture modes/geometry→fake matrices plus real small screen capture; resource cleanup→failure-path tests and repeated input-free capture; dispatch→fake scan-code/mouse payloads and insertion errors; stop/pause/focus→fake preemption and release traces; hotkeys→listener state tests without generated keys. Actual game input remains unverified and is not required by this spec's explicitly input-free acceptance.

## Open Blockers
None for baseline implementation and input-free checks. The owner selected existing hotkey behavior on2026-10-23; the proposed extra-held-key Stop correction is not applied. The application shortcuts and possible game-skill-key conflicts are recorded in RUN-20261023-01; no remapping was requested. Supervised game input still requires the existing human recovery confirmation. Whole-port JPEG/UI/recording decisions remain with their owners.
