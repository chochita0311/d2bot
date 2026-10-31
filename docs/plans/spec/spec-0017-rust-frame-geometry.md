# SPEC-0017: 소유 BGR 프레임과 순수 좌표 변환

## Metadata
- ID: `spec-0017`; Status: `approved`; Run ID: `run-20261022-03`; Attempt: `1`.
- Parent Feature: [FEAT-0018](../feature/feat-0018-rust-frame-geometry.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface: `backend`; Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Source Set
Python `common/frame_geometry.py`, `tests/test_frame_geometry.py`, capture/frame consumers and REFACTOR-0002 invariants. Existing tests establish negative origin, border/client pixel centers, ties-even, inverse continuous coordinates and stale geometry rejection. The existing Python functions are the development comparison oracle.

## Implementation Goal
Provide owned BGR8 frames, lifetime-bound strided/cropped views, reference layout and captured physical geometry with explicit fallible coordinate APIs. No capture, mouse or game calls occur here.

## In-Scope Behavior
- Positive checked sizes; tightly packed owned bytes; borrowed views validate row stride and last-row extent. Crop ends are exclusive and nested crops retain parent stride. Copying a view produces an independent owned frame. Allocation size/overflow failures return errors.
- Reference frame/client box, bounded aspect tolerance [0,0.03], direct UTF8 JSON layout loading and schema1. Actual native Windows dimensions/origins fit signed32; arbitrary Python huge dimensions and dynamic object types are outside this typed boundary.
- FrameGeometry's physical target/client and reference layout remain separate. Without layout, native full-frame geometry is used and any caller client box is ignored as in Python.
- Mapping uses piecewise border/client pixel centers; input coordinates are finite and inside the source interval. Forward desktop coordinates add origin before ties-even rounding; inverse preserves continuous values until the explicit integer crop helper rounds. Native legacy offset retains fractional forward values.
- Geometry comparison checks captured target plus optional current client; stale transforms return the existing RuntimeError message. A missing geometry accepts native legacy mapping without requiring a fresh target query.
- JSON snapshot fields match Python geometry. Native-identity predicate exposes whether normalization can retain the original buffer.

## Out-Of-Scope Behavior
Resize/interpolation/pixel normalization (FEAT-0019), screen/Win32 input/query adapters (FEAT-0021), DPI guessing, new resolution support and unsafe shared/recycled memory. No empty normalize method is introduced.

## Affected Surfaces
`src/common/frame.rs`, `src/common/frame_geometry.rs`, module declarations and development-only Python/Rust geometry parity tests.

## State And Interaction Contract
Constructors publish only validated frames/layouts. Views cannot outlive backing storage; external mutation/recycling requires ownership. Geometry is immutable and every geometry-aware input mapping validates the supplied fresh geometry snapshot. Later capture/input owns obtaining that snapshot at dispatch time.

## Data And Contract Assumptions
Coordinates are f64 for math and signed integer for rounded native points. Size/crop products and conversions are checked. No clamping is introduced where Python mapping returns fractional border values. Allocation/IO/schema/bounds errors are explicit Results; no runtime unwrap/expect or unsafe.

## Contract Surfaces
Producer: owned frame and captured geometry. Consumer: vision/capture/coordinates in subsequent features. Canonical sources remain current Python/layout JSON; no config/assets copies or rewrites. Development oracle subprocess is test-only.

## Required Evaluators
Contract: lifetime/stride/bounds/overflow; formula, rounding order and freshness. Functional: deterministic Python comparison across native/scaled/collapsed-border/negative-origin/half-pixel/invalid points, aspect boundaries/layout errors; owned/crop/stride tests; fmt/check/clippy/tests/release and oracle formatting/compile.

## Acceptance Mapping
Frame safety→constructor/crop/copy failures and data independence. Coordinate parity→exact rounded outputs and continuous absolute tolerance1e-10. Invalid layout/points→error category/message where stable. Stale target/client→rejection before mapping. Pixel normalization and detector decisions remain required downstream and are not inferred from this pass.

## Evaluation Focus
Native forward fractional offset differs from geometry-aware rounded output; origin must be added before rounding. Aspect comparison uses `error > tolerance`, not >=. Input upper endpoint is excluded. A typed API does not claim all Python malformed object compatibility.

## Open Blockers
None for this bounded input-free contract.

## Continuity Notes
- `2026-10-22`: FEAT-0017 passed. Source/consumer evidence fixed this typed boundary under the user's full execution delegation. Resampling is explicitly owned by FEAT-0019.
