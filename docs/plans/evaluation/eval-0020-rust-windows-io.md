# EVAL-0020: Windows 캡처·입력의 입력 없는 검증

## Metadata
- Run: [RUN-20261023-01](../run/run-20261023-01-rust-windows-io.md); Attempt: `1`.
- Feature: [FEAT-0021](../feature/feat-0021-rust-windows-io.md); Spec: [SPEC-0020](../spec/spec-0020-rust-windows-io.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Contract
- Result: PASS; Evidence Coverage: complete for SPEC-0020; Acceptance impact: not applicable.
- Unsafe is confined to the platform Windows module. windows-sys0.61.2 supplies SDK ABI layouts; non-owning window IDs expose no raw pointers to common logic. Windows construction initializes/reports DPI before coordinate use. GDI resources remain local to each capture call, restore/unselect before GetDIBits and close through RAII on every exit. No unsafe Send/Sync implementation or cyclic controller/runtime ownership is used.
- Window/region/monitor precedence, first exact/contains matching with frozen Python3.7 Unicode11 casefold, named follow/current-client checks, PrintWindow2→BitBlt, all-black rejection and auto screen fallback retain source predicates. Screen uses SRCCOPY|CAPTUREBLT. Successful packets own BGR data and geometry; failed capture does not publish new geometry. Screen/source close is explicit/idempotent and reports failures.
- One small gate controls interruption; signed physical coordinates retain negative monitor origins while FieldObservation keeps its nonnegative source contract. Pointer normalization/readback uses ties-even,3px/80ms/5ms and no resend. Slow queries/readback hold no release lock; final dispatch is serialized with close and rechecks the gate. Physical outstanding down/button intents are recorded before dispatch, including one-shot presses. Failed releases remain for retry; poisoned ownership permits cleanup while blocking new downs. Cleanup attempts all owned releases and reports errors; Drop is non-panicking best effort.
- The native listener uses the same system WH_KEYBOARD_LL boundary as the existing keyboard package, forwards events, has bounded callback delivery on a separate worker and requests Stop on unexpected listener failure. It does not enter the game process. Current exact pressed-combination/repeat behavior is preserved; the proposed Stop-key correction is still a separate pending human choice.

## Functional
- Result: PASS; Evidence Coverage: complete for SPEC-0020; Acceptance impact: not applicable.
- 12 fake/differential tests cover modes/fallback/target order, invalid layout constructor cleanup, changed geometry and reuse, negative monitor indexing/origins, Unicode expansion/title order, every installed pydirectinput key payload under both NumLock states, partial down/failed one-shot up/retry, poisoned dispatch cleanup, pause/stop/focus release, pointer timeout/interruption without resend, stop during blocked bounds query, movement hold/release and configured hotkey traces. Oracle SendInput is replaced and never generates input.
- Four explicit native smoke tests passed:32 small screen captures;64-iteration GDI resource balance with a live DC/bitmap accounting anchor; four listener start/unhook cycles; and an owned16px two-color fixture confirming exact BGR/top-down output and unchanged foreground focus. The fixture waits100ms for its initial compositor frame only in setup, captures once and is destroyed/unregistered; production black-frame rejection/fallback remains unchanged.
- A fixed-source packet before the final pointer/cleanup/DPI refinements passed77 tests with0 failures,3 ignored native checks and2 explicitly filtered JPEG gates, plus fmt/Clippy/release, both new individual Black/in-memory compile checks and bootstrap help/version/diagnose0, invalid native exit2.63 source/manifest hashes matched. Its actual full filtered command is recorded in the run.
- After the final refinements, the named limited packet passed fmt, all-target Clippy with warnings denied,12 affected integration tests, all four explicit native checks and locked release build, with matching four-file hashes and unchanged Git status. The broad suite was not repeated after those refinements. Incremental hard-link warnings caused file copies and did not fail the checks.

## Limits And Cleanup
No native key/mouse input, game activation, real action session, configured game-window success, rendered app GUI or gameplay outcome was verified. Native listener startup/cleanup and synthetic chord traces do not establish physical hotkey interaction under a running game. These are separate integration/live gates, not missing requirements of this input-free spec. Optional Stop-key behavior correction remains pending; JPEG remains separately blocked under RUN-20261022-04. Overall port parity remains Unknown/Provisional. Existing runtime Python/config JSON/assets remain unchanged. No screenshots/private window titles or scratch files were persisted; native fixture/resources were closed and temporary diagnostic source was removed. Ignored Cargo output remains for ongoing work.

## Sources
Source comparisons use installed mss7.0.1/pydirectinput1.0.4/keyboard0.13.5 and current Python modules. Native contracts were checked against [Microsoft GetDIBits](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-getdibits), [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput), [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc) and [windows-sys0.61.2](https://docs.rs/windows-sys/0.61.2/windows_sys/).

## Route
Pass this foundation; continue ORB background motion. Keep game recovery and final transition acceptance separate.
