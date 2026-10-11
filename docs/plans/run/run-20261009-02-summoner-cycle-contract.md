# RUN-20261009-02: 전체 방 흐름의 입력 없는 연결

- ID: `run-20261009-02`; Status: `passed`; Attempt: `1`
- Feature: [FEAT-0010](../feature/feat-0010-summoner-cycle-contract.md); Spec: [SPEC-0009](../spec/spec-0009-summoner-cycle-contract.md)
- Profile: `foundation-contract`; Evaluators: Contract, Functional
- Created / Updated: `2026-10-09`

## Direction And Evidence

사용자의 전체 구현·감독 검증 요청을 계속 진행한다. [앞선 시계/세대 계약](run-20261009-01-runtime-observation-provenance.md)은 141개 회귀로 통과했다. read-only scout가 기존 방 생성/종료와 마을 planner, Summoner 훅을 조사했고 primary가 호출 경로를 검토했다. 기존 run_current_stage는 북쪽에서 끝나며 north_return은 선택 화면으로 종료하는 시험 reset이다. 실제 정상 귀환으로 재사용하지 않는다.

이 increment는 확정한 전체 순서와 긍정 확인·중복 방지·실패 후 반복 금지를 순수 planner에 연결한다. primary가 intent/아키텍처/spec/구현/의미 검토를 소유한다. 이후 사용자가 감독 30방 중 27방 이상 정상 마감과 실패 시 안전 종료 후 사람 확인을 확정했다. planner의 review_required/release는 실제 종료 성공이 아니며 후속 adapter가 확인한다. 포션/TP 최신 조건과 보류한 보급 값·정비 절차는 PRD-0007에 기록했다. 게임 입력은 보내지 않는다.

## Results And Remaining

`cycle.py`는 현재 명령 ID와 이후 frame·시각·세대의 의미 결과를 확인한 뒤 한 번만 다음 명령을 요청한다. 네 날개/중앙 귀환, 첫 보스 발견 뒤 다른 날개 생략, 사망·전리품·Act 1 재정비·선택 복귀 확인을 분리한다. 드롭과 실제 획득 수, 생성 후 실제 Act 1 도착 여부를 별도로 기록하며 실패 뒤 다음 방을 만들지 않는다. 모든 시간 상한과 유한 방 목록은 명시적 입력이고 운영 기본값은 없다.

새 14개 입력 없는 테스트와 기존 회귀 합계 155개가 통과했다. 30방 의미 재생은 중복 결과/31번째 생성 없음과 유한 보유를 검사한 테스트이며 실제 게임 30방 완료가 아니다. 변경 Python 2개 Black/compile와 diff check 종료 0, verifier의 전후 status가 같고 compile 임시 출력은 자동 제거됐다. Black의 sandbox WinError 5는 동일 명령을 일반 환경에서 재실행하여 구분했다. primary가 코드/spec/테스트의 의미를 검토한 [Contract](../evaluation/eval-0019-contract-summoner-cycle.md), [Functional](../evaluation/eval-0020-functional-summoner-cycle.md)은 입력 없는 경계의 PASS다.

기본 GUI 실행은 기존 단계에 그대로 있다. 실제 scene/player feet·근접·도착/사망/획득 생산자, 모든 날개의 이동, 안전 종료·정상 귀환·재정비 adapter, GUI 연결과 실게임 전체 런은 후속이다. 실제 신규 d→F1 검증은 [감독 버프 재확인](run-20261009-03-flash-buff-recheck.md)이 소유한다.
