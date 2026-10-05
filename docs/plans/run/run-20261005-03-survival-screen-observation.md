# RUN-20261005-03: 생존 화면 관찰

## Metadata

- ID: `run-20261005-03`; Status: `passed`; Attempt: `1`
- Feature: [FEAT-0005](../feature/feat-0005-survival-screen-observation.md)
- Parent PRD: [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Active Spec: [SPEC-0004](../spec/spec-0004-survival-screen-observation.md)
- Profile: `backend-product`; Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`

## Approval And Readiness

사용자가 화면 테스트/캡처와 후속 구현을 요청했다. 필요한 조작을 직접 수행하고 필드에서는 새 방으로 마을에 들어가라는 추가 지시를 반영했다. PRD-0005 실제 이동에 앞서 PRD-0004의 화면 관찰을 이어간다. 이전 FEAT-0003/0004 결과를 화면 검증으로 소급하지 않는다.

primary가 요구·소유권·설계·구현·의미 검토를 소유한다. named evidence scout는 기존 자료/호출 계약만 조사했다. 마을 준비는 computer-use 스킬의 실제 관찰 후 한 입력/재관찰로 수행하며 필드 버프/이동을 실행하지 않는다. 제품 CLI는 입력 없는 capture/판독이다.

## Attempts And Observations

- 선택 화면을 프로젝트 API로 실제 캡처했다. 사용자 추가 승인 후 선택된 Flash를 플레이/지옥으로 새 방에 진입시키고 Act 1 마을을 확인했다. backtick으로 벨트를 펼쳤다.
- 육안 기준 생명력 985/985, 마나 1075/1075, purple potion 12개·TP scroll 4개. 현재 창 native capture는 1922×1140이며 UI 조작 도구의 스크린샷은 1267×754다. 두 좌표를 혼용하지 않는다.
- 기존 전체 HUD에는 참고 값/아이콘이 있지만 전용 판독기는 없었다. Windows OCR probe의 Korean 결과는 숫자/slash가 틀려 채택하지 않는다. 새 shared visual 자료로 엄격한 OpenCV 판독을 구현한다.
- 사용자 추가 지시대로 버프/장비의 최대 생명력·마나 변화는 같은 프레임의 current/max 비율로 반영한다. 이전 최대값을 가져오지 않고 미확인은 그대로 보류한다. 마을에서 버프를 시전해 확인한 것으로 기록하지 않는다.
- 큰 완전 활력 포션 하나를 인벤토리로 옮겨 실제 11개·upper empty 자료를 확보하고 벨트로 복구했다. 초기 drag 시도는 커서에 아이템이 남아 성공으로 세지 않았고 관찰 후 click pickup/placement로 확인했다. 최종 상태는 안전한 Act 1 마을, 벨트 펼침, 포션 12개·TP 4개, 패널 닫힘이다. 포션 사용·아이템 폐기·장비 변경은 없었다.
- 초기 belt upper/base 자료는 중간 행을 보류했다. 실제 행별 외형 자료를 추가하고 임계값을 낮춰 강제 판독하지 않았다. 빈 슬롯은 한 upper 위치만 실제 확인했다.
- 초기 테스트 fixture는 anchor까지 가린 slot occlusion, global imread mock이 자료 로드까지 바꾼 문제, 얇은 glyph를 두 번 resize한 합성 왜곡으로 실패했다. fixture를 관찰 대상에 맞게 고치고 실제 slash/4 crop를 사용했다. 제품의 모호함 거부 조건을 완화하지 않았다. 최종 검증은 모두 통과했다.
- 캡처/처리 구간은 `perf_counter`로 기록한다. 앞선 저해상도 monotonic 측정의 0ms 판독값을 성능 증거로 쓰지 않는다. live 진단은 최소화/숨김/누락 창을 거부하고 finally에서 capture를 정리한다.

## Evaluation Coverage

- [Contract 평가](../evaluation/eval-0009-contract-survival-screen-observation.md): PASS, 승인한 제한 배치의 관찰 계약 complete.
- [Functional 평가](../evaluation/eval-0010-functional-survival-screen-observation.md): PASS, 실제 마을 판독/진단·합성 경계/회귀 complete.
- 실제 HUD crop 4개: full 12/4, 포션 하나 이동한 11/4, closed(잔량 미확인), restored 12/4. 모두 생명력 985/985·마나 1075/1075. 원상 복구 후 live 3프레임에서도 동일 값/잔량과 `hold / buff_requires_field`, 입력 0을 확인했다.
- 최종 3프레임의 캡처 36.96–46.84ms·판독 3.15–3.64ms는 한 마을의 짧은 호출 구간 관찰값이며 분위수/비상 입력 지연/장시간 성능 보증이 아니다.
- 전체 테스트 **60/60**(화면 13·시전 13·생존 18·기존 마을 16), 새 Python 4개 compile, 3개 Black check 통과. named verifier가 고정 명령을 실행했고 primary가 의미와 실제 대조를 검토했다.
- 관련 Markdown 14개/Python 4개의 strict UTF-8·Unicode 범위, 로컬 링크 156개·앵커 14개와 `git diff --check`도 통과했다. 한글을 실제 Unicode로 유지했다.
- 실제/합성을 구분한다. 최대값 변화와 50%/10%·6/0개는 합성 경계이며 실제 버프 시전·저체력·고갈 종료·필드 생존의 성공이 아니다. 한 방의 crop/인접 live 자료를 독립 방이나 E2 전체 목표로 세지 않는다.

## Integration And Human Review

관찰기는 공통 시각 자료와 개인 정책을 나누고 기존 capture/입력 없는 생존 계약만 소비한다. 독립 CLI는 기존 GUI/서모너 executor의 입력을 바꾸지 않는다. field/clear·버프 효과/시전 시각·포션 소비/획득/회복·종료 확인과 안전 입력 선점은 후속이다. 사람의 결과 수용은 아직 기록하지 않는다. 다음 실제 제어 전 해당 자료와 runtime 지연/중단 조건을 확인해야 한다.

## Session Artifacts

task 소유 bounded 폴더의 원본/diagnostics를 사용했다. 필요한 이름 없는 label/glyph/item·HUD crop는 `assets/ui/survival`에, 결과 요약은 이 run/평가에 보존한다. 자료는 전체 새 화면이나 개인 이름을 포함하지 않는다. 검증 프로세스 종료 뒤 정확한 workspace 내부 scratch 경로와 소유권을 확인해 제거했다. runtime/test에는 임시 경로 의존이 없다.
