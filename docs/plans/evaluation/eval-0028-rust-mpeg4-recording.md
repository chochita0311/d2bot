# EVAL-0028: 직접 Rust MPEG-4·AVI 녹화

## Metadata
- Run: [RUN-20261023-07](../run/run-20261023-07-rust-mpeg4-recording.md); Attempt: `1`.
- Feature: [FEAT-0026](../feature/feat-0026-rust-recording-contract.md); Spec: [SPEC-0026](../spec/spec-0026-rust-mpeg4-recording.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Result: `PASS`; Evidence Coverage: complete for the approved stored/synthetic/fake-capture boundary; Date: `2026-10-23`.
- Primary owns semantic judgment. Named evidence scout supplied exact reference snippets; bounded verifier supplied command results without completion authority.

## Contract And Implementation
Production recording uses safe Rust MPEG-4 Visual I/P coding and the direct RIFF/OpenDML writer. Progressive rectangular8-bit YUV420, bounded−16..15 integer-pixel block motion, standard motion/DC/coefficient prediction/VLC, H.263 quantization, local reference reconstruction and periodic/scene-cut I pictures are implemented. Quantizer4 is default, accepted range2–31, VOL dimensions≤8191 per axis. No B/quarter-pixel/full ASP or formal profile/level certification is claimed. XVID AVI metadata does not mean the GPL Xvid C library is linked.

AVI preserves silent8fps independently of capture sampling, native dimensions, source output naming and relative paths. Indexes mark key/P frames correctly and split RIFF segments with bounded per-segment indexes. Checked packet/frame/offset limits and explicit finalization preserve earlier committed frames on partial writes. Capture/encoder/file belong to one worker; cancellation/close retains it until actual completion, reports timeout/panic/cleanup errors and refuses overlapping restart. No encoder work joins on the GUI thread or holds the field input-release owner.

The source's omitted capture close and live-worker forgetting are repaired within the authorized ownership requirement. Atomic CreateNew reports same-second output collisions instead of truncating another completed/active recording. Existing stop hotkeys and game logic are unchanged. MJPEG implementation and jpeg-encoder dependency remain only under developer examples/tests for reproducing the rejected size alternative.

## Evidence
- Five release MPEG-4 groups passed: exact normal/odd dimensions and counts; I/P source-quality controls; first/middle/last seeking and forced RIFF rollover;320-frame duplicate/reference/scene-cut stress; positive/negative/diagonal/boundary motion and chroma interpolation; quantizer limits,1×1 decoding and changed-dimension error.
- Four recording groups passed: independent MJPEG muxer regression, partial-frame rollback and seek/flush/truncate finalization failure retry, output collision/dimension guards, real fake-capture worker cancellation/timeout/restart refusal/capture error/panic, explicit capture close and independently decoded committed first MPEG-4 frame.
- Three1922×1140 stored/synthetic64-frame sequences decoded normally at8fps with exact counts/dimensions, seek hash equality and empty captured decoder stderr. Q4 files were0.538/0.717/0.838× source XVID size for static/pan/cuts, with Rust/source PSNR36.38/36.15,36.46/36.35,35.98/36.51dB. Rust write p95 was39.43/39.88/50.84ms, below125ms at8fps; source XVID encoded faster. Complete inputs, timing boundaries, secondary quantizers and zero-motion failure evidence remain in RUN07; rejected MJPEG measurements remain in RUN06.
- The odd synthetic sharp-color boundary initially failed an arbitrary30dB check. All-I and source-XVID controls established inherent420 boundary loss: odd40-frame source27.63/Rust27.25dB. The retained stress comparison requires≥26dB and≤1dB below the source's identical common rectangle; replicated even padding is explicit because the source writer truncates odd sizes. This is distinct from detector/JPEG-still tolerances and does not establish identical odd-size container output.
- Configured frozen packet passed fmt, locked all-target check, Clippy with warnings denied,6 UI release groups and24 integration tests (5 MPEG-4,4 recording,13 action-runtime,2 action-session), plus release session-preview linking. No ignored tests were run. Git state and scratch counts stayed unchanged during that packet.
- Normal dependency tree contains no opencv/ffmpeg/xvid/jpeg-encoder; inverse normal jpeg-encoder tree prints no dependency. Decoder/source encoding is the pinned development-only Python3.7.9/OpenCV4.13/NumPy1.21 oracle. Required VLC table data and retained legal notice regenerated identically from the pinned reference; each Python fixture passed Black/in-memory compilation without cache files. A multi-file Black invocation hung and was interrupted; successful formatting used one file per process.

## Attribution And Cleanup
The normative VLC data adaptation retains PacketVideo Apache2 and ISO/ACTS-MoMuSys notices and pinned source/hash in THIRD-PARTY-NOTICES and `licenses/mpeg4-reference-notice.txt`. The reference C files were never compiled/deployed. No repository licensing change or external publication occurred.

All37 owned reference/benchmark files were removed after writers/decoders exited, exact absolute parent/filenames and no reparse/nested entries were checked. Tests self-cleaned their own directories. RUN06/RUN07 retain durable results without depending on scratch paths. Shared Cargo caches, source PNGs, Python runtime, config and assets remain intact.

## Evidence Gaps And Route
Physical game recording, full app entry/CLI/distribution, live cadence/CPU/memory/gameplay latency and final user transition acceptance remain required in FEAT-0025/0027/0028. They are non-blocking for SPEC-0026's explicitly incomplete-app stored/fake boundary and blocking for whole-port completion. Codec byte efficiency on three sequences does not predict every game scene or promise a faster overall app. No full-port parity or gameplay success is inferred.

Pass the scoped recording boundary, resume GUI integration, then complete CLI and distribution under the existing delegation. Real game tests follow full wiring and fresh current-state/recovery checks.
