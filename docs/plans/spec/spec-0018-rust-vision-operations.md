# SPEC-0018: 현재 사용하는 이미지 연산

## Metadata
- ID: `spec-0018`; Status: `approved`; Run ID: `run-20261022-04`; Attempt: `1`.
- Parent Feature: [FEAT-0019](../feature/feat-0019-rust-vision-operations.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface: `backend`; Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Source Set
Current Python OpenCV4.13.0/NumPy1.21.6 operations in common vision/detectors/frame_geometry, Summoner vision/navigation and actions town/gem vision; their existing tests/assets and REFACTOR-0002. Official OpenCV4.13 formulas/implementation parameters and pinned image/RustFFT API docs support the port. Existing Python is the executable test oracle.

## Implementation Goal
Supply owned/borrowed typed images and the actual primitive operations used by consumers, without Python/OpenCV runtime dependencies. Keep domain detector thresholds/ROI/decisions with their owning modules.

## In-Scope Behavior
- Decode current PNG/JPEG/WebP plus BMP with pure Rust image codecs; preserve grayscale/BGR/BGRA channel modes for unchanged loads, explicit color/gray loads, PNG writes and Unicode paths. Asset inventory: PNG473 BGR8+53gray8+1BGRA8, JPEG7BGR8, WebP1BGRA8. Unsupported depths/modes return errors rather than silently changing matcher types. SHA256 reads stream asset bytes for later manifest identity checks.
- Reuse/generalize the tested frame/view storage for gray/mask/f32 images. Checked dimensions/channel counts/strides/allocations remain required; native BGR aliases keep their API.
- Nearest, pixel-center linear and area resize; FEAT-0018 geometry normalization first resizes the full frame linearly then overwrites the client crop with area iff both source client dimensions are >= target, otherwise linear. Native identity borrows the original frame. Source-size mismatch is an error.
- BGR→gray/HSV/Lab, saturating channel arithmetic, in-range masks, reflect101 Gaussian/normalized box filters, rectangular/cross morphology including even2×2 anchors, Otsu/fixed binary threshold, 4/8-connectivity components/stats/centroids and L2 mask5 chamfer distance.
- Raw correlation, normalized correlation and per-channel-mean-removed normalized coefficient maps. Constant template/image, map shape and max/tie semantics are explicit. Efficient correlation may use pure RustFFT; it must satisfy the same numerical/decision gates.
- Generic TemplateMatcher preserves rule order, one maximum per rule, >= threshold, template dimensions and actions. Peak suppression remains a caller operation. Diagnostic overlay drawing/GUI preview rendering belongs to CLI/GUI features, not detection.

## Out-Of-Scope Behavior
ORB/RANSAC (FEAT-0020), video encoder, native capture/input, font-rendering/preview windows, domain policy/manifest rewrite, full OpenCV API and new detector thresholds. No algorithm substitution is justified by an approximate score alone.

## Affected Surfaces
Cargo manifest/lock, common frame generic storage, `common/vision_ops/` and detectors, Python/Rust operation oracle tests. Existing Python/runtime/config/assets remain unchanged.

## State And Interaction Contract
Images own bytes or borrow immutable validated storage. Kernel/size/type mismatches return Result errors. Caches must have explicit ownership/bounds and cannot keep whole frame histories implicitly. No production subprocess or external codec is used here.

## Data And Contract Assumptions
Before comparison the numerical gates are: lossless PNG/nearest/gray/HSV/integer morphology/threshold/label membership exact; JPEG/color-Lab/linear/area/Gaussian each channel absolute error <=2/2/1/1/1 respectively; f32 filters/distance absolute error<=1e-4; normalized template score absolute error<=2e-5 and raw correlation relative error<=1e-6 with absolute floor1e-4. Integer maxima/selected boxes/threshold decisions must match on compared cases; a close score cannot waive a changed decision. Component IDs may be compared through memberships only where IDs have no consumer-visible meaning; stats iteration order and seed-selected component must preserve actual consumer behavior. Unmet gates return to implementation or planning; do not relax them after seeing failures.

## Contract Surfaces
User-approved amendment (`2026-10-23`): JPEG replay/reference decoding may differ by up to3/255 per channel, with unchanged detection presence/evidence/selected variants. The measured7-reference/154-decision comparison permits up to3 floating center differences (each axis≤1px) and at most one rounded center difference of1px; observed accepted-result score drift is bounded by7e-5 in this fixed reference packet. This exception changes neither detector thresholds nor the screen-capture/PNG and other operation gates above. Retain the original failed comparison as history; rerun the complete operation packet under this explicit amendment before foundation acceptance.

Producer: codecs/numeric kernels; consumer: geometry normalizer and later native observers/actions. Current assets are read directly and test data are deterministic synthetic values or explicit existing paths. No asset copies or runtime-generated oracle caches enter distribution.

## Required Evaluators
Contract: formulas/rounding/border/type/ownership, constant/empty/invalid inputs, tie/rule/threshold behavior and dependency tree. Functional: Python operation-map comparison with predeclared gates, current asset decode samples/inventory, existing geometry fixtures plus independent image patterns, fmt/check/clippy/tests/release and oracle Black/compile. Later domain observers must validate final decisions on their own fixtures.

## Acceptance Mapping
Image/normalization invariants→decode/resize/owned/view tests. Numeric parity→operation maps and selected peaks/threshold boundaries. No OpenCV runtime→Cargo tree and source paths. This foundation pass alone cannot establish whole app behavior/performance parity.

## Evaluation Focus
OpenCV CCOEFF_NORMED differs from generic uncentered normalized correlation. Color channel means remain separate. Constant templates yield1; flat target variance and near-unit score treatment must be checked. Morphology default border is operation-specific; Gaussian is reflect101. Tie ordering must be observed, not inferred from an option name.

## Open Blockers
None for implementation start. Exact numerical/asset/decision parity is not yet established and remains a pass gate.

## Continuity Notes
- `2026-10-22`: FEAT-0018 passed; bounded source/asset inventory fixed this primitive boundary under full execution delegation. image0.25.10 and RustFFT6.4.1 were initial pure Rust dependencies. SHA2 0.10.9 supports existing asset identity contracts. JPEG comparison rejected image's Zune path (maximum4); selected jpeg-decoder0.3.2 with platform_independent/no Rayon gives maximum3 on the current7 references. The original <=2 gate and exact downstream coordinate gate remain unresolved pending human decision; they have not been relaxed. No external native codec is selected.
