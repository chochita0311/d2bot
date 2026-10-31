# RUN-20261023-06: Rust AVI/MJPEG 구현과 크기 비교

## Metadata
- ID: `run-20261023-06`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0026](../feature/feat-0026-rust-recording-contract.md); Active Spec: [SPEC-0025](../spec/spec-0025-rust-avi-recording.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Orchestration And Boundary
Primary resolved the user's new direction: implement the proposed direct Rust AVI writer plus pure-Rust JPEG encoder first and report actual size growth. This experiment was completed and superseded by the selected MPEG-4 implementation in RUN07; it is not an active builder. No FFmpeg runtime is adopted. Stored/synthetic frame comparison and fake capture/session checks are authorized here. Actual game tests/performance follow complete app wiring and fresh game-state checks under the user's latest request, not this partial recorder.

## Source Evidence
The source only writes silent AVI/XVID8fps; capture sampling is separate. There is no app video-playback/seek/convert call path. A prior synthetic probe identified OpenCV's writer backend as FFMPEG; that development reader probe did not establish an app playback requirement. Source stop can forget a still-live thread after3s and omits capture close; Rust retains the owner through explicit cleanup. Existing generated names can collide within one second; a new recording must report collision rather than truncate an active/previous owned file.

## Current Route
- Bounded MJPEG implementation/size experiment completed; the measured report below was delivered and the user selected direct Rust MPEG-4.
- AVI muxer is reused by that implementation; MJPEG encoder/dependency now belongs only to developer examples/tests. RUN07/EVAL-0028 own the selected production codec.
- Raw experiment media was removed after closing writers and promoting these results; source/config/assets remain unchanged.

## Evidence Coverage And Human Review
The user reviewed the size figures and chose direct Rust MPEG-4 compression instead of the Xvid C library. This MJPEG experiment is not the selected final recorder; [RUN-20261023-07](run-20261023-07-rust-mpeg4-recording.md) owns that new execution boundary. The user's request to test the completed app does not establish current equipment/mercenary recovery or count as a successful gameplay run.

## Measured MJPEG Comparison
All cases use the same exact input hashes across Rust and Python:1922x1140 BGR,64 frames/8 encoded seconds,8fps, three stored PNG sources (Arcane hub `main.png`, town-loop `npc-dialogue-window.png` and `exit-menu-window.png`). Static repeats the Arcane frame, pan cyclically shifts it3px per frame, cuts switches source every8frames. These are stored/synthetic sequences, not continuous live gameplay. Full sequential independent decoding returned64frames and8fps for all15files. JPEG uses4:2:2; this first timing packet used the scalar jpeg-encoder0.7.1 implementation, before enabling its optional Rust SIMD path. The subsequent SIMD feature has not been timed or accepted as equivalent by this packet.

| Pattern | Codec/quality | Bytes | Ratio to XVID | Encode+I/O ms | PSNR dB |
| --- | --- | ---: | ---: | ---: | ---: |
| Static | XVID | 2115434 | 1 | 374.74 | 36.15 |
| Static | MJPEG70 | 17076532 | 8.07 | 873.69 | 35.25 |
| Static | MJPEG80 | 21727796 | 10.27 | 921.37 | 36.45 |
| Static | MJPEG90 | 33444916 | 15.81 | 979.54 | 38.58 |
| Static | MJPEG95 | 50126772 | 23.70 | 1089.84 | 40.58 |
| Pan | XVID | 2741316 | 1 | 408.02 | 36.35 |
| Pan | MJPEG70 | 17105154 | 6.24 | 867.37 | 35.24 |
| Pan | MJPEG80 | 21781428 | 7.95 | 929.48 | 36.44 |
| Pan | MJPEG90 | 33522154 | 12.23 | 979.15 | 38.57 |
| Pan | MJPEG95 | 50271070 | 18.34 | 1077.16 | 40.58 |
| Cuts | XVID | 2728226 | 1 | 384.93 | 36.51 |
| Cuts | MJPEG70 | 14179380 | 5.20 | 855.91 | 36.11 |
| Cuts | MJPEG80 | 18077332 | 6.63 | 890.68 | 37.32 |
| Cuts | MJPEG90 | 27823844 | 10.20 | 947.25 | 39.24 |
| Cuts | MJPEG95 | 41569428 | 15.24 | 1051.41 | 40.93 |

Encode+I/O excludes input generation/hashing/decoding and includes writer opening/finishing. It is a single comparative packet, not a full-app speed/CPU/memory claim. Quality80 projects to129–156MiB/min versus XVID15–20MiB/min under these sequences. Higher PSNR measures lower squared pixel error, not guaranteed better subjective or detector accuracy. Input SHA256:static `5c4d97f7b398e3947a5adb82e8f02eed8e3f725901a44c46b0fc6086bbfb6ece`;pan `f6d2f2923dd2add57abaecc9a5267e7056b96b0cef3b3ff7a55948cca1ca251a`;cuts `f765eeecc863098d976a2d59a1983f7c79505fbcf162d6d6cb3ffc57a3452a5b`.

## Writer/Session Checkpoint
The safe Rust AVI writer implements OpenDML segments/standard and super indexes, first-segment legacy index, WORD padding, explicit patch/flush/truncate and committed-frame rollback. Index memory is bounded to65536 entries per1GiB target segment and1024 super-index entries; each dimension is at most32767 and a single compressed packet must fit within1GiB. Actual rollover was checked at a reduced threshold with an independent decoder; normal/odd dimensions, four-frame color/count and index-seeking passed.

Initial release tests:2passed/2failed. The two failures came from one-frame EOF→seek on the fixed OpenCV/FFmpeg reader. A separate source-writer probe reproduced the same failure for both OpenCV MJPG and XVID; a fresh reader can seek/read frame0. The oracle now records reopening only for that single-frame case instead of attributing this source limitation to the Rust writer. Rollback/finalization-failure tests now also independently decode the recovered output; their updated run remains pending. No complete feature PASS is claimed. Native recording has not been run against the game.

The corrected four release groups subsequently passed, including independently decoding rollback output, cancellation/panic capture cleanup, collisions, dimension changes, and normal/odd/rollover indexes. A frozen packet repeated those4 plus15 action regressions and6 UI groups successfully; fmt/locked all-target check/Clippy and session-preview release linking passed. Final codec selection has superseded this experiment, so these scoped results do not constitute final recording acceptance.
