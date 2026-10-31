# EVAL-0018: Rust 이미지 연산 평가

## Metadata
- Run: [RUN-20261022-04](../run/run-20261022-04-rust-vision-operations.md); Attempt: `1`.
- Feature: [FEAT-0019](../feature/feat-0019-rust-vision-operations.md); Spec: [SPEC-0018](../spec/spec-0018-rust-vision-operations.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Date: `2026-10-22`.

## Current Result After Approved Amendment (2026-10-23)
- Contract: PASS for the bounded primitive foundation under the explicit JPEG amendment in SPEC-0018; Functional: PASS, 15 release operation tests passed without filters. This includes both amended JPEG tests and the complete lossless asset audit; no codec implementation or asset was changed for the amendment.
- The fixed packet preserved all10 source/fixture SHA256 values before/after. Oracle Black and in-memory compilation passed. Earlier fmt/Clippy/release evidence remains recorded below; current action implementation is evaluated separately. Test-owned temporary comparisons self-cleaned.
- Original Attempt1 FAIL is preserved below as historical evidence. This pass neither establishes whole-app parity nor validates downstream town/Summoner runtime or live input. Overall parity remains Unknown/Provisional.

## Attempt1 Contract Before Approval (Historical)
- Result: FAIL against the unchanged JPEG gates; Evidence Coverage: partial; Acceptance impact: blocking for this feature.
- Generic owned Image<T,C>/immutable views retain the prior BGR API. Numeric kernels are safe Rust, use explicit Result failures and avoid a production Python/OpenCV path. Direct/tiled pure RustFFT correlation uses per-channel means and constant/near-unit handling.
- Actual image0.25.10 JPEG/Zune default gave maximum4, scalar Zune remained4, jpeg-decoder0.3.2 default/portable gave3. JPEG does not guarantee the same reconstruction across decoders. Numerical tolerance was fixed before comparison and remains <=2 until a user decision.
- No wider exactness is inferred from mathematically similar kernels. Other domain observers/actions/ORB remain unported.

## Attempt1 Functional Before Approval (Historical)
- Result: FAIL; Evidence Coverage: partial; Acceptance impact: blocking for JPEG; unaffected fixed-source validation completed.
- Scoped tests passed for unchanged current PNG/WebP assets, color/resize/normalization, filters/threshold, morphology, components/distance and template direct/FFT maps/peaks. Asset comparison reads originals in place and hashes decoded bytes; no new permanent media is created.
- JPEG reference evidence:7 files ×22 TownLoopVision template checks=154. Detection presence/evidence/selected variants did not change. Three floating centers changed; the Act2 waypoint-panel reference produces old(934.2775706693005,305.5671280748789) vs new(934.1725900164463,305.3148052911383) for the Act1 world-waypoint detector, which rounds to different y by1. This is a detector comparison, not proof that the room planner dispatches that detector on that scene.
- Across those7 files: maximum channel difference3/255, per-file mean absolute channel difference0.02043–0.05305,373 channels differ by3 among20,035,071. Maximum accepted-result score difference6.872415542602539e-5. These exceed the original pixel/score and exact-coordinate gates; they are not PASS evidence.
- The two JPEG tests remain active/failing. A separate packet filters them explicitly to verify unaffected code; filtered tests must be reported as unverified or known failed, never passed.
- Final unaffected packet:13 vision tests passed/0failed/2filtered;1056 unchanged+gray comparisons across528current lossless images all matched. gAMA/sRGB LUT handling was repaired after an expanded audit found two mismatches. Actual Unicode PNG save/load and oracle comparison passed. fmt/clippy/release and oracle Black/compile passed. Earlier library4/config10/geometry7 checks passed; this is not an unfiltered full-suite PASS.

## Human Decision And Remaining Coverage
The user approved the measured JPEG replay/reference difference (`2026-10-23`), including up to3/255 and one rounded coordinate pixel, while preserving screen-capture/PNG gates. SPEC-0018 records the explicit exception and all15 operation tests passed under it; no detector threshold was relaxed. Whole port parity remains Unknown/Provisional. Downstream observer decision replay remains separate coverage. Actual direct dependencies are image0.25.10(PNG encode/WebP/BMP), png0.18.1(PNG decode/metadata), jpeg-decoder0.3.2(portable/no Rayon), RustFFT6.4.1, SHA2 0.10.9 and serde_json1.0.154; no production Python/OpenCV/native JPEG codec is selected.

## Cleanup
Temporary PNG comparisons live only in exact per-test owned Cargo tmp directories and are removed after oracle processes exit. No asset/config files are changed. Standard build output is retained for ongoing port work; abandoned JPEG dependencies/experiments were removed from the current implementation.
