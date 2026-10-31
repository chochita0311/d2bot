# RUN-20261023-07: Rust MPEG-4 압축·녹화

## Metadata
- ID: `run-20261023-07`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0026](../feature/feat-0026-rust-recording-contract.md); Active Spec: [SPEC-0026](../spec/spec-0026-rust-mpeg4-recording.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Orchestration And Direction
User explicitly selected direct Rust MPEG-4 compression after the MJPEG size comparison. Primary resolved this new boundary before implementation. This is the sole active builder; GUI RUN05 waits for final recording integration. RUN06 owns the superseded MJPEG experiment and its scoped results. No Xvid/FFmpeg binary or C runtime dependency is approved/adopted.

## Current Route
Builder implemented; configured validation passed in [EVAL-0028](../evaluation/eval-0028-rust-mpeg4-recording.md). Safe Rust VOL/VOP, I/P coefficient coding, bounded integer-pixel block motion/chroma interpolation, YUV420 reference reconstruction and proper AVI key/non-key indexes are implemented. Production RecordingSession now uses this MPEG-4 writer. MJPEG encoder code/dependency moved to developer examples/tests only. Main app entry is still bootstrap; complete EXE/live performance remains due. The rebuilt fake GUI session was directly observed, including background/footer corrections, F10 stopping and deferred close after recording cleanup; GUI RUN05 owns its scoped evidence, not a full GUI PASS.

## Review And Evidence
Production recording adoption passed the scoped stored/fake boundary; no whole-port PASS. Do not substitute MP4/MJPEG or a native encoder for the user's explicit choice, or claim Xvid efficiency from merely decodable output. Keep generated test media scoped under `target/tmp`, promote results and remove owned media after closing writers. Later game tests require the completed app and fresh state checks.

### Initial Independent Decoder Evidence
- 64×48 and65×49 clips,1/40 frames, decoded without diagnostics at the exact dimensions/count/8fps. First/middle/last seeks matched sequential decoded hashes. A20-frame forced RIFF rollover clip likewise passed, with I frames0/16 and P-frame indexes between.
- An initial absolute30dB synthetic threshold failed for65×49. Investigation compared all-I coding (27.82dB) and per-column errors: the sharp blue wrap boundary dominated, rather than progressive reference drift. The source XVID encoder, using replicated even padding and comparing only the identical original rectangle because its writer truncates odd dimensions, measured27.63dB against Rust27.25dB; even64×48 measured30.85/30.47dB. Tests retain the synthetic stress case and require≥26dB and no more than1dB below that source control. This control does not establish equal whole-file dimensions or byte efficiency for odd sizes.
- The temporary all-I/padding hypotheses were removed. Local reconstruction retains the padded coded macroblocks. Quantization uses H.263 mode, not MPEG quantization matrices; the reference's matrix mismatch-control branch does not establish a missing H.263 adjustment.

### Initial Zero-Motion Comparison — Superseded By Motion Work
Same1922×1140/64-frame/8-second stored sources, generators and input hashes as RUN06; Rust q4, GOP16, fixed8fps, YUV420, no game capture/input. All12 files independently decoded64 frames with matching seeks/count/dimensions/FPS and no printed decoder diagnostics. Python's XVID baseline uses its actual OpenCV/FFmpeg backend only as a development oracle.

| Sequence | Source XVID bytes / PSNR | Rust bytes / ratio / PSNR | Rust encode+I/O / write p95 |
| --- | --- | --- | --- |
| Static | 2,115,434 /36.15dB | 1,084,380 /0.513× /36.41dB | 1,543.54ms /38.57ms |
| Pan3px/frame | 2,741,316 /36.35dB | 17,118,762 /6.245× /35.02dB | 2,476.33ms /41.63ms |
| Cuts every8 frames | 2,728,226 /36.51dB | 2,174,454 /0.797× /36.42dB | 1,550.50ms /38.96ms |

Source encode+I/O was368.77/412.90/385.36ms respectively. Q3 and q6 also decoded: static ratios0.661/0.351, pan8.868/3.777, cuts1.048/0.542; their differing PSNR does not establish matched quality. These figures establish the zero-motion deficiency and are not final performance. Task-owned raw comparison files were removed after the final motion comparison was promoted.

### Reference And License Scope
Primary reference: Android PacketVideo source at commit `1e6a675fccdc37afcfb337f8fa97d1ddab90adf0`, MPEG-4 encoder directory. The reproducible generator extracts only required normative VLC table data from pinned `vlc_enc_tab.h` SHA256`9cb73c2482d73d3cf11fce3be647d889d2380adb6d0db831f9917311351f30f2`. `licenses/mpeg4-reference-notice.txt` retains the actual PacketVideo Apache2 and ISO/ACTS-MoMuSys notices; THIRD-PARTY-NOTICES links them. C reference files are analysis-only scratch, never compiled/deployed. No Xvid C/FFmpeg runtime dependency is introduced. VOL dimensions are bounded to8191 per axis, progressive rectangular8-bit4:2:0; no full ASP or formal profile/level certification is asserted for1922×1140.

### Motion Encoder Comparison And Production Wiring
Same inputs/hashes/count/dimensions/timebase as the zero-motion packet. Each macroblock may use a bounded−16..15 integer-pixel luma vector; median neighbor prediction, differential VLC and half-chroma interpolation follow the reference syntax. No B pictures, quarter-pixel or full ASP feature claim. Large/unmatched motion falls back to coded residuals or scene-cut I pictures, so these three sequences do not establish universal Xvid efficiency.

All12 comparison files decoded64 frames with exact size/count/8fps, first/middle/last seek equality, and **empty captured decoder stderr**. Python source XVID encode+I/O:367.70/398.64/372.18ms (static/pan/cuts). Quantizer4 is the production default; comparison timing excludes input generation/hashing/independent decoding and includes writer open/write/finish.

| Sequence | Rust bytes / ratio to XVID | Rust / source PSNR | Encode+I/O | Write p50 / p95 | Finish | MiB/min Rust / source |
| --- | --- | --- | --- | --- | --- | --- |
| Static | 1,138,452 /0.538× | 36.38 /36.15dB | 2,168.78ms | 33.00 /39.43ms | 2.09ms | 8.14 /15.13 |
| Pan3px/frame | 1,966,206 /0.717× | 36.46 /36.35dB | 2,345.06ms | 36.17 /39.88ms | 1.69ms | 14.06 /19.61 |
| Cuts every8 frames | 2,286,746 /0.838× | 35.98 /36.51dB | 2,229.68ms | 34.11 /50.84ms | 1.73ms | 16.36 /19.51 |

The source encoder is faster in this scoped measurement. Rust write p95 stays below the125ms interval of8fps, but does not establish whole-app throughput, real recording cadence or live gameplay latency. Q3 ratios/PSNR are0.701/37.59dB,0.888/37.70dB,1.087/37.32dB; Q6 are0.377/34.55dB,0.538/34.61dB,0.590/34.37dB in static/pan/cuts order. These have different quality and are not matched-quality size claims.

Five MPEG-4 release groups passed: normal/odd I/P source controls; seekable RIFF rollover;320-frame duplicate/reference/scene-cut stress with per-frame MSE≤9 and compact P packets; translated texture in positive/negative/diagonal and motion-range limits; quantizer rejection/minimal1×1 frames/changed-dimension failure. Four recording groups also passed after production wiring: independent MJPEG muxer regression, write/seek/flush/truncate failure finalization, collision/dimension guards, actual blocked worker cancellation/timeout/restart refusal/capture failure/panic with finalized first MPEG-4 frame and capture close. These use fake capture/stored pixels, not the game.

Earlier compilation checks found and corrected cast/comparison parentheses, a checked test allocation return, and a test-only private cleanup call after moving MJPEG. Clippy's manual range checks were corrected; the final configured packet determines current static/link status. No failed attempt is counted as passing.
