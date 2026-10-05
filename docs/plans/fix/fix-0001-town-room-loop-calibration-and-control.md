# FIX-0001: 마을 인식과 입력 보정

- Status: `complete`
- Run / Attempts: [RUN-20261004-02](../run/run-20261004-02-supervised-town-room-loop.md), `180854`–`192534`
- Feature / Spec: [FEAT-0002](../feature/feat-0002-supervised-town-room-loop.md), [SPEC-0001](../spec/spec-0001-supervised-town-room-loop.md)
- Profile: `backend-product`
- Input reports: [Functional](../evaluation/eval-0003-functional-town-room-loop.md), [Contract](../evaluation/eval-0004-contract-town-room-loop.md)
- Created / Updated: `2026-10-04`

## Corrections

| 관측된 결함 | 수정과 재확인 |
| --- | --- |
| Native 템플릿이 실제 캡처 글자/테두리와 다름 | 사람 검토한 실제 캡처 변형 추가. 선택/난이도/HUD/메뉴 확인 뒤 왕복. |
| 오른쪽/위쪽만 탐색하여 다른 배치를 놓침 | 텐트 단서, 제한 탐색 순서, 홀드 중 관찰/재인식. 세 배치 실화면 왕복. |
| WP 개폐의 게임 커서 이동을 중단으로 판정 | 예상 변위+긍정 메뉴/도착 증거로만 인정; 최대 0.45초 관찰 보류. 계약 테스트와 실제 메뉴. |
| Act 2 고정 좌표/발판 밖 클릭 | 지형+푸른 효과로 최신 위치 계산, 직접 성공한 발판 중앙 보정, 세 번 재접근 상한. 3회 및 최종 1회 성공. |
| 종료 뒤 없는 capture.close 호출로 실행 플래그 고정 | ScreenCapture MSS close 추가, 영상 해제와 구분, 정리 실패에도 나머지 해제/플래그 정리. 두 회귀 테스트 및 CLI exit 0. |

NPC 취소 메뉴의 홀드 해제→Esc→재관찰을 구현했다. 실화면 재생과 직접 Esc, 합성 계약까지 확인했고 자연 개입 자동 통합 표본은 남는다.

## Impact And Route

유한 GUI/CLI 경로와 planner/evidence 계약을 추가했다. 기존 room-only/Summoner 기본 호출·난이도 전달은 소스 검토했고 사용자 변경을 보존했다. `re-evaluate`를 완료하여 결과와 남은 검증은 연결된 평가/run이 소유한다. 유지한 private 인식 자산과 세션 자료 정리 결과는 [run](../run/run-20261004-02-supervised-town-room-loop.md#session-close)을 따른다.
