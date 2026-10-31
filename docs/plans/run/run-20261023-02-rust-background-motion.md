# RUN-20261023-02: Rust ORB 배경 이동

## Metadata
- ID: `run-20261023-02`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0020](../feature/feat-0020-rust-background-motion.md); Spec: [SPEC-0021](../spec/spec-0021-rust-background-motion.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-23`.

## Goal
Reproduce ORB600/Hamming/partial-affine translation and source observer state without native input or an OpenCV runtime.

## Current Boundary
FEAT-0021's input-free Windows foundation passed. This is the sole active builder loop. JPEG and the optional Stop-hotkey correction are separate pending decisions. Existing game recovery/input restrictions remain unchanged.

## Source Findings
ORB600 uses default parameters; BGR→gray is scaled before extraction and mask resize uses nearest. Sequence/time admission occurs after extraction; features publish for each accepted newer frame while the previous baseline advances only on successful displacement or initial/no-descriptor/size reseeding. Matching uses Hamming KNN2 strict0.7 ratio without an extra sort/dedup. Model is partial-affine RANSAC, not homography. Hint-filtered failure only recovers full-feature stationary movement with norm<2. Existing tests assert translations/holds but generally do not assert exact counts/ratios/errors; the new comparison must expose them.

The installed development oracle is Python3.7.9 / NumPy1.21.6 / OpenCV4.13.0, version-control build `4.13.0-1-gb4c5ec4042`, official full commit `b4c5ec4042f097e2a5b386b9d413ec7333d0a184`. ORB/FAST/keypoint/partial-affine source hashes also match the 4.13.0 tag. The build and local MSVC toolset are14.44.35207. Exact parameter values, observed CPU/IPP context, source hashes and fixed comparison limits are retained in [motion-baseline.json](../../../tests/fixtures/rust-port/motion-baseline.json). The oracle refuses changed Python/NumPy/OpenCV/build identity. NumPy Mat inputs use the CPU path; merely having OpenCL available does not select its ORB implementation.

ORB retains response-boundary ties and the baseline MSVC `nth_element`/partition order; a stable sort would change later matches. Each pyramid level uses float32 scale dimensions and Q8 `INTER_LINEAR_EXACT`; nonzero mask values are first binarized. Descriptor preprocessing operates on a non-isolated pyramid ROI, which skips GaussianBlur's whole-Mat Q8 path. The implemented float separable kernel follows the general fallback. Matching comparisons, rather than source reading alone, establish the checked outputs; other CPU dispatch conditions remain outside the measured scope. Ordinary pre-extraction linear scaling is separate, with explicit fx/fy, Q11 coefficients and the special2× area rounding. Odd image edges and scales0.33/0.5/0.7/1.0 have exact pixel comparisons.

Partial-affine uses the source two-point kernel, fixed-u64-max MCG sampling,2000 maximum iterations,0.99 confidence, original inlier mask and dynamic iteration count. Rust refinement solves the same linear least-squares objective in closed form rather than reproducing ten LM iterations. Fabricated grid/rotation/scale/outlier/degenerate cases and eight noisy64-point cases met the predeclared1e-6 coefficient tolerance and exact mask comparison; no tolerance was relaxed.

## Implemented Contract
`src/common/scene_motion.rs` and its focused modules own limits, Hamming matching, model classification, ORB and the observer. One observer retains only previous/latest immutable feature sets and the last stamp; native/scaled points and descriptors have bounded ownership, and source images/pyramids/masks drop after extraction. Native points use float32 division as the NumPy publication does. Blank descriptors remain unavailable rather than becoming a successful zero displacement. Floor/anchor and explicit landmark/asymmetric actor/excluded-pixel masks preserve the source branch, and invalid hint validation retains its original stage after current-feature publication.

Parent-run checks before the frozen final packet passed: five matching/model/extraction integration tests individually or in their affected subsets, observer sequence/time/blank/recovery/resize/reset at scales0.5/1/0.7, floor/exclusion/landmark/overlap stationary cases, early-hint validation/publication, exact native features/descriptors for the existing `north-step-before.png`/`north-step-after.png`, two unit checks for boundary predicates and full-pixel scaling. These are development results, not yet the final evaluation receipt. The two real PNG debug observer calls took904ms and738ms on this host; they are individual observations, not p50/p95 or an accepted performance limit.

## Attribution And Temporary Ownership
[THIRD-PARTY-NOTICES.md](../../../THIRD-PARTY-NOTICES.md) and `licenses/` retain OpenCV ORB/FAST/resize/filter/keypoint/partial-affine and Microsoft STL terms. No OpenCV runtime/library or C++ compilation was added. Official source downloads are task-owned temporary research under `target/port-research/feat-0020`; retain only while this comparison/diagnosis is active, then remove this exact directory after the source hashes/notices and results are durable. Cargo build output remains in the shared task's ignored target directory for continuing port work.

After final verification, all16 task-owned research files and that exact directory were removed. The durable baseline metadata/notices/tests do not reference the temporary files as inputs.

## Final Verification
The frozen limited packet passed `cargo fmt --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, two motion unit tests, five background-motion tests and four observer tests, the release existing-PNG test, locked release build, individual Black check and in-memory Python compilation. All11 source/fixture SHA256 values matched before/after. Release observer times were55ms/59ms for the two existing PNG frames; they are two local measurements, not a statistical or acceptance benchmark. Parent documentation updates overlapped verification, so repository-status equality is not claimed. The verifier made no source edits. JPEG gates, broader suites and native I/O checks were not run in this packet.

## Current Route
[EVAL-0021](../evaluation/eval-0021-rust-background-motion.md) records Contract/Functional PASS for the input-free foundation. Continue to action/lifecycle contracts. Whole-port parity remains Unknown/Provisional; no GUI/main/live/distribution completion is inferred.

## Human Review Outcome
Existing full-port implementation delegation permits the next bounded feature. No comparison limit was relaxed or tracker substituted. JPEG and optional Stop-key behavior remain separate pending human choices; existing live recovery confirmation is unchanged.
