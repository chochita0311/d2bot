# EVAL-0003: 마을 방 반복의 실제 동작

- Status: `complete`
- Evaluator Type / Result: `functional` / `PASS`
- Evidence Coverage: `partial`
- Run / Attempt: [RUN-20261004-02](../run/run-20261004-02-supervised-town-room-loop.md), `192301` 및 수정 후 `192534`
- Feature / Spec: [FEAT-0002](../feature/feat-0002-supervised-town-room-loop.md), [SPEC-0001](../spec/spec-0001-supervised-town-room-loop.md)
- Profile: `backend-product`
- Created: `2026-10-04`

## Checks And Evidence

실제 Flash/Hell에서 선택, 생성, Act 1 실제 WP 발판, Act 2 마을 도착, Act 1 복귀, 저장 종료, 다음 방 생성을 캡처와 직접 관찰로 확인했다. 입력 전 프레임과 다음 상태의 긍정 증거를 구분했다. `192301`은 세 배치에서 3/3, `192534`는 정리 수정 뒤 1/1 및 CLI exit 0이다. events/result/timings와 최종 native 화면은 당시 ignored `recordings/summoner/evidence/`에서 검토한 역사적 출처다. 세션 종료 후 유지한 자산·원시 자료 정리 결과는 [run](../run/run-20261004-02-supervised-town-room-loop.md#session-close)이 소유한다.

초기 실패와 중단도 보존했으며 상세 결과는 run의 시도 표를 따른다. `192301`의 완료 뒤 캡처 정리 오류는 게임 루프 성공과 프로세스 종료 실패로 분리했다. NPC 실화면은 인식/계획 재생 점수 0.989이며 직접 Esc 해제를 관찰했다. 확인한 마을 화면은 HP/MP 전량과 용병 생존 상태였다.

## Findings And Corrections

해소한 implementation bug: 프로젝트/native 캡처 변형 불일치, Act 1 탐색 순서, WP 게임 커서 이동 오판, Act 2 발판 클릭/재접근, 완료 후 캡처 정리. [FIX-0001](../fix/fix-0001-town-room-loop-calibration-and-control.md)이 수정 근거를 소유한다. 최종 마을 왕복/종료에서 열린 실패는 없다.

## Evidence Gaps And Route

TOWN-05의 자동 이동→NPC 대화→복구→왕복 완료 통합 사례는 미관측이다. 세 배치 각각의 많은 반복 표본도 없다. 현재 finite loop 결과 판정에는 non-blocking이지만 **NPC 복구 일반성과 전체 TOWN-05의 최종 수용**은 미확정이다. 전투/무감독 확대로 이 PASS를 사용하지 않는다. 아케인·생존·버프·아이템/보급은 이번 실제 검증 범위에 포함되지 않는다.

`pass`: 검증한 유한 마을 루프 결과를 사람에게 보고. Feature의 최종 수용은 별도이며 다음 감독 검증에서 NPC 개입 사례를 수집한다.
