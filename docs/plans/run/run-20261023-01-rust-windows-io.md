# RUN-20261023-01: Rust Windows 캡처·입력

## Metadata
- ID: `run-20261023-01`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0021](../feature/feat-0021-rust-windows-io.md); Spec: [SPEC-0020](../spec/spec-0020-rust-windows-io.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-23`.

## Goal
Owned standard Windows capture, geometry and interruptible guarded input adapters, verified without dispatching input.

## Current Boundary
FEAT-0022 passed. JPEG remains independently blocked under RUN-20261022-04; native BGRA capture does not use it. Parent fixes architecture/ownership and implementation; named read-only workers gather bounded evidence or run fixed checks. No game input/focus/cursor mutation is authorized by this run.

## Source Findings
Capture precedence is named-window→region→monitor. Window capture tries PrintWindow2→BitBlt, rejects all-black, auto falls back to screen and forced window errors. Installed mss7 uses GDI SRCCOPY|CAPTUREBLT. Window/client geometry must remain unchanged across capture. Existing GDI cleanup lacks an exception-safe guard; Rust owns every acquisition and unselects before GetDIBits per the official API contract. Field pointer mapping uses the virtual desktop,3px readback/80ms timeout/5ms polls; key presses keep50ms with finally-release.

## Current Route
Contract/Functional→pass for the input-free SPEC-0020 boundary; [EVAL-0020](../evaluation/eval-0020-rust-windows-io.md) owns acceptance evidence. Capture selection/fallback, native GDI/window/DPI wrappers, guarded keyboard/mouse/pointer payloads, physical outstanding-input cleanup, movement state and the system hotkey listener are implemented. Current final checks passed12 fake/differential tests and4 explicit native tests, fmt/all-target Clippy/release. Native input and final GUI/app integration remain unverified. Next independent foundation is ORB background motion.

## Hotkey Compatibility Decision
Source inspection found the installed keyboard library matches the exact complete pressed scan-code tuple: an extra held F2 prevents a plain F8/F9/F10 hotkey, while matching repeated key-down events repeat callbacks. The current Rust tracker preserves that behavior, passes events through and filters the same VK_PACKET/injected-Alt cases. The owner answered to preserve the existing behavior on2026-10-23; the proposed extra-held-key Stop correction is not authorized and is not applied. These are application shortcuts, not Diablo defaults: current config sets CLI F8 pause/resume and F9 stop; Summoner registers both configured keys plus fixed F10 as Stop, and Gem Summing/Item Looting/Room Lifecycle use fixed F10. The owner raised game-skill-key conflicts; the same physical key can currently trigger the application action while that listener is active. No remapping or conflict-policy change is authorized by this answer.

## Attempts
- Attempt1: preserve capture/native payload predicates. Native desktop coordinates are signed to support negative monitor origins; FieldObservation separately retains its original nonnegative aim validation. A generated Unicode11 title casefold table preserves the baseline Python3.7 comparison without a Python runtime. A test-only SendInput replacement compares every installed keyboard mapping under both NumLock states; it sends no native input.
- Parent checks passed10 new tests and three explicit input-free native smoke tests. The first final packet found a Clippy duplicate-branch warning in the fake insertion-failure helper; the parent merged the equivalent predicates before rerunning. That packet stopped at lint and is not counted as a test/build PASS.
- A subsequent fixed-source packet passed the actual full filtered command `cargo test --locked -- --skip jpeg_decode_uses_declared_pixel_bound --skip jpeg_reference_decisions_are_compared_without_waiving_pixel_gate`:77 passed,0 failed,3 ignored,2 JPEG gates filtered. It also passed three explicit native checks, fmt/Clippy/release, both new Python Black/in-memory compile checks and bootstrap smoke0/invalid native exit2.63 source/manifest hashes matched. The verifier ran this broad command despite a requested nonvision-only packet; its actual scope is recorded rather than relabeled.
- Final source refinement serializes only final pointer SendInput/rechecked guard with close; a blocked bounds query cannot dispatch after stop. Physical input cleanup recovers poisoned ownership for releases, retains failed one-shot ups for retry, attempts all releases even after a backend panic and reports the fault. Native Windows construction initializes/reports DPI before coordinate use.12 new tests pass; the final named fixed-source packet repeated affected tests, fmt/all-target Clippy, release and all four native checks with matching hashes/status. The earlier77-test result predates these refinements; the full suite was not repeated afterward.
- The added own-window fixture initially returned all-black before its first compositor frame (hidden/off-screen versions did not establish usable pixels). A brief16px NOACTIVATE/TOOLWINDOW fixture now paints known top/bottom colors, waits100ms only during test setup, captures once and verifies exact BGR orientation and unchanged foreground before destruction/unregistration. No production retry/fallback/pixel gate was weakened. Temporary diagnostic instrumentation was removed; no screenshots or private titles were persisted.

## Human Review Outcome
Continue full port delegation. Game recovery prerequisites, JPEG decision, GUI/codec and final transition acceptance remain separate.
