# EVAL-0018: 실시간 관찰 동시성 검증

- ID: `eval-0018`; Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Run: [RUN-20261009-01](../run/run-20261009-01-runtime-observation-provenance.md); Attempt: `1`
- Feature: [FEAT-0009](../feature/feat-0009-runtime-observation-provenance.md); Spec: [SPEC-0008](../spec/spec-0008-runtime-observation-provenance.md)
- Profile: `foundation-contract`; Evidence Coverage: `complete` — 입력 없는 fake clock/capture 및 실제 worker thread
- Created: `2026-10-09`

| Acceptance | 관찰된 결과 |
| --- | --- |
| OBS-01 | wall-clock 변화에도 동일한 freshness; capture 지연을 포함한 시작 시각 보존; 오류 뒤 capture close; 시계 역행 거부 |
| OBS-02 | 명시적 invalidate/창 영역 변경 뒤 이전 frame/vision/decision·버퍼 무효화 |
| OBS-03 | 이전 세대 capture·느린 worker·판단 완료 거부; 미래·중복·역순 source가 최신 결과를 덮어쓰지 않음 |
| OBS-04 | None 관찰 뒤 이전 결과 복원 거부; 20개 frame publication 뒤 최신 5개만 유지; worker는 backlog 대신 최신 frame 처리 |
| OBS-05 | 새 12개와 기존 전체 회귀 합계 141개 통과 |

named bounded verifier가 고정된 전체 unittest/Black/compile/diff packet을 실행했다. 첫 sandbox 실행의 WinError 5는 일반 환경에서 동일 명령을 재실행하여 구분했다. 전체 unittest 종료 0(141개), 변경 Python 3개 Black check/compile 종료 0, diff check 종료 0이었다. compile 임시 파일은 자동 제거했고 검증 전후 작업 트리가 같았다. primary가 결과 범위와 계약을 별도로 검토했다.

실제 게임은 입력 없이 Flash 91 캐릭터 선택 화면만 확인했다. 이 자료는 실게임 이동·전투·버프 효과·획득·귀환이나 전체 런 통과의 근거가 아니다. 본 기능은 입력 없는 관찰 provenance 계약으로 완료하며 전체 Summoner 구현은 계속 진행한다. Route: `pass`.
