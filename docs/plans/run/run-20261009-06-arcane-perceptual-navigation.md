# RUN-20261009-06: 네 날개 상황 인지 이동

- ID: `run-20261009-06`; Status: `in-loop`; Attempt: `1`
- Feature: [FEAT-0013](../feature/feat-0013-arcane-perceptual-navigation.md)
- Spec: [SPEC-0012](../spec/spec-0012-arcane-perceptual-navigation.md)
- Profile: `backend-product`; Evaluators: Contract, Functional; Created: `2026-10-09`

Orchestrator가 사용자의 자동 네 방향 목표와 현재 화면 인지·텔레포트 지연·아케인 호버 처리 범위를 확정했다. NPC 호버는 마을 전용이라는 추가 답변을 반영했다. Builder는 현재 지형/이동 확인과 유한 탐색 세션을 연결한다. 기존 북쪽의 끝 감지는 전체 stop을 켜며, 동/남/서와 실제 중앙 귀환은 metadata-only다. 약230ms 조향 sleep과 최대1.8초의 slow 관찰을 빠른 이동의 최신 안전 증거로 재사용하지 않는다.

기존 summoner_north 자산은 실제 픽셀 확인에서 클래식 모드 모습이며 현재 remastered 판독의 정답으로 쓰지 않는다. 현재 실제 지역 장면과 HUD 자산은 크기/지원 조건 안에서 참고한다. 기존 버프의 입력 없는174개 통과와 실제140초 미검증, 전체30방 수용 미완료는 유지한다.

## Implemented And Checked

[탐색 안내](../../features/arcane-navigation.md)가 현재 API/운영 조정값의 owner다. `navigation.py`는 명시적 지역 팔레트·격자/연결 성분·굴곡·발밑 가림 범위와 배경 ORB/RANSAC 이동을 처리한다. `search.py`는 한 번 시전 뒤 새 세대의 이동 확인/직업 참고 하한을 기다리고, 확인한 경로 흔적으로 귀환하며 중앙 도착 확인 뒤에만 다음 날개를 시작한다. NPC는 Arcane 호버 종류에서 거부한다. 보스 발견과 처치, 날개 완료와 전체 사용자 stop을 구분한다.

`search_runtime.py`는 실제 `ScreenCapture`/동일 프레임 HUD/명시적 scene 생산자를 공통 생존·버프 runtime에 연결하는 API다. 캡처 시작의 단조 시각, 현재 창 원점/크기와 좌표 변환을 검사한다. 공통 actor lock 안에서 최신 판단이 목적지만 기다릴 때 외부 이동을 허용하고 한 번 press한다. 실제 backend에는 독립 watchdog이 필요하다. 자동 주변 안전/버프 효과/발 위치/끝/중앙의 실화면 생산자와 기존 GUI 연결은 아직 없다.

신규 입력 없는24개 테스트와 기존174개를 합한 전체 **198개 OK**. 네 방향/가변 길이 굴곡/공백·분리 발판/발밑 작은 조각/호버·NPC 거부, 배경 이동/미검출 재획득, 새 세대/지연/도착 미확인, 귀환 후 중앙 확인 대기, 전투 중 보스 인계, HUD/CV 지연, 창 이동, 동일 stamp/공통 선점과 독립 watchdog 경계를 검증했다. 직사각형 갈림·합류의 실지도 완주를 검증한 것은 아니다.

named bounded verifier의 최종 고정 검증: 전체 unittest198개, Black8개 unchanged, 명시적 cfile/TemporaryDirectory의 compile8개, diff check 모두 종료0. unittest/Black의 최초 sandbox는 알려진 Python3.7 `Path.resolve` WinError5였고 동일 명령의 require_escalated 재실행에서 통과했다. compile 임시물은 자동 정리됐고 검증 전후 Git 상태가 같았다. primary가 의미/제품 수용 범위를 별도로 검토했다.

## Actual Archived Replay

보관된 북쪽 두 레이아웃8개 crop을 입력 없이 재생했다. legacy void의 중간 회색이 발판을 지우는 문제, 어두운 벽/효과 조각, 발밑 가림으로 가까운 한 조각에 묶이는 문제를 확인했다. 현재 지역 manifest는 이미 gray125를 void에서 제외한다. 이를 재사용하며 실험의 밝은 바닥 후보는 별도 [보정 파일](../../../assets/regions/arcane-sanctuary/remastered-ko-1922x1140/navigation-calibration.json)에 명시했다. 공통 legacy 팔레트의 적합성을 가정하거나 전역 변경하지 않았다.

초기 `(960,570)`은 진단 추정이었다. 기존 [사람 기준점](../../features/encounter-observation.md)의 alive `(961,620)`으로 출처를 바로잡았다. 다른 자세에 고정값을 사용하는 것은 진단 대용이며 실제 발 위치 생산자가 아니다. 최종 보정의 재생은8건/입력0회, 조준 후보7건/`no_observed_progress` 보류1건이다. 후보7건을 유효 이동7회로 세지 않는다. 실제 계단에서는 앞쪽 단상 모서리를, 신단 장면에서는 진행할 계단 대신 같은 단상 쪽을 고를 수 있다. PN-01/03 실제 기능 수용을 막는 판독 문제다.

최종 primary 재생의 CV 시간은 첫 장면235ms, 이후46–63ms였다. 앞선 별도 verifier 표본은31/47/750ms(min/median/max)였다. 측정은 crop 기반의 후보/배경 처리이며 캡처·HUD·입력의 전체 지연 상한이나 live 성능 보장이 아니다. `.py` 직접 경로 실행은 import 오류였고 `python -m diablo2.tools.arcane_navigation_replay` 모듈 실행으로 재현한다.

## Live Boundary And Next Work

이 절은 RUN-06 시점의 실패/미확인 상태다. 이후 화면 복구와 실제 프로그램 시험·코드 수정은 [RUN-20261009-07](run-20261009-07-summoner-live-integration.md)이 소유한다. 마을·성역 왕복 각1회가 통과했으며 진입 전용 커서 오류 수정 후 실제 재시험과 필드 이동은 남아 있다. 사용자는 Act 1 정비 세부 동작을 지금 건너뛰도록 답했다.

이번 구현에서 필드 이동/전투 입력은 보내지 않았다. 실제 창 캡처가 검은 화면이고 활성화는 `GetCursorPos` 접근 거부0x80070005였다. 창 재선택 후에도 같아 화면 복구를 요청했다. 사용자는 화면 보호기였을 가능성을 말하며 계속 진행을 지시했다. 그 뒤 새 화면 요청은 **물리 Escape로 Computer Use가 중지됐다는 결과**를 반환했다. 해당 turn의 Computer Use를 중지했고 현재 게임 위치/상태는 새 화면으로 확인하지 못했다. 이전 선택 화면 상태를 현재 확인으로 재사용하지 않는다.

후속은 실제 발 위치/계단·가림 자료와 끝/중앙/안전/버프 효과 생산자 보완→GUI 경로 연결→제한된 감독 이동/귀환→공포의 영역 포함30방 중27방 이상 정상 마감이다. 실패 시 안전 종료 후 사람 확인, 사망 후 자동 재정비 없음, Act1 정비 절차는 사람과 정한 뒤 공통 layer/per-Act adapter로 구현한다. 전체 목표는 계속 미완료다.

[Contract](../evaluation/eval-0025-contract-arcane-navigation.md)는 입력 없는 연결 계약 PASS/partial, [Functional](../evaluation/eval-0026-functional-arcane-navigation.md)은 제품 수용 FAIL/partial로 구분한다. Route: `spec-review` — 지형/발 위치·실제 scene 증거 보완. 개발 진단 그림은 재생 도구로 다시 생성 가능하며 소유가 확인된 이번 task 디렉터리를 정리한다. durable 참조는 그 임시 경로에 의존하지 않는다.
