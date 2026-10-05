# EVAL-0014: 실제 북쪽 조우와 재생 관찰

## Metadata

- ID: `eval-0014`; Status: `complete`; Evaluator Type: `functional`; Result: `FAIL`
- Run ID: `run-20261005-05`; Attempt: `1`
- Feature: [FEAT-0007](../feature/feat-0007-arcane-north-encounters.md)
- Spec: [SPEC-0006](../spec/spec-0006-arcane-north-encounters.md)
- Execution Profile: `backend-product`
- Evidence Coverage: `partial` — 실제 조우/공통 재생·합성은 있음; 감독 갱신 조건 실패
- Created: `2026-10-05`

## Checks And Evidence

[Run](../run/run-20261005-05-arcane-north-encounters.md)이 세 시도의 입력·저장 범위·실패·귀환/잔량을 소유한다. 실제 중앙의 정확한 w/a/a/s/F1/d/w 뒤 북쪽 회색 통로·계단/평면 배치·구울 군주/지옥혈족원 alive→교전 후 사체/드롭을 관찰했다. 신단 오인은 이름 대조로 바로잡아 landmark 음성 자료로 남겼다. 용병 개입/겹침으로 개체별 사망 연결/순수 노바 처치/정확한 수는 미확인이다.

마나45/1543 화면에 감독자가 포션1을 사용하고1543/1543 회복을 확인했다. town expanded 벨트3/4/4개와 TP4개를 확인하여11개 포션을 남긴 뒤 닫았다. 의도적 저체력/고갈·스크롤 사용·포션 획득/보충은 실행하지 않았다. 이 결과는 자동 포션 모듈/전체 생존 완료가 아니다.

17개 새 검증은 실제 종류/위치/점수/신단·사체/지형과 합성 다중 후보·반경/맥락/연결·가림을 구별한다. 0.5초 후 alive 자세의 미검출은 unknown이며 사망으로 오기록하지 않는다. 실제11개 belt crop+이전 town HUD의 합성 재생도 통과했다. 원본 scene와 같은 템플릿의 높은 점수는 독립 방/자세 정확도가 아니다.

최종 named verifier는 전체89/89, Python3개 compile, Black3개, PNG16개·문서7개 integrity를 종료0으로 확인했다. 자동 검증 통과가 아래 실제 갱신 실패를 해소하지 않는다. 필요한 공통 자료만 남기고 세 캡처 writer 종료와 task 소유 임시 원본/진단 정리를 확인했다.

## Findings And Acceptance Impact

- **High / spec gap / blocking to FEAT-0007 supervised safety completion**: 화면 왕복으로140초 갱신 경계를 놓쳤고 버프 최대값 감소를 확인했다. 마지막 시도는70초 이후 새 탐색을 제한했지만 귀환/방어/관찰 지연을 통제하지 못했다. SPEC의 버프 갱신 필요 선점과 사용자 최단 버프140초 정책을 실제로 완료한 근거가 없다. 동일 방식의 필드 재시도로 해결하지 않고 독립 실시간 감시와 실행기 선점을 구체화한다.
- **Recorded implementation limits / blocking before autonomous combat**: 세 번째 HUD 양쪽 숫자 동시 판독은232/357. 단일 자세 템플릿은 바로 다음 alive 자세를 놓친다. 미확인 화면으로 행동을 계속하는 자동 경계는 구현하지 않았으며 현재 모듈을 기존 북쪽 실행기에 연결해 안전하다고 판단할 수 없다.
- **Evidence gap / non-blocking to input-free EN-01–EN-04**: 전체 레이아웃 분류·true walkability·landing 성공·자동 instance/life 생산자·운영 반경·파밍·서모너 처치/키 회수는 범위 밖이다. 단순 corpse group와 template candidate는 개체 사망 증거로 쓰지 않아 계약 검증은 통과한다.

첫 시도200MiB 원본 상한 도달은 조우 저장 실패로 기록했고 시작시점/압축을 수정해 이후 실제 alive/corpse 자료를 확보했다. 최종 귀환 F2는 recorder 종료 뒤이며 Sky 직접 관찰 근거와 분리했다. 자료 누락을 저장된 재생 성공으로 바꾸지 않는다.

## Route

`spec-review`. 코드/17개 새 검증은 통과하나 실제 감독 버프 조건은 실패다. 새로운 live 확대 전 실시간 생존·버프 감시/포션 보충·닫힘과 입력 실행기를 연결하는 다음 경계를 명시한다. feature/run acceptance는 보류하며 전체 PRD 완료로 보고하지 않는다.
