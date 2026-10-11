# EVAL-0024: 자동 버프 갱신 동작

- ID: `eval-0024`; Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Feature: [FEAT-0012](../feature/feat-0012-automatic-buff-renewal.md); Spec: [SPEC-0011](../spec/spec-0011-automatic-buff-renewal.md)
- Run: [RUN-20261009-05](../run/run-20261009-05-automatic-buff-renewal.md); Attempt: `1`
- Profile: `backend-product`; Evidence Coverage: `partial` — 합성 scene/가짜 시계/dry backend; Created: `2026-10-09`

신규 14개 테스트는 실제 프로필의 최초 순서와 두 번의 140초 갱신, 시전 요청 기준 타이머, 시한의 유지 키 해제, 새 화면 없는 전송 금지, 안전 확보 대기와 상한, 간격 중 적/미관찰/벨트 변화, 회복 1회 선점과 0개 종료 우선, pause/focus/stop, 같은 프레임 확인과 불완전 확인 거부, 실행/확인 시간 초과, 입력 예외, 중복/미래/다른 캐릭터와 시계 역행, backend 응답 이후 간격, 전송 도중 큐에 들어온 프레임을 검증했다. `validated_vision` 표기는 주입한 합성 확인의 API 출처이며 실제 판독기를 시험한 것이 아니다.

named bounded verifier가 고정 명령을 실행했다. 전체 unittest 174개 OK, Black 3개 파일 unchanged, `TemporaryDirectory` 안의 compile 3개 파일 및 diff check 종료 0. 최초 sandbox unittest는 Ran 50/errors 8의 알려진 Python 3.7 Path.resolve WinError5였고 Black도 같은 환경 오류였다. 동일 명령의 권한 상승 재실행에서 통과했다. 검증 전후 Git 상태는 같고 임시 compile 산출물은 자동 정리됐다. primary가 최종 코드와 수용 범위를 별도로 검토했다.

실제 게임 입력·140초 감독 갱신·자동 clear/효과 판독·네 방향 완주·30방 수용은 수행하지 않았다. AB-05에 따라 공통 연결의 입력 없는 검증은 PASS이며, 위 증거 부재는 live 활성화/전체 기능 수용에 blocking이다. Route: `pass` — 실제 producer 준비와 감독 검증 계속.
