# SPEC-0026: 직접 구현하는 Rust MPEG-4 녹화

## Metadata
- ID: `spec-0026`; Status: `approved`; Run ID: `run-20261023-07`; Attempt: `1`.
- Feature: [FEAT-0026](../feature/feat-0026-rust-recording-contract.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `backend-product`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Authority And Source
After reviewing MJPEG's measured size cost, the user explicitly chose direct Rust MPEG-4 compression rather than linking the Xvid C library. Keep AVI/no audio/fixed8fps and native dimensions; capture sampling remains separate. This supersedes MJPEG as the final production codec while retaining that experiment's factual measurements. Use the source recorder/session contracts in SPEC-0025 and Python capture/recording/bot modules. No app video playback dependency exists; tests independently decode files.

## Codec Boundary
Implement only needed MPEG-4 Visual rectangular progressive video tools, starting with I/P pictures, owned YUV reference reconstruction, transform/quantization, macroblock/coefficient bit coding, bounded rate/quality controls and temporal prediction. Do not call an external codec executable or C library or label an MJPEG packet as XVID. Correct compatible output must be established by an independent decoder. Do not claim full Xvid ASP feature coverage, profile-level conformance for unsupported resolutions or identical compressed bytes/quality/efficiency without evidence. Document actual limits before adopting them.

Keep syntax, transforms, reference storage and AVI/session ownership separate. Safe Rust, checked dimensions/allocation/indices/counters, explicit errors, no unbounded packet/index queues, and no encoder locks on the input-release path. Reuse and extend the tested RIFF/OpenDML writer with proper key/non-key indexes. Preserve valid earlier frames on capture/cancel/write failure; report finalization/cleanup failure.

Use normative syntax and primary implementation/reference evidence, track any incorporated code/data's actual license and attribution, and avoid assuming that the XVID FourCC establishes use of the GPL Xvid library. No external source publication or repository licensing change is authorized. Final comparison uses exactly the same frame sequences and dimensions as the MJPEG packet and source XVID encoder.

## Verification And Exit
Before production wiring, independently decode normal/odd dimension clips, I/P transitions, exact duplicate frames, motion, scene cuts, periodic keyframes, seeking and segment rollover; check frame count/FPS/duration/color/PSNR and no decoder warnings/corruptions. Compare bytes and encode p50/p95/finish time against source XVID on identical hashes. Report efficiency gaps rather than asserting them away. Check reference drift over longer generated clips and bounded memory/stop/failure cleanup. Continue main app GUI/CLI/distribution integration after this boundary passes; no incomplete bootstrap acceptance.

Require configured fmt/locked all-target check/Clippy/relevant release tests/release link and dependency/license audit. Fake/stored-pixel tests only during incomplete implementation. The user authorizes actual game tests/performance after all features are connected; fresh current scene and unresolved equipment/mercenary recovery must be checked then. Whole-app throughput and gameplay acceptance remain unproven by codec tests.
