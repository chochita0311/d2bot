# SPEC-0027: Rust 실행 진입점·CLI·관찰/재생 도구

## Metadata
- ID: `spec-0027`; Status: `approved`; Run ID: `run-20261023-08`; Attempt: `1`.
- Feature: [FEAT-0027](../feature/feat-0027-rust-cli-replay.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Authority And Source
The human delegated the full Rust port through a working executable, preserving existing behavior except explicitly approved changes. FEAT-0017–0026 supply implemented domain/Windows/action/UI/codec contracts. Python `diablo2/app.py`, `core/bot.py`, `common/controller.py`, `common/detectors.py`, the four retained `tools` modules and `actions/town_loop_vision.py` remain comparison sources. Existing tests and exact configured Python/OpenCV versions supply development-only oracles. No Python/OpenCV runtime ships.

## Implementation And Ownership
Keep one root Cargo package and one `d2bot.exe`. Empty arguments launch the implemented GUI. Existing main flags keep their names, defaults, type/choice/exclusion rules and routing precedence: list windows, town/Arcane action, Summoner, GUI, then `--cli` preview. Preserve original argparse last-value wins, unambiguous long-option abbreviations and valid Unicode integer conversion. Help/usage errors do not initialize game effects. Existing Rust version/diagnose commands remain available; diagnose reports actual capabilities rather than stale bootstrap status.

Map Python module invocations to explicit Rust subcommands `survival-observe`, `field-replay`, `arcane-navigation-replay`, `field-input-probe`, and `town-loop-vision`. Their inner flag names, required/exclusive rules, JSON/asset outputs, directory creation, limits, dry/live semantics and exit/error meanings follow their source modules. Document invocation mapping rather than dropping less-used tools. Reuse existing policies/runtime/calibration models instead of forking their logic.

`app` owns argument routing/error boundaries, `core` owns the template-match preview loop, and `tools` owns developer command adaptation. Share a small parser only for actual common argument semantics; no new parser/C codec dependency is needed. OS paths remain `OsString`/`PathBuf`; textual policy values require valid Unicode. Console output errors propagate. Runtime failures are actionable errors rather than false success.

Core preserves first-frame initialization, ordered matching/actions, configured dry-run, pause/stop chords, status/rectangle/Hershey overlay and fixed8fps full-size annotated recording. The preview uses the existing Rust native renderer and exact source resize semantics; Escape/close requests stop. Implement only the drawing primitives actually consumed, using the pinned OpenCV implementation as the pixel comparison source and retaining any incorporated attribution. No C/OpenCV calls are allowed in production. GUI renderer changes do not authorize different recorded annotation pixels.

Own capture, input, hotkey/console registrations, recorder and any preview worker explicitly. Ctrl+C only requests cancellation in the native callback; no allocation, locks, joins or I/O there. Cancellation/cleanup must release owned inputs, close capture/finalize committed video, report failures and retain workers until actual completion. Avoid holding console-output locks across the GUI event loop. Poll/worker cancellation must remain independent of slow vision/encoding.

## Verification And Exit
Compare parser acceptance/defaults/routing/exclusions and tool JSON/errors against source fixtures without game effects. Compare overlay primitives/pixels and preview resize, recorder full-size frame selection and ordered dry actions on fixed frames. Exercise EOF, broken output, malformed/oversized JSON, missing assets/config, stop/pause/close and partial initialization/finalization with fake effects. Reuse existing action tests; no synthetic replay result establishes live success.

Run configured fmt/locked all-target check/Clippy, relevant release tests and release linking. Directly observe the preview with stored/fake frames. Actual game tests/performance remain deferred until every main/tool surface is connected, then FEAT-0028 performs fresh scene/recovery checks, no-Python distribution and final live validation. No current blocker needs another human choice; exact implementation defects remain in this run.
