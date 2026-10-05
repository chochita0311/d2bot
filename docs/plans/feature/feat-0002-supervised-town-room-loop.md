# FEAT-0002: 감독 하의 마을 방 반복

## Metadata

- ID: `feat-0002`
- Status: `approved`
- Type: `product`
- Surface / Execution Profile: Python 화면 인식·입력 세션 / `backend-product`
- Parent PRD: [PRD-0003](../prd/prd-0003-summoner-room-lifecycle.md)
- Approval: 2026-10-04 사용자의 “방 생성 종료부터 자동화…액트2…액트1…루프 테스트” 요청. 구현 선택과 필요한 캡처도 위임됨.

## Boundary

Flash가 선택된 온라인 화면에서 Hell 방을 만들고 Act 1 웨이포인트로 Act 2 마을을 방문한다. 런 완료를 가정하여 Act 1로 돌아와 종료하고 새 방을 만든다. 먼저 1회, 통과 후 짧은 유한 반복으로 검증한다. 아케인 입구는 별도 도착·안전 복귀 검증을 갖춘 후속 시도로 구분한다. 전투·날개 탐색·아이템 처분·보급은 제외한다.

기존 `ScreenCapture`, `CaptureConfig`, `RunLifecycleSession`의 화면/입력 소유권을 재사용한다. 새 공유 패키지 계약이나 별도 기반 리팩터를 만들지 않는다. 화면 기준이 맞지 않으면 최신 캡처로 필요한 작은 템플릿을 만들고, 원본은 개발 세션의 검토 동안 ignored `recordings/summoner/evidence/<session-id>/`에 둔다. 세션 종료 시 [개발자료 정리 기준](../../project/developer-guide.md#evidence-handling)에 따라 필요한 인식 자산을 `assets/`에 선별하고 불필요한 원본·진단 자료를 제거한다.

## Acceptance Contract

| ID | 통과 조건 |
| --- | --- |
| TOWN-01 | 선택→난이도→마을 도착을 긍정 화면 증거로 확인하고 선택 난이도를 유지한다. |
| TOWN-02 | Act 2 방문→Act 1 복귀→종료→다음 방 생성이 순서대로 확인된다. 클릭 자체는 성공 증거가 아니다. |
| TOWN-03 | 모든 반복은 횟수·시간 상한이 있고, F10/중단/수동 개입·불명확 상태·창 변화에서 추가 입력을 억제한다. |
| TOWN-04 | 개발 세션 동안 상태 전환·실패·전후 프레임을 검토할 수 있어야 하며 실패를 성공 표본에서 숨기지 않는다. 세션 종료 시 결과 요약·필요 인식 자산을 남기고 불필요한 원시 자료를 정리한다. |
| TOWN-05 | Act 1 세 배치에서 이동 중 목표를 재인식하고 홀드를 해제한다. NPC 대화 개입을 제한 복구하며 실제 검증 배치를 결과에서 구분한다. 캡처·인식·입력 간격을 측정한다. |

## Checks And Regression

- 현재 화면에서 1회 진행 후 제한 반복. 전투 지역 이동은 별도 결과로 기록한다.
- 인식에서 템플릿보다 작은 프레임, 크기 변화, 입력 중단, 빠른 로딩의 취급을 검증한다.
- 기존 room-only 생성/종료 및 Summoner 호출의 기본 동작·난이도 전달을 보존한다.
- 평가: Functional(필수), Contract(좌표/중단/증거), UX(상태·실패 설명). GUI 레이아웃을 변경하면 직접 화면 확인을 추가한다.

## Harness Trace

- Spec: [SPEC-0001](../spec/spec-0001-supervised-town-room-loop.md)
- Run: [RUN-20261004-02](../run/run-20261004-02-supervised-town-room-loop.md)
- 구현 결과와 사람의 최종 수용은 run에서 구분한다.
