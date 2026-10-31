# Developer Guide

This document holds development workflow details that are too specific or verbose for `AGENTS.md`.

## Working Style

- Prefer small, focused changes that preserve current behavior unless the task explicitly asks for a UI or behavior change.
- Read the local code first before changing project structure or patterns.
- Keep edits Windows-friendly.
- Do not revert unrelated user changes.
- Prefer changing behavior through `config/` when possible instead of hard coding values.

## Code Style

- Follow nearby Rust style and run the configured Rust formatter.
- Avoid adding noisy comments for obvious assignments.

## Rust Development

The [Rust port](../plans/prd/prd-0009-rust-windows-port.md) keeps one root Cargo package and the existing domain responsibilities under `src/`. The Python source and interpreter are retired. Frozen pre-retirement observations under `tests/fixtures/legacy-baseline/` retain independently observed expected results; new cases require reviewed expectations, never automatic regeneration from the implementation under test. Only publish modules with implemented contracts; empty files or disabled buttons do not establish migration coverage.

- Use the exact `rust-toolchain.toml` version, track `Cargo.lock`, and keep ordinary Cargo output in the standard ignored `target/`. Run `cargo fmt --check`, `cargo check --all-targets --locked`, `cargo clippy --all-targets --locked -- -D warnings`, relevant tests, and release linking for a completed feature. Metadata/type checking does not prove that an EXE links or runs.
- Rust uses ownership/borrowing and deterministic `Drop`, rather than a tracing garbage collector. Prefer a clear owner for mutable state and owned, validated image buffers. Share immutable frames only when a real consumer needs them; reference counting is not cycle collection. Keep references out of buffers that a producer may reuse.
- Keep fallible runtime operations in `Result` paths, preserve unknown observations and typed failure/stop outcomes, and handle partial writes/initialization explicitly. Runtime `unwrap`/`expect` are denied by the Cargo lints. Do not turn backend errors into false success or add retries without the consumer contract.
- Library code denies `unsafe_code`. If Windows FFI is needed, allow unsafe only in the narrow reviewed platform boundary, documenting calling conventions, handle ownership, buffer bounds and lifetimes. Never use unsafe to bypass an ownership/type error in policy, UI, vision or routing code.
- Give each worker/session a clear cancellation and shutdown owner. Avoid holding a lock during capture, disk I/O, foreign calls, sleeps, input dispatch or worker joins. Input release deadlines and stop handling must not wait for a busy GUI/vision/writer. Confirm normal shutdown errors explicitly; `Drop` is a fallback for releasing owned resources, not proof that an OS release succeeded.
- Vision uses `common::compute` scoped jobs on at most eight shared Rayon workers, capped by available parallelism. The family divides its budget between foreground and broad background work (6+2 at eight workers); nested work stays in the entering pool. Current confirmations belong to the foreground, even when their recognizer also supplies background hints. Active sessions/observers own a `ComputeLease`. Do not create OS threads per frame, retain a strong global pool, or let jobs retain frames after their scope. The final family lease joins both pools' actual OS workers outside registry locks. Keep output ordering and each pixel's arithmetic order when partitioning work.
- Use fake clocks/backends and the fixed legacy/config/assets baseline for parity. Check thresholds, deadlines, ordering, float/rounding behavior, failure and cleanup paths. Safe Rust does not by itself prove correct timing, bounded memory, freedom from deadlocks, or behavioral parity.

The active feature/spec/run owns the actual implemented and verified Rust surface. Keep OS installation failures and local evidence there instead of weakening shared checks or silently switching toolchains.

The Windows packager retains shared runtime assets according to [the asset policy](../../assets/README.md), rather than copying every replay source. Register new runtime asset consumers in `examples/runtime-assets-check.rs` and required reference paths in `assets/runtime-bundle.json`; keep config-selected shared assets portable. A successful loader smoke supports asset availability, not live gameplay correctness.

## Comment Style

- Keep comments short and intent-focused.
- Prefer comments that explain why a block exists, what decision rule it applies, or how a tuning constant affects behavior.
- Use section comments for grouped constants, control-flow stages, and non-obvious helper functions.
- Avoid line-by-line narration; skip comments that only restate the code literally.
- Write code comments in Korean.
- For UI code, prefer section comments that explain layout intent over line-by-line commentary.
- Do not regenerate Korean comments through lossy shell or whole-file rewrite flows that can collapse characters into `?`.
- Prefer targeted edits when changing Korean comments, especially in existing files with mixed old encodings.
- After editing Korean text, verify the file still contains real Unicode Korean characters instead of literal `?`.
- If terminal output looks broken, verify the file bytes or decoded text rather than trusting the console rendering.

## Verification

Run the Rust checks above and `cargo test --all-targets --locked`. The tests execute Rust only; the frozen baseline loader hashes requests and referenced file contents, including newly written AVI bytes. A changed recording therefore needs a new independent decode review rather than reusing an unrelated old decode result. Fixed results cover observed cases, not all possible behavior. Explicit native-window/capture/hotkey tests remain opt-in and do not establish game acceptance.

For historical interpreter/SSL diagnosis, use the pre-retirement [developer guide at the pinned baseline](https://github.com/chochita0311/d2bot/blob/cde68bc2eb1285eddcd8a261cdfe411ed4924f76/docs/project/developer-guide.md#interpreter-diagnostics). The current project requires no Python environment.

## Evidence Handling

Feature references own reusable operating and tuning guidance. Plans/runs and their evaluator reports own execution decisions and validation evidence. Keep the current approved boundary and actual evidence coverage in those canonical artifacts.

During active development, keep only the recordings, logs, and intermediate frames needed for the current diagnosis or verification in a bounded session directory. The GUI log is a bounded live view; [system settings](../../config/system/system.md) describe CLI logging, manual recording and current Summoner evidence ownership.

The owner confirmed the development cleanup rule on `2026-10-05`: cleanup happens when the development session ends, not immediately when a test finishes. Before cleanup, promote the frames/templates actually needed by the program into the appropriate `assets/` owner, then update and verify their consumers. Keep personal captures in ignored `assets/private/` only while they serve an active purpose. Follow the [asset owner and naming guide](../../assets/README.md): identify the subject/state and any feature, layout, language or size not already supplied by its parent directory; avoid generic names such as `frame1.png` or `test.png`.

At session close, stop task-owned writers and remove development logs, recordings, redundant/intermediate frames, scratch files, and other generated diagnostics that the program does not need. Summarize test results and limits in the owning run/evaluation documents before deleting raw diagnostics. Update asset manifests and documentation so retained outputs do not depend on removed session paths. A failed run or an old evaluation reference does not require indefinite retention of its raw data. Preserve required runtime assets, user-owned source/configuration, and active files; verify ownership and exact workspace paths before deletion. This session-close rule supersedes earlier development instructions to retain complete recordings or baseline datasets indefinitely.

## Documentation

- If a new constant or manual tuning point is introduced, make it easy to find and edit.
- Keep relevant project description files in `.md` up to date when behavior, structure, setup, or developer workflow changes.
- Keep project-specific maintenance guidance in repository docs such as `AGENTS.md` and `docs/project/*.md`. Shared harness policies own planning and execution rules; `docs/plans/` owns their artifacts.
- Keep first-time user guidance in `README.md` or other user-facing markdown docs.

## Planning And Execution

- Use [the local harness guide](../agents/README.md) for the shared workflow and role contracts, and [planning artifacts](../plans/README.md) for templates and current records.
- Project-specific implementation and verification rules remain in this guide; planning, approval, and run lifecycle rules remain in the installed shared policies.
