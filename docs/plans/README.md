# Planning And Execution Artifacts

현재 Summoner는 단계A·북쪽 단독10연속 정상 마감을 개발 검증한다. 원본 개발의61번 실패와2026-10-22 중단 이력은 [세션 마감](run/run-20261009-07-summoner-live-integration.md#session-close-20261022)이 소유한다. 이후2026-10-24 사용자가 시체·장비·용병 복구와 Act1 저장 종료를 확인했으며, 새 Rust 포팅 검증의 실제 장면·입력·종료 결과는 [최종 배포 run](run/run-20261023-09-rust-windows-distribution.md)이 소유한다. 같은 복구 확인을 다시 요구하지 않되 현재 장면/포커스/HUD 보호는 유지한다. 북쪽 단독→북남 조합→동쪽 단독→북동남→북동남서 순서·기존 성공/실패/연속 수·미검증 경계는 [통합 run의 개발 재개](run/run-20261009-07-summoner-live-integration.md#development-resume-20261020)를 유지한다. 포팅 검사는 이 원본 수용 실적에 더하지 않으며 실제10연속과 전체30방/27방 수용은 미완료다.

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
| `refactoring/` | Cross-cutting refactor plans, invariants and verification gates; each plan records its approval state | [Frame coordinates](refactoring/refactor-0001-frame-coordinates.md), [Rust port draft](refactoring/refactor-0002-rust-port.md) |

For `docs-content`, an adequate [compact writing contract](../policies/harness/execution-loop-governance.md#docs-content-writing-contract) may be designated as the active spec. Record its real ID and file/section locator in the run and downstream artifacts.

## Current Work
이번 세션의 마지막 범위는 사용자가 승인한 [Python 소스 정리와 세션 마감](run/run-20261031-01-rust-source-retirement.md)이다. 앱/도구/테스트의 Rust-only 전환과 소스 제거·문서 리뷰·최종 release370검사/배포 확인을 마쳤고 새 게임 입력은 하지 않았다. commit/push와 최종 정리는 마감 run receipt를 따른다. [엄격 비교 merge check](refactoring/refactor-0002-rust-port-merge-check.md)는 전체 parity의 Unknown과 지정 검사/승인된 delta를 구분한다. 이 정리의 완료가 아래 서모너 기능 수용을 완료시키지 않는다. 새 세션은 마감 기록의 최종 receipt와 아래 FEAT-0031에서 이어간다.

[Rust 서모너 개발](feature/feat-0031-rust-summoner-navigation-refinement.md)이 현재 승인된 실행 loop다. 사용자 요청에 따라 일반 공격 반경 축소와 비국소화 구간의 현재 바닥 연속성을 보완하고, 북쪽 단독 새 방3회마다 검토한 뒤 원래 단계 순서를 이어간다. [SPEC-0031](spec/spec-0031-rust-summoner-navigation-refinement.md)·[현재 RUN](run/run-20261024-03-rust-summoner-development.md)이 구현/새 시도/복구/다음 행동을 소유한다. 아래 CPU 개선은 이 실행의 기준이며50ms는 지향 목표다. TP 없이 기존 north→중앙→Act1→저장 종료 코드로만 복구한다.

[현재 평가](evaluation/eval-0034-rust-summoner-navigation-refinement.md)는 구현·배포 검사와 실제 방의 성공·실패·지연·자원을 구분한다. I-loot의 R18은 단상 도착 뒤 처치 판독 실패와 대기 방어의 포인터 오류로 미완료였으며, 여섯 번째 묶음은1정상/2실패·누적6정상/12실패/연속0이다. 봇·녹화/GUI는 정상 종료했다. J는 지도 가림 재관찰·실제 몸통 호버·방어 후 처치 재확인·기존 시체 조명 보정과 로그의 바닥 따라가기/위로 읽기 유지를 구현해 관련 release164 검사와24.06MB 배포·외부CWD 실제 EXE dry를 통과했다. J의 게임 로직을 유지한 최종 Rust-only 배포는 `dist/d2bot-windows-x64-rust-only/`이며 release370 PASS/6ignored와 실제 EXE dry/필수 자산 확인을 마쳤다. 추가 실게임 없이 세션을 마감하며 새 입력 전 현재 장면과 조작 인계를 확인한다. 정상 마감은 처치→일지→붉은 포탈→협곡 WP→Act1이며, 처치/키 미확인을 중앙 복귀로 바꾸지 않는다. 현재 줍기는 열쇠/이스트/오움만 지원하고 보석/공통 룬은 제외하며 나머지 고급 룬 판독은 미지원이다. J 실게임·협곡 마감, 종점 재획득·긴 방의 graph 슬롯 상한·직렬 판독과50ms는 미완료이며 북쪽10연속이나 남쪽 단계가 완료된 것으로 읽지 않는다.

이전 [CPU 중심 실시간 판단](feature/feat-0030-cpu-realtime-control.md)은 현재 Rust 게임 개발의 성능 기준이다. 사용자는 WGC/GPU를 보류하고 조준 대기·읽기→판별·관찰 cadence의 구조 개선과 녹화 병행 감독 게임을 승인했다. 안전성·최신 source·자원 회수 아래 속도 단축을 우선하며 기존 timing/threshold/경로 행동의 적응은 후속이다. [계획](refactoring/refactor-0004-realtime-decision-latency.md)·[SPEC](spec/spec-0030-cpu-realtime-control.md)·[RUN](run/run-20261024-02-cpu-realtime-control.md)이 구현 경계/시도 상태, [EVAL-0033](evaluation/eval-0033-cpu-realtime-control.md)이 현재 source·입력 지연·메모리·배포 검사와50ms 미달 범위를 소유한다. 복구는 TP 없이 기존 north→중앙→Act1→저장 종료 코드로 검증한다. 이전 [Rust 효율 개선](feature/feat-0029-rust-runtime-efficiency.md)의 기준은 [기존 RUN](run/run-20261024-01-rust-runtime-efficiency.md)·[EVAL-0032](evaluation/eval-0032-rust-runtime-efficiency.md)에 freeze했다. 이전 [효율 계획](refactoring/refactor-0003-rust-efficiency.md)은 계산/codec 이력을 유지한다. 아래 초기 포팅 결과를 현재 개선·전체 게임 수용으로 자동 완료하지 않는다.

Rust 포팅은 사용자가 Windows EXE까지의 전체 실행을 위임했다. [PRD-0009](prd/prd-0009-rust-windows-port.md)와 [REFACTOR-0002](refactoring/refactor-0002-rust-port.md)가 범위/동등성 계획을 소유한다. [빌드 기반](run/run-20261022-01-rust-native-build.md)·[설정](run/run-20261022-02-rust-config-contract.md)·[프레임/좌표](run/run-20261022-03-rust-frame-geometry.md)·[독립 캐릭터 정책/상태](run/run-20261022-05-rust-character-policy.md)·[Windows 캡처/입력 adapter](run/run-20261023-01-rust-windows-io.md)·[ORB 배경 이동](run/run-20261023-02-rust-background-motion.md)의 각 한정된 경계를 통과했다. [이미지 연산](run/run-20261022-04-rust-vision-operations.md)의 JPEG 참고 화면 차이는 사용자가 허용했으며 수정 계약의 release 영상 검사15개가 제외 없이 통과했다. [독립 action/방 lifecycle](run/run-20261023-03-rust-actions-lifecycle.md)은 입력 없는/가짜 입력 계약을 통과했고 [Summoner 실행 흐름](run/run-20261023-04-rust-summoner-runtime.md)은252개 release 검사의 저장/가짜 입력 경계를 통과했다. [직접 Rust MPEG-4 녹화](run/run-20261023-07-rust-mpeg4-recording.md)는 독립 재생·크기/화질/시간·오류/정리 검증을 통과했고, [Rust 제어판](run/run-20261023-05-rust-desktop-control.md)은 지정한 fake/native/DPI 경계를, [CLI·도구](run/run-20261023-08-rust-cli-replay.md)는 source/stored/fake와 실제 preview 연결 경계를 통과했다. [최종 배포 run](run/run-20261023-09-rust-windows-distribution.md)에서 FEAT-0028의 전체 회귀·Python/OpenCV 없는 폴더 배포·최종 GUI/CLI/녹화 실행과 제한한 실게임/성능 검사를 완료했다. 실제 북쪽 이동·근접 전투 입력·플랫폼 인식을 확인했고 전체 게임 완주는 미확인이다. 이 포팅 checkpoint의 사람 외관·기본 전환 수용은 미확정이며, 당시 검사 수치·실행 종료 상태·정확한 한계는 [배포 평가](evaluation/eval-0031-rust-windows-distribution.md)가 소유한다. 이후 실제 배포와 효율 개선의 현재 수치·추가 실게임 실패는 EVAL-0032가 소유하며 위 활성 작업을 계속한다. 사용자가 중단 키의 기존 동작 유지를 선택했으므로 다른 키를 누르는 중의 Stop 인식 수정은 적용하지 않는다. F8은 일반 CLI에서 일시정지·재개/Summoner에서 중단, F9는 설정상 중단, F10은 독립 action·Summoner 중단이며 게임 단축키와 충돌할 수 있음을 설명했다. 키 재지정 요청은 없다. 최종 Python/OpenCV 배포 제거는 확정 목표이며 bootstrap EXE를 전체 포팅 완료로 취급하지 않는다. 기존 Summoner 실행 상태와 검증 실적은 아래 소유 문서를 유지한다.

현재 실행 이력과 실패·복구·검사 수치는 [Attempt18](run/run-20261009-07-summoner-live-integration.md#현재-attempt18-3런마다-속도경로-보완)에서 관리한다. GUI 연결/공용 자산 정리의 당시 범위는 같은 run의 [Attempt13](run/run-20261009-07-summoner-live-integration.md#attempt13-gui-자동-버프-조건부-alt와-공용-자산-정리), 현재 실행 절차는 [Summoner 안내](../features/summoner-run.md)를 따른다. 이 인덱스에 같은 실행 로그를 중복 누적하지 않는다.

- [Perceptual Arcane navigation](feature/feat-0013-arcane-perceptual-navigation.md): 현재 우선순위는 새 방 북쪽10회 연속 정상 마감과 성공/실패 합계3회마다 보완하는 개발 검증이다. 빈 끝은 중앙WP, 처치 후에는 일지·붉은 포탈·협곡WP를 거쳐 Act 1→방 종료로 마감한다. 이후 같은 방 북남 조합10회→동쪽 단독10회→북동남→북동남서 순서로 연결하며 방향별 실제 검증과 통합 성공을 구분한다. 지도ON·검토 viewport 카메라·현재 끝/중앙 부위·F2 유지/넓은 조향·탈부착 근접 노바를 사용한다. 일반 지도 선 해석, 모든 variation 정확도, 먼 적 정리 후 원위치 복귀, 실제 처치/회수와 정상 반복은 미완료다. Act 1 정비 세부 동작만 사용자 요청대로 건너뛴다. Act 2 초기화 준비 방은 완료 수에서 제외하며 최종30방 중27방 조건은 유지한다.

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

- [Summoner platform encounter evidence](feature/feat-0014-summoner-platform-encounter-evidence.md): 현재 발판 접근/근접 적 패널 기반 노바와 최대24장 전후 화면을 기록한다. 사용자 확인 종류의 생존/시체 포즈를 사용하고, 생존 관찰은 선택 증거로 두되 같은 방·이동 세대·현재 발판 상대 위치와 연속 두 시체 화면으로 확인하며 처치/전리품 확인 뒤 협곡 마감을 연결했다. [SPEC-0013](spec/spec-0013-summoner-platform-encounter-evidence.md)과 통합 run Attempt18이 경계와 실제 증거를 소유한다. 입력 없는 재생 통과와 실제 자동 처치/획득/새 방 반복 미완료를 구분한다.

- [Live emergency recovery](feature/feat-0015-live-emergency-recovery.md): main216의 마나 부족 시 포션 입력 누락을 보완한다. 현재 보이는 보라 포션 열1회 입력과 새 HUD 회복·실제 벨트 잔량·닫힘을 이동/전투/대기에 연결했으며 관련141개 검사가 통과했다. [SPEC-0014](spec/spec-0014-live-emergency-recovery.md)와 통합 run Attempt10이 실제 재시험 결과를 소유한다. 자동 줍기/재보급/사망 복구는 미구현이다.

## Reference Ownership

Project policy, architecture, developer guidance, config references, and domain documentation retain their local owners. The harness manifest tracks only shared-origin assets, not these product/planning records.
