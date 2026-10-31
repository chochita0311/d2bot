# EVAL-0017: 소유 프레임과 좌표 경계

## Metadata
- Run: [RUN-20261022-03](../run/run-20261022-03-rust-frame-geometry.md); Attempt: `1`.
- Feature: [FEAT-0018](../feature/feat-0018-rust-frame-geometry.md); Spec: [SPEC-0017](../spec/spec-0017-rust-frame-geometry.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Date: `2026-10-22`.

## Contract
- Result: PASS; Evidence Coverage: complete for SPEC-0017; Acceptance impact: not applicable.
- Frames own packed BGR bytes. Immutable strided/cropped views borrow backing storage, keep parent stride and validate end-exclusive bounds/last-row extent. Copying creates independent ownership. Size/extent multiplication and allocation reservations are checked.
- Reference/physical/client geometry is separate and immutable. Mapping follows the original piecewise pixel-center expression, adds physical origin before ties-even rounding and preserves continuous inverse values. Native legacy forward retains fractions. Stale target or queried client returns the original RuntimeError before mapping; missing geometry avoids querying current bounds.
- No FFI, capture or input calls are present. Typed native dimensions do not claim arbitrary Python object/huge-integer compatibility. Resampling is explicitly FEAT-0019's responsibility.

## Functional
- Result: PASS; Evidence Coverage: complete for SPEC-0017; Acceptance impact: not applicable.
- Seven tests passed for ownership/stride/crop/copy, mapping/inverse/negative origins, rounding order/native fractions, freshness, invalid points/aspect/boxes/tolerance, collapsed borders and the current asset-owned reference layout.
- Python oracle comparison uses exact integer outputs and continuous tolerance1e-10. The current1922×1140 layout asset is read directly.
- Fixed-source fmt/check/clippy with warnings denied, locked all-targets tests and release build passed: **21 tests total, 0 failed**. Both Python oracle scripts passed Black and in-memory compile. Existing765-test Python baseline was not rerun because runtime Python is unchanged.

## Limits And Cleanup
No resize pixels, detector decisions, Windows geometry queries or dispatched input were validated. These pure checks do not prove resized-window visual accuracy or game success. Overall port parity remains Unknown/Provisional. No scratch/recordings were created; standard Cargo build output remains for ongoing work.
