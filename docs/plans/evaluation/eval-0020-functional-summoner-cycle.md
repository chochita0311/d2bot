# EVAL-0020: Summoner 전체 순서 의미 재생

- ID: `eval-0020`; Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Feature: [FEAT-0010](../feature/feat-0010-summoner-cycle-contract.md); Spec: [SPEC-0009](../spec/spec-0009-summoner-cycle-contract.md)
- Run: [RUN-20261009-02](../run/run-20261009-02-summoner-cycle-contract.md); Attempt: `1`
- Profile: `foundation-contract`; Evidence Coverage: `complete` — 입력 없는 의미 관찰
- Created: `2026-10-09`

| Contract | 결과 |
| --- | --- |
| CYCLE-01 | 시작 화면 미확인·다른 단계 확인·누락 관찰은 명령 생성/반복 없음; Act 1→Arcane→준비 순서 보존 |
| CYCLE-02 | 첫/마지막 날개 보스, 각 빈 날개 뒤 중앙 귀환; 모든 날개 부재는 정상 성공 대신 검토 |
| CYCLE-03 | 보스 사망/전리품 미확인에서 다음 단계 차단; 미회수 열쇠는 검토 요청 |
| CYCLE-04 | 0/복수 열쇠 드롭·획득 별도 기록; 정비/종료 뒤 한 번만 결과 생성; 30방 재생 후 31번째 생성 없음 |
| CYCLE-05 | 이전 명령/역순 frame·세대/이전 시각은 진행 차단; 잘못된 맥락/미래·오래된 화면/stop/focus/시한/clock 역행은 종료 요청과 1회 실패 결과 |

새 14개와 기존 회귀를 합해 전체 155개 unittest 종료 0. named bounded verifier는 같은 고정 packet의 Black 2파일/compile 2파일/diff check 종료 0과 status 전후 동일을 보고했다. Black sandbox의 Path.resolve WinError 5는 동일 명령의 일반 환경 재실행과 구분했다. compile은 자동 정리되는 task 임시 디렉터리만 사용했다. primary가 의미 범위를 따로 검토했다.

30방 재생은 합성 의미 event의 순서 테스트이며 실제 30방 감독 실행이나 자동 화면 판독·이동·처치·획득·안전 종료 성공의 근거가 아니다. 그 전체 수용은 남은 producer/adapter를 검증한 후 진행한다. Route: `pass`.
