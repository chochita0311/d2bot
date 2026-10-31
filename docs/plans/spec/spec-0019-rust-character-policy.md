# SPEC-0019: 캐릭터 정책과 관찰·입력 상태

## Metadata
- ID: `spec-0019`; Status: `approved`; Run ID: `run-20261022-05`; Attempt: `1`.
- Parent Feature: [FEAT-0022](../feature/feat-0022-rust-character-policy.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface: `backend`; Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Source Set
Current common casting/survival/buffing/field_control/realtime/field_runtime and corresponding tests, configuration schema contracts and refactor invariants. Existing Python functions/state are development-only differential oracles. Passed FEAT-0017/0018 provide prerequisite data/ownership contracts; JPEG/Image/Win32 are not required for this fake-input boundary.

## Implementation Goal
Provide typed policies, immutable context-stamped observations, explicit state transitions and cancellable owned input/runtime boundaries. Same valid profile/time/observations must produce the same hold/reason/request/confirmation trace.

## In-Scope Behavior
- Casting reference catalog/context precedence and below-or-equal last FCR breakpoint; preserve unknown/unconfigured holds and reference metadata. Large JSON integers remain exact during lookup; converting animation frames/time is checked at its actual consumer.
- Survival belt counts/learned targets and buff stamps stay character/room scoped. Newer sequence with nonregressing timestamp, full belt replacement/invalidation and confirmed deltas/effects preserve source ordering. Unknown is not zero/safe. Priority, equality/age boundaries, deficits/order/pickup and future/older-state holds follow current source.
- Buff sequence keeps repeated tokens, last matching timed step, requested/dispatched/acknowledged/effect-confirmed phases. Automatic renewal/inspection retain safety, gaps, total/confirmation/watchdog timeouts and preemption. Approved invalid optional pause repair uses existing consumer default and warnings; values unrepresentable as runtime durations are also handled at the conversion boundary.
- Field limits/cadence, source/generation and latest-observation admission, potion confirmation and release deadlines retain source predicates. Fake backend/clock traces check pause/stop/focus failure and no re-hold after deadline.
- Runtime owns recent immutable frames (bounded5 where source requires), worker handles, shared monotonic origin/cancellation and independent input-release service. No actor/state lock is held during capture, blocking waits, backend pointer confirmation or joins. Mutable state is owned by its actor; buffers/policies share immutable Arc only as needed. Avoid ownership cycles.

## Out-Of-Scope Behavior
Native Win32 capture/dispatch/focus (FEAT-0021), image observers, gameplay improvements or current main integration, new confirmed-effect claims and PRD-0008 retention. No keyboard/game input is sent during evaluation.

## Affected Surfaces
Rust common casting/survival/buffing/field_control/realtime/field_runtime/time/worker helpers, pure ResourceReading/HudReading and same-frame actor bridge (image recognition remains out of scope), config policy adapter visibility/validation reuse, minimal serde dependencies and Rust/Python oracle tests. Existing Python/config/assets are read-only.

## State And Interaction Contract
Validated constructors publish complete values. State transitions return explicit decisions/Result, preserve failed/incomplete outcomes and accept confirmation only with matching current provenance. Input ownership records intended held keys before down so error cleanup can release them; cleanup release remains permitted after pause/stop/focus loss and attempts every held key. Explicit close/join reports failures; Drop provides non-panicking best-effort cleanup.

## Data And Contract Assumptions
Use enums/Option for known alternatives and unknown evidence. Public constructors validate finite/nonnegative time and explicit positive limits; per-consumer strict/inclusive age predicates stay separate. Fake/native monotonic seconds never mix with wall-clock recording timestamps. Typed runtime sequence counters/dimensions fit native ranges; unbounded FCR/reference integers retain their JSON precision. Config schema validators remain one implementation shared with runtime adapters.

## Contract Surfaces
Producer: validated policy/context/observation. Consumer: state/held owner/runtime followed by native adapters. Inputs/outputs are ordered fake traces and snapshots, not actual effect evidence. Actors own mutable policy state; a buff sequence receives explicit state access rather than storing a second mutable owner or cyclic references.

## Required Evaluators
Contract: state/stamp admission and mutation order, unknown/priority/equality cases, ownership/cancellation/cleanup/no-lock-across-blocking boundaries. Functional: deterministic Python traces for actual policies and boundary cases, fake delayed pointer/backend errors and independent release, casting missing/unsupported/huge integer cases, fmt/check/clippy/tests/release and oracle formatting/compile. Record unrelated known JPEG gate filters explicitly if they remain pending; do not report whole-repo unfiltered PASS.

## Acceptance Mapping
Same policy/time/request→casting and survival/buff/field oracle traces. Provenance→negative/future/duplicate/clock regression and generation checks. Release safety→fake backend failures and delayed work with clock/deadline service. Runtime ownership→stop/pause/join/late-result tests with bounded buffers. Actual native dispatch and game success remain unverified here.

## Evaluation Focus
Preserve the exact order in which invalid newer belt readings replace previous known state. Sequence equality differs between observation admission and confirmations. Potion/buff request never proves consumption/effect. Do not merge the independent shared runtime into the currently active Summoner main.

## Open Blockers
None for this input-free boundary. FEAT-0019 JPEG compatibility remains a separate blocked decision and does not define these policies.

## Continuity Notes
- `2026-10-22`: source state/validation/consumer contracts inspected and feature fixed under full port delegation. FEAT-0019 returned to planning for JPEG while this independent foundation becomes the sole in-loop feature.
