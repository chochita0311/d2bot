# RUN-20261009-01: 실시간 관찰 시각·이동 세대

- ID: `run-20261009-01`; Status: `passed`; Attempt: `1`
- Feature: [FEAT-0009](../feature/feat-0009-runtime-observation-provenance.md)
- PRD: [PRD-0008](../prd/prd-0008-realtime-runtime-and-recording-retention.md)
- Spec: [SPEC-0008](../spec/spec-0008-runtime-observation-provenance.md)
- Profile: `foundation-contract`; Evaluators: Contract, Functional
- Created / Updated: `2026-10-09`

## Direction

사용자는 전체 Summoner 요구사항 확정·구현·감독 검증을 요청했다. 최신 Flash 순서는 w/a/a/s/d/F1/w이며 앞선 변경의 입력 없는 33개 회귀가 통과했다. 전체 런의 첫 수용 표본 수와 진행 실패 후 다음 방 정책은 사용자 답변을 기다린다. 이 값과 무관한 단일 시계/관찰 세대 계약을 먼저 실행한다.

기존 공통 제어는 monotonic이고 북쪽 runtime은 일반 시각이었다. 이전 RUN-20261005-06의 주입형/짧은 실제 입력 통과를 자동 생산자·전체 런 통과로 확대하지 않는다. 이번 run은 Orchestrator→Spec→Builder→Contract→Functional 순서이며 primary가 아키텍처·승인·최종 의미 검토를 소유한다. named read-only scout는 기존 자산/생산자 부재만 조사했다.

## Current Evidence

computer-use로 Flash 91의 온라인 캐릭터 선택 화면을 입력 없이 확인했다. 게임 상태 변경이나 필드 이동은 수행하지 않았다. 기존 지역 자산은 Ghoul Lord/Hell Clan 외형과 제한된 사체/신단 음성 사례이며 플레이어 발·자동 안전·실제 도착·개체 사망 연결은 여전히 필요하다.

## Results And Remaining

`realtime.py`가 capture 시작 시각·인식 완료·판단·대기/종료에 같은 주입형 단조 시계를 사용한다. 원본 wall-clock은 별도 필드다. 명시적 invalidate와 창 영역 변경은 세대/캐시를 지우고, 이전 세대에서 늦게 끝난 capture/vision/decision publication을 거부한다. 북쪽 소비자는 세대 변경 때 캐시와 유지 입력을 해제하며 행동 직전 현재 세대를 확인한다.

새 입력 없는 테스트 12개와 기존 회귀를 합한 141개가 통과했다. 변경 Python 3개의 Black check/compile, `git diff --check`도 종료 0이었다. 최초 sandbox unittest는 `Path.resolve()`의 WinError 5로 실패했으며 동일한 입력 없는 전체 검증을 승인된 일반 환경에서 재실행하여 통과했다. compile 출력은 자동 정리되는 임시 디렉터리였고 별도 task scratch를 남기지 않았다. verifier의 상태 전후 비교에서 검증으로 생긴 추가 파일은 없었다.

primary는 코드·생산자/소비자·테스트·spec의 의미를 직접 검토했다. [Contract](../evaluation/eval-0017-contract-runtime-observation-provenance.md), [Functional](../evaluation/eval-0018-functional-runtime-observation-provenance.md) 결과는 승인한 입력 없는 경계의 `PASS`다. 실제 입력이나 전체 런 검증으로 해석하지 않는다.

전체 목표는 계속 진행한다. writer/retention, 실제 scene/도착/안전 생산자, 버프·포션·종료 소비자 통합과 전체 Summoner 런은 미완료다. 현재 기본 실행은 북쪽 이동에서 끝나고, north_return은 방 종료용 테스트 reset이다. 정상 귀환·네 방향 탐색·처치/획득 확인과 제한된 반복을 추가해야 한다. 이후 사용자가 감독 30방 중 27방 이상 정상 마감과 실패 시 안전 종료 후 사람 확인을 확정했다. [후속 방 제어 run](run-20261009-02-summoner-cycle-contract.md)이 이 방향을 연결한다.
