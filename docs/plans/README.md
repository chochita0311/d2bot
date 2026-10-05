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

- [Real-time runtime and recording retention](prd/prd-0008-realtime-runtime-and-recording-retention.md): draft requirements for latest-frame consumption, isolated writers, bounded queues/memory, disk quotas and session cleanup before teleport-heavy or 100–200-run expansion. Current town behavior is unchanged; feature execution remains unapproved.

- [Supervised town room loop](feature/feat-0002-supervised-town-room-loop.md): 감독 자동 시도 13번에서 왕복 8회를 완료했다. 최종 확인은 3회 연속 완료와 종료 정리 수정 후 1회 정상 종료다. [실행 기록](run/run-20261004-02-supervised-town-room-loop.md)이 실패 이력·사용자 추가 실행·세션 정리·미검증 복구·사람 수용 상태를 소유한다. 1차 기본 흐름 완료이며 전체 서모너 실행 승인과 구분한다.
- [Documentation conformance PRD](prd/prd-0001-harness-documentation-conformance.md), [feature](feature/feat-0001-harness-documentation-ownership.md), and [run](run/run-20261004-01-harness-documentation-conformance.md): canonical installation and documentation ownership; see the run for validation and review state.
- [Package-boundary PRD](prd/prd-0002-package-boundary-planning.md): draft direction for shared-run, configuration, and runtime ownership; feature execution requires separate approval.
- [Summoner completion and experiment plan](../features/summoner-experiment-plan.md): planning for online softcore Resurrection Flash, with bounded capture/replay/supervised evidence stages. Its PRDs cover [room lifecycle](prd/prd-0003-summoner-room-lifecycle.md), [survival and buffs](prd/prd-0004-summoner-survival-and-buffs.md), [Arcane navigation](prd/prd-0005-arcane-navigation-and-recovery.md), [combat and complete runs](prd/prd-0006-summoner-combat-and-loot.md), and [shared loot/town maintenance](prd/prd-0007-shared-loot-and-town-maintenance.md). Execution is approved only for FEAT-0002 above; the other boundaries remain planning-only.

## Reference Ownership

Project policy, architecture, developer guidance, config references, and domain documentation retain their local owners. The harness manifest tracks only shared-origin assets, not these product/planning records.
