# EVAL-0005: 마을 반복의 관찰과 상태 설명

- Status: `complete`
- Evaluator Type / Result: `ux-heuristic` / `PASS WITH SUGGESTIONS`
- Evidence Coverage: `partial`
- Run / Attempt: [RUN-20261004-02](../run/run-20261004-02-supervised-town-room-loop.md), GUI 관찰 및 `192301`/`192534`
- Feature / Spec: [FEAT-0002](../feature/feat-0002-supervised-town-room-loop.md), [SPEC-0001](../spec/spec-0001-supervised-town-room-loop.md)
- Profile: `backend-product` + GUI 실행/상태 확인
- Created: `2026-10-04`

## Checks And Evidence

GUI의 `Live town loop (1–3 runs)`, Repeat Count, Difficulty, Start/Stop을 직접 확인했다. 선택 컨트롤이 기존 창 안에 보이며 빈 횟수의 무한 town 실행은 거절한다. CLI는 단계와 완료 횟수·증거 폴더를 출력한다. 오류 메시지가 상태 갱신으로 숨겨지지 않도록 유지했다. 최종 실제 입력 검증은 CLI로 했으며 GUI 버튼을 통한 3회 재실행으로 확장하지 않았다.

홀드 중 발판 인식/해제로 이동의 고정 대기를 줄였다. 관찰 중앙값 93–94ms, 캡처 완료→입력 시작 중앙값 6.55–7.07ms를 측정했다. 가장 긴 관찰 간격과 PNG/입력 비용도 run에 남겨 항상 같은 속도로 오해하지 않도록 했다. 사용자 마우스 조작이라고 오판했던 설명을 WP의 게임 커서 이동 관측으로 정정했다.

## Suggestion And Evidence Gaps

지연 원인별 분포와 더 긴 반복 표본, 자연 NPC 복구를 후속 관찰에서 보완한다. 현재 유한 마을 루프 사용의 non-blocking 제안이며 전투/무감독 준비 판정과 구분한다. [HEUR-0001](../heuristic/heur-0001-town-loop-observation-latency.md)이 제안을 소유한다. `pass`: 이번 결과를 보고하고 다음 단계에서 제안을 검토한다.
