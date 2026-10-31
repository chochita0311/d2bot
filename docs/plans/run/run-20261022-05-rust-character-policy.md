# RUN-20261022-05: Rust 캐릭터 정책·필드 상태

## Metadata
- ID: `run-20261022-05`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0022](../feature/feat-0022-rust-character-policy.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Active Spec: [SPEC-0019](../spec/spec-0019-rust-character-policy.md); Profile: `foundation-contract`; Evaluators: Contract, Functional; Created: `2026-10-22`; Updated: `2026-10-23`.

## Goal
Port context-stamped casting/survival/buff/field/provenance state and owned fake-input/runtime behavior without game or OS input.

## Selected Loop
Single backend foundation after passed0017/0018. FEAT-0019 is blocked on a JPEG-only human decision; it is not a prerequisite. Parent owns typed architecture, state/clock/ownership and implementation. Named workers may gather bounded read-only source/test evidence or run fixed validation.

## Contract Surfaces
Policy schema reuse, casting reference/unknown states, survival belt/effect confirmations, buff/field/input trace, monotonic/generation and bounded worker lifecycle.

## Current Artifacts
Typed casting/survival, buff sequence/executor/automatic renewal/inspection, field decisions/cadence, shared held-key owner, bounded realtime snapshots/generation and owned field/realtime workers are implemented. Same-frame pure HUD readings feed the actor mailbox; navigation requests retain source/age/focus/deadline gates. Development-only policy/field/realtime/HUD Python oracles and seven new Rust integration targets provide comparison evidence. [EVAL-0019](../evaluation/eval-0019-rust-character-policy.md) records the completed input-free boundary.

## Evaluation Coverage
The named verifier's fixed-source packet passed fmt, all-target Clippy with warnings denied, locked tests (**67 passed, 0 failed, 2 JPEG tests explicitly filtered**), release build, all four individual Black/in-memory oracle compilation checks and bootstrap help/version/diagnose. The seven new targets contain33 tests; library/config/geometry contribute21 and unaffected vision contributes13, including1056 exact asset decoding comparisons. Source/manifest hashes and Git status were unchanged by verification. The excluded JPEG gates remain blocked under RUN-20261022-04; this is not an unfiltered whole-port PASS.

After that packet, the parent added a startup readiness handshake so the field actor cannot dispatch before the independent release service owns cleanup, and revision-counter exhaustion now stops/releases rather than returning early. A second fixed-source packet passed fmt, all-target Clippy, the23 affected field-runtime/worker/field-control/buff tests and release build. Explicit native exit-code capture verified invalid bootstrap usage returns2; a prior shell-wrapper status1 was not an app failure. No runtime Python/config JSON/assets changed and no native/game input was sent.

## Current Route
Contract/Functional→pass for SPEC-0019. Native Windows capture/dispatch is the next independent foundation; JPEG remains with RUN-20261022-04. Existing game recovery restrictions remain in force.

## Attempts
- Attempt1: policy/stamp priority and mutation order read; typed state access/immutable policy ownership and no-blocking-lock rules fixed before implementation.
- `2026-10-23`: Source comparison caught `serde_json::from_value` shortening a huge integer through f64; casting adapters now parse the validated JSON spelling directly and the10^300 comparison passes. Optional pause conversion also repairs values beyond `Duration` range to the consumer's existing default with diagnostics. Held input tracks intent before key-down, retains failed releases and checks an operation epoch after slow pointer work. Two multi-file Black invocations stalled in the sandbox and were cancelled; individual file formatting then completed successfully. No temporary files were created by those invocations.
- `2026-10-23`: Completed bounded generation/Arc ownership, capture-start provenance, actor mailbox/HUD bridge, independent release/focus service, pause-resume interruption retention, navigation reply cancellation and bounded join retry. Fake pointer stalls cannot block key release or permit a late re-hold. Worker start waits for release-service readiness. Full acceptance evidence and remaining native/image/UI limits are in EVAL-0019.

## Post-Contract Regression Check
Recheck shared config comparisons if adapters/validator visibility change. Python/config JSON/assets remain unchanged; oracle subprocesses are test-only.

## Human Review Outcome
Proceed under full implementation delegation. Existing game recovery/input restriction and final app-transition acceptance remain separate.
