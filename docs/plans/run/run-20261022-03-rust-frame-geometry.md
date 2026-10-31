# RUN-20261022-03: Rust 소유 프레임과 좌표

## Metadata
- ID: `run-20261022-03`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0018](../feature/feat-0018-rust-frame-geometry.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Active Spec: [SPEC-0017](../spec/spec-0017-rust-frame-geometry.md); Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Goal
Validate owned frame/view memory boundaries and preserve coordinate mapping/freshness before implementing image resampling or actual capture/input.

## Selected Loop
Single backend foundation. FEAT-0016/0017 passed. Parent owns architecture/implementation; named read-only scout supplied source call sites/test coverage. No game/UI/input is authorized by this run.

## Contract Surfaces
BGR8 buffers, stride/crop, layout/target/client, mapping/rounding/serialization/freshness. Pixel resize is FEAT-0019's downstream boundary.

## Current Artifacts
SPEC-0017, frame/geometry source and Rust/Python parity oracle tests. [EVAL-0017](../evaluation/eval-0017-rust-frame-geometry.md) owns the results.

## Evaluation Coverage
Contract/Functional PASS; complete for this spec: 7 frame/geometry tests and all21 Rust tests passed, fmt/check/clippy/release and both oracle Black/compile checks passed. No pixel normalization, native input or game acceptance is inferred.

## Current Route
No blocker. Continue to FEAT-0019's image operations; overall parity remains Unknown/Provisional.

## Attempts
- Attempt1: source and geometry consumers investigated; spec fixed for the pure typed boundary.
- Owned/strided/cropped frames and geometry mapping implemented. Python comparison covers borders/negative origins/half-pixel origin-before-rounding/inverse, invalid points/aspect/client, actual reference layout and stale geometry. Missing geometry does not call the fresh-target callback. Checked extent/overflow and independent copy tests passed.
- Fixed source packet: fmt/check/clippy/release, library4+config10+geometry7=21 tests/0failures. Both oracle scripts passed Black and in-memory compile. Sandboxed Black stalled and was interrupted, then identical check passed with host permissions; no source workaround was made.

## Post-Contract Regression Check
No Python runtime/config JSON/assets changes. New oracle/support are integration-test-only. Cargo target is retained for ongoing builds; this feature created no persistent scratch or image outputs.

## Human Review Outcome
User delegated full implementation; this run proceeds within the existing scope. Final app transition and game recovery remain separate.
