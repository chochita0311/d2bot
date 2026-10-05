# EVAL-0006: 캐릭터 생존·버프 입력 없는 동작

## Metadata

- ID: `eval-0006`
- Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Run ID: `run-20261005-01`; Attempt: `1`
- Feature: [FEAT-0003](../feature/feat-0003-character-survival-state.md)
- Spec: [SPEC-0002](../spec/spec-0002-character-survival-state.md)
- Execution Profile: `foundation-contract`
- Evidence Coverage: `complete` — 입력 없는 판단과 기존 마을 회귀
- Created: `2026-10-05`

## Checks And Evidence

프로젝트 venv에서 `python -B -m unittest discover -s tests -v`: **34/34**, 새 생존/설정 18개와 기존 마을 16개. 최초 통과 뒤 Black 포맷 후 같은 검증이 통과했다. 모든 검증은 합성/설정 입력이며 게임 조작을 실행하지 않았다.

새 테스트는 체력 50%·마나 10%의 엄격한 미만 조건/동시 조건의 한 포션 요청, 7/6/1/0개·종료 선점, 140/150초·실패 시전·일시정지·다른 시전 시각, 초기/갱신 시 마을/적/미확인 거부, 소모/획득·중복·틀린 종류·용량 충돌·수동 변경·초기 TP 잔량을 다룬다. 다른 방/캐릭터·과거 sequence·시간 역전·화면/벨트 나이·미설정 정책과 상태 복사도 확인했다.

기존 마을 회귀는 상태 전환·중단/홀드 해제·제한 재시도·NPC 대화·종료 정리 등을 유지했다. 이번 통과를 새 실게임 왕복 성공이나 생존 대응 성공으로 계산하지 않는다.

## Evidence Gaps And Route

실제 HUD/벨트 인식, 버프 효과, 포션 사용/스크롤 획득, 필드 안전 확보/고갈 종료·보충 전투·장시간 런은 미검증이다. FEAT-0003에 비차단이며 이후 관찰·실제 입력 기능은 별도 자료와 검증이 필요하다. 합성 테스트의 시간 상한은 실게임 수치가 아니다.

차단 finding 없음. Route: `pass`; 후속 기능 실행을 승인하지 않는다.
