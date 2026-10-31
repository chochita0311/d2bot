# RUN-20261022-04: Rust 이미지 연산

## Metadata
- ID: `run-20261022-04`; Status: `passed`; Attempt: `1` (approved-contract revalidation appended).
- Feature: [FEAT-0019](../feature/feat-0019-rust-vision-operations.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Active Spec: [SPEC-0018](../spec/spec-0018-rust-vision-operations.md); Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Goal
Replace the actual used numeric/image primitives while retaining numerical and downstream decision gates.

## Selected Loop
Single backend foundation after frame/geometry passed. Named scout provided bounded read-only common/Summoner/action API/parameter evidence. Parent owns architecture, dependencies, implementation and semantic review.

## Contract Surfaces
Codecs/typed images, normalization, color/filter/mask/morphology/components/distance, template maps/peaks. No game or GUI is executed.

## Current Artifacts
SPEC-0018, image kernels/codecs/generic storage/TemplateMatcher and development oracle tests. [EVAL-0018](../evaluation/eval-0018-rust-vision-operations.md) records current partial evidence and unresolved JPEG gates.

## Evaluation Coverage
The complete release vision-operation packet passed15 tests/0failures/0filters under the user-approved JPEG amendment, with10 scoped source/fixture hashes stable, oracle Black and in-memory compilation passed. The original unaffected packet/history below remains13 tests/2filters and1056 lossless comparisons; its earlier FAIL is not rewritten. Overall parity stays Unknown/Provisional; no unfiltered all-targets/full-app/live PASS is claimed.

## Current Route
The user approved the measured JPEG replay/reference difference on2026-10-23; SPEC-0018 fixes that narrow exception while retaining capture/PNG and detector thresholds. All15 operation tests now pass without filters, closing this foundation gate. Historical blocked routing below applies to the earlier comparison. FEAT-0023 remains the sole active builder loop.

## Attempts
- Attempt1: source API/parameters/tests and existing asset channel/depth inventory read. Current asset file set580 files has SHA256 `fe0db02fa7f7b5cf39aa74c28b2c73aa29e93127cc67c31f6061940e030b21b0` over Python-sorted relative POSIX paths, each UTF8 path+NUL+binary SHA256 content hash. This is a current-tree aggregate, not an assertion of untracked files' baseline provenance.
- Generic typed Image/ImageView retains BGR aliases and adds gray/mask/f32 storage. Resize initially failed AREA enlargement, repaired the observed area emulation branch. Runtime gray initially differed1pixel, repaired to the current15-bit coefficients. PNG grayscale loader is a distinct truncating conversion; official libpng coefficients differ from runtime cvtColor.
- All current lossless PNG/WebP assets passed unchanged-pixel SHA256 comparison. Color/resize/normalization, Gaussian/box/threshold, even-anchor/cross morphology, 4/8 component membership/order/centroids, mask5 distance, direct/FFT correlation and constant-template/threshold/rule order cases have passing scoped tests. This does not establish domain observers or performance.
- Zune JPEG default and scalar trial maximum4; jpeg-decoder0.3.2 maximum3 on all7 JPEG references. Direct color-matrix experiments did not reduce it and were removed. Experimental ColorTransform::None exposed a dependency panic on padded rows; that mode is not used. The final selected path uses normal decode and no copied/edited registry source.
- 154 TownLoopVision comparisons preserve detection presence/evidence/variant selection. Three unrounded blue-pair centers differ, one rounds to a different pixel. Maximum score difference6.872415542602539e-5 and3/255 pixel difference exceed the original gates. Both tests remain failing until the question is resolved.
- The original detector parity fixture hit OpenCV imread's extended/Unicode path limitation, so the rule/threshold comparison uses a normal relative ASCII fixture path. Codec Unicode support needs its separate test; this fixture change is not evidence of legacy Unicode path parity.
- Current verification packet explicitly filters the two known JPEG failures to check unaffected code. Its results will be recorded as partial and cannot mark this feature passed. First packet stopped at Rustfmt indentation; formatted the source and reran.
- Expanded grayscale audit found2gAMA/sRGB assets with mismatches. Traced image0.25's gamma getter to obsolete png0.18 metadata field, switched PNG reads to the same pinned png0.18 decoder's effective gamma API and matched libpng's8-bit linear/encoded LUT behavior. Focused two-asset tests passed, then1056 unchanged+gray comparisons all passed without tolerance changes.
- Final unaffected packet:13 vision tests passed,2JPEG tests filtered; fmt/clippy/release/Black/compile passed. Unicode PNG actual file round trip passed. Previous fixed packet also verified library4/config10/geometry7. No actual game/window/input was executed.

## Post-Contract Regression Check
Existing Python/config JSON/assets remain unchanged; no generated media or temporary oracle output becomes canonical.

## Human Review Outcome
`2026-10-23`: user explicitly approved the measured JPEG difference. SPEC-0018 records its narrow replay/reference bounds; complete release operation revalidation passed15 tests without filters. No detector threshold change or external native codec is authorized. FEAT-0023 remains the sole active builder loop.
