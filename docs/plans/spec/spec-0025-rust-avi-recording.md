# SPEC-0025: 직접 작성하는 Rust AVI 녹화

## Metadata
- ID: `spec-0025`; Status: `approved`; Run ID: `run-20261023-06`; Attempt: `1`.
- Feature: [FEAT-0026](../feature/feat-0026-rust-recording-contract.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Authority And Source
The user authorized the complete Rust port and now directs implementation of the proposed Rust AVI/MJPEG alternative first, with measured file-size growth. This explicitly permits the XVID→MJPEG experiment; do not imply XVID bitstream parity. Primary owns source/architecture/evaluation. The GUI feature waits on this dependency and is not a concurrent builder.

Golden source: `diablo2/common/capture.py::SessionRecorder`, `diablo2/actions/recording.py::RecordingSession`, GUI start/stop, `core/bot.py` and normalized recording/capture config. The source stores no audio, encodes at fixed8fps while sampling separately, uses native GUI dimensions without reference resize and names `recordings_dir/diablo2_YYYYMMDD_HHMMSS.avi`. No production app video reader/seek/convert exists; independent decoding belongs to verification only.

## Implementation And Ownership
Implement RIFF/AVI headers, frame chunks, padding, indexes and explicit finalization in safe Rust. Use a verified exact pure-Rust JPEG encoder rather than reimplementing DCT/entropy coding. Keep one frame plus a bounded current-segment index in memory. Handle segment/file-size limits explicitly, preserve successfully indexed frames after capture/cancel failures and report write/finalization/cleanup errors. Do not silently overwrite an existing generated file or silently discard geometry changes. No external FFmpeg executable, OpenCV, audio, MP4 substitution or retention subsystem.

Use OpenDML segments/indexes for long files; verify rollover at a reduced test threshold without allocating multi-GB test media. Any actual frame/count/header limits must be checked before writes and documented. Encoder quality is an explicit implementation tuning point, compared at multiple values before choosing the production value. Keep existing config files unchanged; identify legacy XVID spelling and report the intentional MJPEG substitution instead of producing mislabeled XVID data.

The common recorder owns a seekable output and explicit finish. The GUI recording session owns its capture, writer and worker through cancellation and real cleanup; no UI-thread joins/I/O. `is_running` stays true until actual worker exit. Start/stop/errors and filenames retain source meaning; capture-close omission and premature thread forgetting are repaired under the user's resource-safety requirement. Use the existing bounded action event queue. Source's8fps file time base remains separate from configured sampling delay.

## Verification And Exit
Test headers/index offsets/WORD padding, zero/single/multiple frames, rollover, dimension mismatch, same-name collisions, write/seek/flush/close failures, cancellation during slow work, restart ownership and capture cleanup. Independently decode with the fixed Python/OpenCV development oracle: codec/FPS/dimensions/frame count/seeking/color/duration/normal and interrupted finalization. Compare XVID and Rust MJPEG on exactly the same stored/synthetic frame sequences; report bytes/ratio/MiB-per-minute, quality metric and encode time with source/dimensions/frame count/temporal-pattern limits. These figures are not live-game throughput.

Require fmt/locked all-target check/Clippy/relevant release tests/release linking and actual dependency inspection. Later completed-app live capture/performance is authorized by the user, after fresh game-state inspection; this run uses stored/synthetic pixels and fake capture only. Preserve measured results in the run/evaluation, then remove owned temporary media. Final GUI integration resumes RUN-20261023-05. Full EXE/distribution and gameplay acceptance remain separate.
