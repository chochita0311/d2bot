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

최신 세션 마감과 다음 실행 인계는 [통합 run Attempt15](run/run-20261009-07-summoner-live-integration.md#attempt15-세션-마감-정리와-다음-gui-실행-인계)가 소유한다. assets 전체 정리에 이어 미사용 private 원본23개와 빈 이미지 폴더를 삭제하고14개 legacy 이미지 이름을 물체/상태 이름으로 맞췄다. 현재 공용339/private0개이며 개인 비이미지 보고서·바인딩은 local owner에 남긴다. 옛 진입/입력 helper·미사용 로그/튜닝 설정을 제거하고 현재 안내와 역사적 증거를 구분했다. 전체557개 회귀(스킵0)가 통과했다. 다음 세션은 GUI Repeat Count1로 현재 새 방 북쪽 왕복·서모너 전투/획득·조건부ALT·제때140초 갱신을 실제로 검증한다. 새 게임 입력과 수용 실적은 추가하지 않았으며 아직 새 방0/5·수용0방이다.

GUI 연결 구현 이력은 [통합 run Attempt13](run/run-20261009-07-summoner-live-integration.md#attempt13-gui-자동-버프-조건부-alt와-공용-자산-정리)이 소유한다. GUI를 최신 main에 연결하고 감독 JSON/I 없이 비율·근접 후보 기반 자동 버프와 실제 S+140초 갱신, 이동OFF/전투후ON 검사/OFF를 구현했다. 예전 북쪽 실행기/버튼은 제거했다. 지형·UI·클래스·카메라·창고 숫자는 공용 assets이며 당시 최소화한 개인 이미지도 Attempt15에서 정리했다. 당시 전체557개 입력 없는 검사와 Tk 생성/배치 검사는 통과했으나 새 자동 경로의 실게임 검증은 하지 않았다. 새 방 완전 왕복0/5·수용0방, 실제 처치/획득·네 방향은 미완료다. 실행 안내는 [현재 Summoner 문서](../features/summoner-run.md), 이전 상세 실패는 통합 run의 당시 Attempt가 소유한다.

- [Perceptual Arcane navigation](feature/feat-0013-arcane-perceptual-navigation.md): 현재 우선순위는 새 방 북쪽 끝→중앙 WP→Act 1→방 종료를5회 반복해 보완하는 것이다. 이후 동/남/서를 연결한다. 지도ON·검토 viewport 카메라·현재 끝/중앙 부위·F2 유지/넓은 조향·탈부착 근접 노바를 사용한다. 일반 지도 선 해석, 모든 variation 정확도, 먼 적 정리 후 원위치 복귀, 실제 처치/회수와 정상 반복은 미완료다. Act 1 정비 세부 동작만 사용자 요청대로 건너뛴다. Act 2 초기화 준비 방은 완료 수에서 제외하며 최종30방 중27방 조건은 유지한다.

- [Automatic buff renewal](feature/feat-0012-automatic-buff-renewal.md): 사용자 지시에 따라 공통 필드 runtime에서 타이머·최신 안전 관찰·프로필 입력·효과 확인을 연결했고 전체 174개 테스트를 통과했다. [Run](run/run-20261009-05-automatic-buff-renewal.md)이 입력 없는 검증과 실제 scene 생산자/감독 갱신의 남은 경계를 소유한다.

- [Potion pickup policy](feature/feat-0011-potion-pickup-policy.md): 기본 대량 활력/현재 6개 이하의 작은 활력 허용·TP 4장 목표를 입력 없는 개인 설정/공통 판단으로 구현하고 전체 160개 테스트를 통과했다. [Run](run/run-20261009-04-potion-pickup-policy.md)이 결과를 소유한다. 실제 라벨/작은 포션 외형·획득/정비는 후속이다.

- [Flash buff recheck](run/run-20261009-03-flash-buff-recheck.md): 당시 새 d→F1 순서를 짧은 감독 probe로 전송했고 초기 버프 효과는 확인했지만140초 갱신은 통과하지 못했다. 한 방의 중앙 버프 증거이며 전체 런 통과와 구분한다. 이후 정상 GUI/main 자동 경로의 남은 검증은 위 Attempt15가 소유한다.

- [Summoner cycle contract](feature/feat-0010-summoner-cycle-contract.md): 전체 방 순서와 새 화면의 긍정 결과·중복 방지·실패 후 반복 금지를 입력 없는 planner로 구현하고 전체 155개 테스트를 통과했다. [Run](run/run-20261009-02-summoner-cycle-contract.md)이 검증 범위와 남은 실제 adapter를 소유한다. 전체 수용 기준은 감독 30방 중 27방 이상 정상 마감이고 실패 시 안전 종료 후 사람 확인이다.

- [Runtime observation provenance](feature/feat-0009-runtime-observation-provenance.md): 단일 단조 시계와 관찰 세대/늦은 결과 거부의 입력 없는 계약을 구현하고 전체 141개 테스트를 통과했다. [Run](run/run-20261009-01-runtime-observation-provenance.md)이 검증 범위와 후속 통합·사용자 확인 대기 항목을 소유한다. 최신 Flash 버프 순서는 w/a/a/s/d/F1/w다.

- [Common held field control](feature/feat-0008-field-control-loop.md): latest-observation mailbox, independent release deadlines, survival preemption, potion confirmation/reconciliation, class/FCR arrival pacing and continuous profile buff execution are implemented. [Run](run/run-20261005-06-field-control-loop.md) owns regression, short supervised input evidence and current integration limits. Automatic scene/effect/death/pickup producers, common executor live composition and complete Summoner runs remain incomplete.

- [Arcane north encounter observation](feature/feat-0007-arcane-north-encounters.md): implemented the approved input-free region candidates, native ground/radius and positive life-evidence contracts. Supervised north attempts retained stair/flat-bridge, Ghoul Lord/Hell Clan alive/corpse and shrine-negative assets; one mana-triggered potion recovery was observed. [Run](run/run-20261005-05-arcane-north-encounters.md) owns recording gaps, buff expiry during supervision, pose/HUD misses and remaining live integration gates. Whole PRD-0006 and autonomous combat remain incomplete.

- [Field buff confirmation](feature/feat-0006-field-buff-confirmation.md): passed the approved supervised Arcane buff test, input-free transaction and positive belt-closure boundary under PRD-0004. [Run](run/run-20261005-04-field-buff-confirmation.md) owns actual changed-max HUD/equipment evidence and 72-test regression. Automatic safety/effect producers, staged-input integration and full four-wing combat remain separate.

- [Survival HUD/belt observation](feature/feat-0005-survival-screen-observation.md): passed the approved input-free CV/CLI boundary under [PRD-0004](prd/prd-0004-summoner-survival-and-buffs.md), with shared assets and bounded actual town checks. Same-frame current/max ratios handle changed maxima; field/buff confirmation and automatic actions remain separate. [Run](run/run-20261005-03-survival-screen-observation.md) owns the evidence and limits.

- [Class casting reference lookup](feature/feat-0004-class-casting-rules.md): implemented the approved first input-free boundary under [PRD-0005](prd/prd-0005-arcane-navigation-and-recovery.md), combining shared Sorceress/Paladin references with individual class/FCR. Flash's confirmed 105 selects 8 reference frames. [Run](run/run-20261005-02-class-casting-rules.md) owns those checks; subsequent common pacing and short input evidence belong to FEAT-0008 above.

- [Character survival/buff state](feature/feat-0003-character-survival-state.md): passed the approved input-free character policy, confirmed buff/belt state, and recovery/exit/replenishment request checks under [PRD-0004](prd/prd-0004-summoner-survival-and-buffs.md). [Run](run/run-20261005-01-character-survival-state.md) owns implementation and checks; live observation/input/combat and asset migration remain separate.

- [Real-time runtime and recording retention](prd/prd-0008-realtime-runtime-and-recording-retention.md): FEAT-0009의 단일 시계·관찰 세대 계약을 구현했다. 저장/제어 분리, 큐·메모리·디스크 상한과 장시간 retention은 후속 기능이며 실제 100–200회 확대는 아직 승인한 검증 범위가 아니다.

- [Supervised town room loop](feature/feat-0002-supervised-town-room-loop.md): 감독 자동 시도 13번에서 왕복 8회를 완료했다. 최종 확인은 3회 연속 완료와 종료 정리 수정 후 1회 정상 종료다. [실행 기록](run/run-20261004-02-supervised-town-room-loop.md)이 실패 이력·사용자 추가 실행·세션 정리·미검증 복구·사람 수용 상태를 소유한다. 1차 기본 흐름 완료이며 전체 서모너 실행 승인과 구분한다.
- [Documentation conformance PRD](prd/prd-0001-harness-documentation-conformance.md), [feature](feature/feat-0001-harness-documentation-ownership.md), and [run](run/run-20261004-01-harness-documentation-conformance.md): canonical installation and documentation ownership; see the run for validation and review state.
- [Package-boundary PRD](prd/prd-0002-package-boundary-planning.md): draft direction for shared-run, configuration, and runtime ownership; feature execution requires separate approval.
- [Summoner completion and experiment plan](../features/summoner-experiment-plan.md): online softcore Resurrection Flash의 방·생존·네 방향·전투/회수·재정비 목표다. 사용자가 `2026-10-09` 요구사항 확정·구현·감독 검증까지 진행하도록 명시했고 현재 FEAT-0013의 상황 인지 네 방향 이동을 실행한다. 기존 기능의 통과는 각 한정된 증거 범위를 유지하며, 미확정 운영값과 후속 기능별 실게임 조건을 전체 목표 승인만으로 채우지 않는다.

- [Summoner platform encounter evidence](feature/feat-0014-summoner-platform-encounter-evidence.md): 현재 룬/일지 바닥 접근과 실제 호버 적 패널 기반 노바, 최대24장 전후 화면의 시각·관찰 순번 기록을 연결했다.215는 실제 접근1회·전용 노바0회였고219는 현재 목표를 즉시 선점했으나 접근 입력이 거부됐다. 다음 재시험에서는 검사 단계·거부 이유·프레임 나이를 구분한다. [SPEC-0013](spec/spec-0013-summoner-platform-encounter-evidence.md)과 [Attempt9 이후 기록](run/run-20261009-07-summoner-live-integration.md#attempt9-서모너-발판-접근과-전투-전후-증거)이 경계를 소유한다. 실제 처치/획득/새 방 반복은 미완료다.

- [Live emergency recovery](feature/feat-0015-live-emergency-recovery.md): main216의 마나 부족 시 포션 입력 누락을 보완한다. 현재 보이는 보라 포션 열1회 입력과 새 HUD 회복·실제 벨트 잔량·닫힘을 이동/전투/대기에 연결했으며 관련141개 검사가 통과했다. [SPEC-0014](spec/spec-0014-live-emergency-recovery.md)와 통합 run Attempt10이 실제 재시험 결과를 소유한다. 자동 줍기/재보급/사망 복구는 미구현이다.

## Reference Ownership

Project policy, architecture, developer guidance, config references, and domain documentation retain their local owners. The harness manifest tracks only shared-origin assets, not these product/planning records.
