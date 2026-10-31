# SPEC-0021: ORB 배경 이동과 특징 provenance

## Metadata
- ID: `spec-0021`; Status: `approved`; Run ID: `run-20261023-02`; Attempt: `1`.
- Parent Feature: [FEAT-0020](../feature/feat-0020-rust-background-motion.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface: `backend`; Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-23`.

## Source Set
Full implementation delegation, common/scene_motion.py and test_arcane_navigation.py, continuous_navigation/navigation_execution callers, route_graph's downstream SceneFeatures use, approved refactor invariants. Installed baseline OpenCV4.13.0 build `4.13.0-1-gb4c5ec4042` is the oracle, pinned by [motion-baseline.json](../../../tests/fixtures/rust-port/motion-baseline.json). Official commit `b4c5ec4042f097e2a5b386b9d413ec7333d0a184` ORB/FAST/partial-affine source and license are implementation references; no OpenCV library is linked or invoked at runtime.

## Implementation Goal
Pure Rust ORB600 feature extraction, Hamming KNN2 and partial-affine RANSAC, with the same state/admission/mask/translation/hold contracts. Features retain points/descriptors, not source images. First compare intermediate extraction/matching and small shifted synthetic cases before integrating the full observer.

## In-Scope Behavior
- Validate current constructor limits and paired landmark viewport/actor exclusion. Build native floor/anchor exclusion or explicit rectangular masks, apply same-frame excluded pixels, then source BGR→gray/linear scale and nearest mask resize. Use ORB's actual default scale pyramid/FAST/Harris/orientation/256-bit descriptor parameters with nfeatures600; retain source tie/order behavior rather than introducing a new tracker or arbitrary hard feature cap.
- Hamming KNN2 in source descriptor order, strict first-distance <0.7×second-distance, source/target index ties. Partial-affine RANSAC uses current reprojection threshold maximum_error×scale and source default confidence/max iterations/refinement with a documented reproducible RNG. Preserve float32 keypoint input and native feature conversion semantics.
- Initial/no-descriptor/size-change seeding, last_seen strict sequence/nonregressing timestamp, features publication order, previous baseline retained after failure and advanced only on accepted displacement. Reset clears all three state owners. Early correspondence-unavailable takes precedence over overlap-hint validation as in the source.
- Overlap projects source points by -hint and target by +hint into the half-open landmark viewport; it is a visibility filter, never a displacement substitute. Failed hint-filtered measurement permits only a successful full-feature stationary displacement with norm<2.
- CameraMotion preserves reason/presence/inliers/matches/ratio/linear-error fields. Reject count<minimum, ratio<minimum, linear_error>0.03 or median accepted residual>maximum_error×scale; equality is accepted. Translation is negative affine translation divided by scale, with original axis order/sign.

## Out-Of-Scope Behavior
Different tracking algorithm, relaxed decision gates, current main/GUI integration, route search/gameplay changes, native input, JPEG delta acceptance and future retention. No source screenshot or private recording is uploaded or added to tracked fixtures.

## Affected Surfaces
src/common/scene_motion.rs and focused algorithm submodules, necessary image-operation additions only when current source math requires them, Rust/Python comparison fixtures and attribution notices. Keep unsafe confined to platform; no new runtime C/C++/Python/OpenCV dependency. Parent owns all architecture/implementation; named workers provide bounded read-only source evidence or fixed checks.

## State And Interaction Contract
One observer owns previous/last_seen/latest features. Share immutable point/descriptor data only when reused; temporary grayscale/pyramids/masks drop after extraction. Validate dimensions/allocation/extents at boundaries. Typed native sequence/time/range validation does not claim arbitrary Python-object compatibility; failures remain explicit and unknown motion never becomes arrival/zero movement.

## Comparison Contract
Pin source inputs, OpenCV build, algorithm parameters and RNG before comparing. Initially target exact keypoint count/order/octave/positions and descriptor bits; record differences at each stage rather than waiving them. Float keypoint angle/response and model arithmetic may use documented numerical tolerances, initially angle1e-3 degrees, response absolute1e-6, matrix coefficient absolute1e-6 and final displacement absolute0.1 native pixel. Reasons/presence, integer match/inlier counts and decision-boundary outcomes remain exact. A failed intermediate or branch comparison blocks parity and requires diagnosis or a separately approved intentional delta; successful displacement alone is insufficient.

## Required Evaluators
Contract: mask geometry and publication/mutation order, strict/inclusive predicates, hint stationary fallback, bounded feature ownership and no input/native dependency. Functional: current seeded noise/shift/blank/recovery/actor-exclusion/hint cases plus fabricated descriptor/RANSAC and threshold boundaries, intermediate differential traces, fmt/Clippy/targeted tests/release. Broader asset/vision comparisons are rerun only after relevant shared image changes; the two pending JPEG gates remain explicit.

## Acceptance Mapping
ORB extraction→keypoint/descriptor comparison; matching/model→fabricated deterministic tie/outlier/degenerate and threshold cases; observer state→duplicate/time/size/no-descriptor/reset/failure-baseline cases; actual behavior→current synthetic and approved local consecutive PNG frames. Any unexamined live/resize/RANSAC conditions remain provisional and cannot establish whole-port parity.

## Open Blockers
No blocker to this implementation/comparison attempt. Exact portable ORB/numerical compatibility remains a measured risk, not assumed success. JPEG and optional Stop-hotkey correction await their separate human decisions; no dependent behavior is inferred from elapsed time.
