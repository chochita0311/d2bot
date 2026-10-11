# EVAL-0021: 포션 회수 종류 계약

- ID: `eval-0021`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Feature: [FEAT-0011](../feature/feat-0011-potion-pickup-policy.md); Spec: [SPEC-0010](../spec/spec-0010-potion-pickup-policy.md)
- Run: [RUN-20261009-04](../run/run-20261009-04-potion-pickup-policy.md); Attempt: `1`
- Profile: `foundation-contract`; Evidence Coverage: `complete` — 설정/입력 없는 판단; Created: `2026-10-09`

primary가 사용자 조건과 실제 schema·판단·테스트를 대조했다. 선호/대체 종류는 캐릭터 설정이며 fallback 종류와 정수 임계값을 함께 요구한다. 미설정 프로필에는 Flash 값을 추측하지 않는다. 확인한 같은 맥락 벨트의 최신 잔량을 사용하며 현재 6개 이하에서만 작은 활력을 반환한다. 7개가 되면 중단하며, 12개까지 이어지는 보충 전투 모드와 섞지 않는다. 고갈 종료·가득 참·미확인·오래된/미래 stamp는 허용 종류 없음이다.

Flash 4열 목표 4장은 실제 관찰과 결손을 계산한다. 다른 null 목표의 첫 관찰 baseline은 유지한다. aggregate `purple_potion`은 회복 잔량 계약이고 세부 회수 종류/현재 관찰 지원 범위와 다르다. 종류 허용은 화면 식별/안전 접근/획득이나 입력 허가가 아니며 문서에 후속 소비자 의무를 남겼다.

전체 160개 회귀, 변경 2개 파일의 Black/compile, diff check 통과. 실제 작은 포션 자산/라벨/획득과 미확정 정비값은 포함하지 않는다. 차단 finding 없음. Route: `pass`.
