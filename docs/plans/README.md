# Planning And Execution Artifacts

Use the [shared workflow](../agents/flows/workflow.md) for sequence and approval boundaries, [planning management](../policies/harness/prd-feature-management.md) for scope/status, and the [runner](../agents/operations/runner.md) to start or resume an approved feature. These are the installed harness's owners of the process.

## Artifact Owners

| Directory | Content | Template |
| --- | --- | --- |
| `prd/` | Bounded scope, exclusions, uncertainty, and acceptance envelope | [PRD](prd/template-prd.md) |
| `feature/` | One loop-sized product or foundation boundary | [Feature](feature/template-feature.md) |
| `spec/` | Implementation-facing execution contract | [Spec](spec/template-spec.md) |
| `run/` | Current execution pass, attempts, routing, and review outcome | [Run](run/template-run.md) |
| `evaluation/` | Evaluator result and evidence coverage | [Evaluation](evaluation/template-evaluation.md) |
| `fix/` | Targeted corrections tied to evaluator findings | [Fix log](fix/template-fix-log.md) |
| `heuristic/` | Non-blocking interaction suggestions | [Heuristic backlog](heuristic/template-heuristic-backlog.md) |

For `docs-content`, an adequate [compact writing contract](../policies/harness/execution-loop-governance.md#docs-content-writing-contract) may be designated as the active spec. Record its real ID and file/section locator in the run and downstream artifacts.

## Current Work

- [Common held field control](feature/feat-0008-field-control-loop.md): latest-observation mailbox, independent release deadlines, survival preemption, potion confirmation/reconciliation, class/FCR arrival pacing and continuous profile buff execution are implemented. [Run](run/run-20261005-06-field-control-loop.md) owns regression, short supervised input evidence and current integration limits. Automatic scene/effect/death/pickup producers, common executor live composition and complete Summoner runs remain incomplete.

- [Arcane north encounter observation](feature/feat-0007-arcane-north-encounters.md): implemented the approved input-free region candidates, native ground/radius and positive life-evidence contracts. Supervised north attempts retained stair/flat-bridge, Ghoul Lord/Hell Clan alive/corpse and shrine-negative assets; one mana-triggered potion recovery was observed. [Run](run/run-20261005-05-arcane-north-encounters.md) owns recording gaps, buff expiry during supervision, pose/HUD misses and remaining live integration gates. Whole PRD-0006 and autonomous combat remain incomplete.

- [Field buff confirmation](feature/feat-0006-field-buff-confirmation.md): passed the approved supervised Arcane buff test, input-free transaction and positive belt-closure boundary under PRD-0004. [Run](run/run-20261005-04-field-buff-confirmation.md) owns actual changed-max HUD/equipment evidence and 72-test regression. Automatic safety/effect producers, staged-input integration and full four-wing combat remain separate.

- [Survival HUD/belt observation](feature/feat-0005-survival-screen-observation.md): passed the approved input-free CV/CLI boundary under [PRD-0004](prd/prd-0004-summoner-survival-and-buffs.md), with shared assets and bounded actual town checks. Same-frame current/max ratios handle changed maxima; field/buff confirmation and automatic actions remain separate. [Run](run/run-20261005-03-survival-screen-observation.md) owns the evidence and limits.

- [Class casting reference lookup](feature/feat-0004-class-casting-rules.md): implemented the approved first input-free boundary under [PRD-0005](prd/prd-0005-arcane-navigation-and-recovery.md), combining shared Sorceress/Paladin references with individual class/FCR. Flash's confirmed 105 selects 8 reference frames. [Run](run/run-20261005-02-class-casting-rules.md) owns those checks; subsequent common pacing and short input evidence belong to FEAT-0008 above.

- [Character survival/buff state](feature/feat-0003-character-survival-state.md): passed the approved input-free character policy, confirmed buff/belt state, and recovery/exit/replenishment request checks under [PRD-0004](prd/prd-0004-summoner-survival-and-buffs.md). [Run](run/run-20261005-01-character-survival-state.md) owns implementation and checks; live observation/input/combat and asset migration remain separate.

- [Real-time runtime and recording retention](prd/prd-0008-realtime-runtime-and-recording-retention.md): draft requirements for latest-frame consumption, isolated writers, bounded queues/memory, disk quotas and session cleanup before teleport-heavy or 100–200-run expansion. Current town behavior is unchanged; feature execution remains unapproved.

- [Supervised town room loop](feature/feat-0002-supervised-town-room-loop.md): 감독 자동 시도 13번에서 왕복 8회를 완료했다. 최종 확인은 3회 연속 완료와 종료 정리 수정 후 1회 정상 종료다. [실행 기록](run/run-20261004-02-supervised-town-room-loop.md)이 실패 이력·사용자 추가 실행·세션 정리·미검증 복구·사람 수용 상태를 소유한다. 1차 기본 흐름 완료이며 전체 서모너 실행 승인과 구분한다.
- [Documentation conformance PRD](prd/prd-0001-harness-documentation-conformance.md), [feature](feature/feat-0001-harness-documentation-ownership.md), and [run](run/run-20261004-01-harness-documentation-conformance.md): canonical installation and documentation ownership; see the run for validation and review state.
- [Package-boundary PRD](prd/prd-0002-package-boundary-planning.md): draft direction for shared-run, configuration, and runtime ownership; feature execution requires separate approval.
- [Summoner completion and experiment plan](../features/summoner-experiment-plan.md): planning for online softcore Resurrection Flash, with bounded capture/replay/supervised evidence stages. Its PRDs cover [room lifecycle](prd/prd-0003-summoner-room-lifecycle.md), [survival and buffs](prd/prd-0004-summoner-survival-and-buffs.md), [Arcane navigation](prd/prd-0005-arcane-navigation-and-recovery.md), [combat and complete runs](prd/prd-0006-summoner-combat-and-loot.md), and [shared loot/town maintenance](prd/prd-0007-shared-loot-and-town-maintenance.md). Execution is approved for FEAT-0002 through FEAT-0008 above, including bounded town preparation, supervised central field buffs, north encounter observation and common held field control; other feature boundaries remain planning-only.

## Reference Ownership

Project policy, architecture, developer guidance, config references, and domain documentation retain their local owners. The harness manifest tracks only shared-origin assets, not these product/planning records.
