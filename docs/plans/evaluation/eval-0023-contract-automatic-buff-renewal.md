# EVAL-0023: 자동 버프 갱신 연결 계약

- ID: `eval-0023`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Feature: [FEAT-0012](../feature/feat-0012-automatic-buff-renewal.md); Spec: [SPEC-0011](../spec/spec-0011-automatic-buff-renewal.md)
- Run: [RUN-20261009-05](../run/run-20261009-05-automatic-buff-renewal.md); Attempt: `1`
- Profile: `backend-product`; Evidence Coverage: `partial` — 공통 실행 계약/주입형 관찰; Created: `2026-10-09`

primary가 입력 소유·시계·관찰/응답 순번·효과 확인 경계를 검토했다. `create_field_runtime`은 동일 상태/시계/유효 시간으로 버프 소비자를 연결하고 기본 dry backend를 유지한다. 최신 mailbox에 같은 stamp의 확인만 넣을 수 있다. 입력 전 유지 키 해제·포커스/중단/나이 검사를 수행하며, 각 pump는 최대 한 토큰을 보낸다. 키 간격은 실제 backend 응답부터 계산한다. 응답보다 오래된 큐 프레임은 다음 입력이나 확인에 사용하지 않는다.

타이머는 기존 시전 요청 직전 관찰 기준이며 효과 검토 시점으로 연장하지 않는다. 안전/실행/확인 대기에는 명시적 양수 상한을 요구한다. 적 접근·관찰 중단·pause·포커스 상실·예외·상한 초과는 검토 사유로 고정한다. 부분 시전 뒤 자동 w 복원이나 자동 재시작을 추가하지 않았다. 회복/확인된 고갈이 먼저이며, 회복·종료 요청은 실제 완료와 구분한다.

전체 174개 회귀와 변경 3개 파일 Black/compile, diff check가 통과했다. 코드 계약의 차단 finding은 없다. 실제 자동 주변 안전·전체 효과 생산자와 감독 140초 갱신은 미검증이다. AB-05에 따라 이 간극은 입력 없는 연결 계약의 PASS를 막지 않지만 live 활성화와 전체 Summoner 수용을 막는다. Route: `pass` — 후속 producer 검증.
