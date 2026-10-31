# EVAL-0019: 캐릭터 정책·필드 상태와 worker 소유권

## Metadata
- Run: [RUN-20261022-05](../run/run-20261022-05-rust-character-policy.md); Attempt: `1`.
- Feature: [FEAT-0022](../feature/feat-0022-rust-character-policy.md); Spec: [SPEC-0019](../spec/spec-0019-rust-character-policy.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Date: `2026-10-23`.

## Contract
- Result: PASS; Evidence Coverage: complete for SPEC-0019; Acceptance impact: not applicable.
- One actor owns mutable survival/buff/field state. Immutable policies/frames use Arc; transactions bind the exact owning state through an identity token, preserving character/room context without cyclic ownership. JSON schema validation is reused; huge casting integers retain their original decimal spelling and ordered breakpoint semantics.
- Observation admission preserves strict sequence/nonregressing time, per-consumer age/deadline predicates and invalid-newer belt mutation order. Unknown resources/stock remain unknown. Potion/buff dispatch is distinct from acknowledgement and confirmed effect; repeated buff tokens and request-time renewal dates remain intact.
- Held intent is recorded before down; failed releases remain owned for retry. Stop/pause/focus loss can release while pointer movement or readback stalls. Epoch/deadline checks reject late re-holds. Pause-resume retains interruption of an active buff transaction. Backend calls, capture and joins do not hold actor/state locks.
- Runtime workers share one monotonic origin/cancellation owner, bounded latest mailboxes and five recent frame references. Generations discard late results. Navigation checks current source, age, buff due time, phase, focus, pause revision and the live release service; a timed-out reply cannot dispatch later. The actor waits for release-service readiness at startup. Explicit close/join reports errors and retains unfinished handles for retry; Drop cleanup catches unwinding failures.
- Approved malformed optional pauses recover to the existing consumer default and retain warnings. Required policy errors remain explicit. Image recognition, native input and GUI warning presentation are outside this boundary.

## Functional
- Result: PASS; Evidence Coverage: complete for SPEC-0019; Acceptance impact: not applicable.
- 33 new integration tests passed: character-policy6, buff-runtime4, field-control3, realtime2, workers6, field-runtime10 and HUD-readings2. Python development oracles compare current casting FCR0..202 plus10^300/huge reference frames, missing/unsupported context, survival priorities/equality/stamp mutation, repeated-token buff/effect traces, renewal watchdogs, field travel/combat/loot/potion/cadence, generation admission and pure HUD numeric/belt interpretation.
- Concurrent fake backends cover partial down/failed release, blocked pointer and confirmation, pause after movement, focus failure, stale queued belt, fast pause-resume, native-like watchdog gating, navigation timeout and move-only/single-cast behavior. Worker cases cover start-time provenance, late generation, close failure/panic, cancellable waits and bounded join retry. Arc/Weak assertions over1000 unique frames retain exactly the bounded recent frames/latest payload and release them after invalidation and caller-snapshot drop.
- A fixed-source packet passed fmt, all-target Clippy with warnings denied, locked tests (**67 passed, 0 failed, 2 filtered JPEG gates**), release build and four individually formatted/compiled Python oracles.1056 PNG/WebP asset decode comparisons passed within the13 unaffected vision tests. Before/after57 file hashes and Git status were unchanged.
- After the startup/counter cleanup refinement, fmt, all-target Clippy,23 affected runtime/worker/field/buff tests and release build passed again with unchanged file hash/status. Bootstrap help/version/diagnose succeeded; explicit child exit-code capture confirmed invalid usage returns2. A shell-wrapper status1 was investigated and does not change that result.

## Limits And Cleanup
No native capture/input, image HUD observer, current Summoner integration, rendered GUI or gameplay outcome was evaluated. Those are separate features, not evidence gaps in this fake-input spec. The two JPEG gates remain unresolved under [RUN-20261022-04](../run/run-20261022-04-rust-vision-operations.md); the whole suite was not reported as unfiltered PASS. Overall parity remains Unknown/Provisional. Existing765-test Python baseline was not repeated because runtime Python/config JSON/assets were unchanged. Test-owned temporary directories self-clean; no recordings or durable outputs depend on scratch. Ignored Cargo output remains for ongoing port work.

## Route
Pass this foundation; proceed to an approved native adapter spec. The existing human recovery prerequisite for game input remains unchanged.
