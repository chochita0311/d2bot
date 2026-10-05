# RUN-20261005-05: 아케인 북쪽 감독 조우

## Metadata

- ID: `run-20261005-05`; Status: `returned-to-spec`; Attempt: `1`
- Feature: [FEAT-0007](../feature/feat-0007-arcane-north-encounters.md)
- Parent PRD: [PRD-0006](../prd/prd-0006-summoner-combat-and-loot.md)
- Active Spec: [SPEC-0006](../spec/spec-0006-arcane-north-encounters.md)
- Profile: `backend-product`; Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`

## Readiness And Approval

사용자의 실제 북쪽 이동/조우와 자산 수집 지시로 한 감독 구간을 승인했다. primary가 요구/출처/자산 소유·설계·게임 입력·최종 검토를 담당하고 named scout는 기존 지형/템플릿의 고정 범위를 읽기만 한다. 실제 입력은 Computer-use Sky로 수행한다. 기존 전체 북쪽 실행기는 호출하지 않는다.

시작 화면은 Act 1 마을 웨이포인트, HP 985/985·마나 1075/1075, 벨트/인벤토리 닫힘이다. 필요한 준비/감독 자료와 실패/귀환 결과는 관찰 후 이 문서에 기록한다.

## Session Artifacts

입력 없는 캡처 프로세스와 Computer-use 감독 입력을 분리했다. 공통 자료에는 이름/방 식별/개인 설정을 넣지 않았다. 필요한 [지역 자산](../../../assets/regions/arcane-sanctuary/remastered-ko-1922x1140/README.md)과 [포션 소모 후 벨트](../../../assets/ui/survival/README.md)를 선별했다. 원본/진단은 task 소유 bounded scratch이며 최종 검증 뒤 writer 종료·정리를 확인했다. 남긴 자산/consumer는 임시 경로에 의존하지 않는다.

## Supervised Attempts And Findings

| 시도 | 실제 화면/입력 | 저장 및 한계 |
| --- | --- | --- |
| 1 | Act 1에서 12개 포션/4개 스크롤 확인 후 벨트 닫힘. 아케인 중앙의 w→a→a→s→F1→d→w, 북쪽 여러 층/발판에서 붉은 근접 적·F4 전후 사체 관찰, 중앙→Act 1 복귀 | 3fps/JPEG96 원본이 85.6초·약 200MiB 상한에 먼저 도달했다. 252프레임은 마을/버프 준비만 포함하며 **첫 조우 원본은 남지 않았다**. 종류/처치를 자산/자동 정확도 근거로 계수하지 않는다. 감독 중 버프 만료도 관찰 |
| 2 | 새 지옥 방, 중앙 동일 버프, 북쪽 계단 배치. 탑 모양 대상에 F4 뒤 이름을 확인하니 **화염 저항의 신단**. 추가 수집 중지·중앙→Act 1 복귀 | 버프 완료 뒤 시작한 2fps/JPEG88, 357프레임·180.2초·약125MiB. 신단을 몬스터로 오인한 장면은 음성 자료. 골격 잔해만으로 종류/개체 사망을 확정하지 않는다. 140초 갱신 경계를 놓치고 최대값 감소를 관찰 |
| 3 | 새 지옥 방, 동일 7단계 버프를 확인. 북쪽 평면 통로에서 이름표가 보인 구울 군주·지옥혈족원과 조우. F4 뒤 다수 사체/드롭, 마나 **45/1543** 화면에 1번 포션 사용 후 **1543/1543** 회복. 약72초부터 추가 탐색 중단, 중앙→Act 1 복귀 | 버프 시작 전 기록, 357프레임·180.2초·약132MiB, 간격0.500–0.517초. alive/corpse/독립 사체 후속 자료 확보. 버프 귀환 중 만료; 마지막 귀환 F2는 기록 종료 약6초 뒤여서 해당 입력은 Sky 직접 화면 근거만 있음 |

세 시도 모두 감독 입력이며 자율 전투/반경 제어가 아니다. 용병이 함께 공격했으므로 모든 처치를 노바 한 번의 결과로 귀속하지 않는다. 각 방의 정확한 개체 수/각 대상의 alive→corpse 연결은 확인하지 않았다. 입력 전 조준을 위한 짧은 왼쪽 클릭 이동이 있어 전 구간 순수 텔레포트 성공으로 계수하지 않는다. 우주로의 실패 입력을 일부러 실험하지 않았다.

세 번째 입력 타임라인(KST): w14:51:09.686, a16.298/22.443, s28.346, F1 36.076, d44.084, w53.450; 북쪽 F2 14:52:10.986/29.987, F4 39.877, 포션1 49.226. 시전 시작 전 시각을 따로 기록했고 새 탐색 F2/조준을70초 이후 거부했으나 귀환/방어·도구 왕복 지연까지는 제한하지 못했다. 이 값은 모델을 거치는 감독의 지연 근거이며 FCR105/8프레임 게임 입력 성능 측정이 아니다.

named scout가 고정 JSONL/입력 기록을 확인했다. 세 번째 생명력 판독247/357(최저 유효비율94.3%), 마나305/357(최저3.69%), 둘 다 판독232/357이다. 생명력 미확인110·마나 미확인52는 glyph ambiguity/HUD unverified/unreadable이며 미확인을 정상/안전으로 바꾸지 않는다. 벨트는 닫힌342·미확인15, 숨은 잔량 미확인이다. 감독자가 마을에서 펼쳐 **3/4/4개의 포션과4개 스크롤**, 합계11개 포션을 확인하고 다시 닫았다. 소지품에 여분 포션은 보이지 않았다. 다음 필드 진행 전1개 보충이 필요하다.

## Implementation And Reuse

[encounters.py](../../../diablo2/common/encounters.py)는 native context·이동 세대·지상점/개인 반경·출처/개체 연결이 있는 생존 상태 계약이다. [encounter_vision.py](../../../diablo2/common/encounter_vision.py)는 소유한 PNG manifest에서 위치/점수/외형 후보와 지형 patch 근거를 반환한다. 게임 입력·개체 연관 생산자·살아 있는 적 없음 확인·실시간 감시를 만들지 않았다. 기존 런 입력 실행기를 import/수정하지 않는다.

기존 공통 팔레트/거리 함수는 재사용했다. 바닥과 겹치는 회색 void 샘플은 신규 지역 manifest에서 제외했다. 기존 WEBP 위장 PNG 3개를 일괄 변환하거나 불확실한 레거시 파일을 삭제하지 않았다. 신규 자산은 지역/화면 variant 아래 짧은 kind-state 이름으로, 개인 설정과 분리했다.

실제 재생에서 alive 원본의 구울/혈족 후보와 좌표를 찾고 독립 사체 후속의 혈족 무더기를 약.985 점수로 찾았다. 반면 바로 다음 alive 자세는 **후보0**이다. 이 누락은 죽음이 아니라 미확인이다. 신단 장면은 landmark 한 후보와 몬스터 후보0이다. 실제 patch는 회색 바닥 ratio.842, 우주 ratio.997, 캐릭터 가림 reject를 보였으며 진짜 접근 가능/전체 map 분류 근거는 아니다.

## Evaluation And Route

named verifier의 고정 packet은 전체 **89/89** 테스트(기존72+신규17), Python3개 compile, Black3개, 공통 PNG16개 geometry/출처, 문서7개의 링크/Unicode 검증을 통과했다. 종료 코드 모두0이다. [Contract report](../evaluation/eval-0013-contract-encounter-observation.md)와 [Functional report](../evaluation/eval-0014-functional-encounter-observation.md)는 공통 코드와 실제 감독 조건의 결과를 구분한다.

실제 **140초 재버프 경계 준수는 실패**했다. 사람이 모델의 화면 왕복을 기다리는 수집 방식은 독립 안전/버프 감시를 대신할 수 없다. 같은 실패를 다시 필드 시도로 반복하지 않는다. 다음 live 확대 전 실시간 생존 감시·미확인 HUD 선점·최단 버프 갱신/안전 재버프·포션 보충·닫힘 확인을 입력 실행기와 연결하는 경계를 구체화해야 한다. 그때 다중 자세/개체 연결과 landing 확인도 필요하다.

공통 관찰 코드/자료 수집은 구현했으나 감독 안전 조건 실패로 이번 feature를 전부 통과/완료로 처리하지 않는다. 전체 PRD-0004/0005/0006와 네 방향 자동 런도 미완료다. 다음 역할: primary spec review, 필요한 실행기/실시간 감시 경계 결정. 인용한 사용자 승인은 전체 PRD 승인으로 확대하지 않는다.

Post-contract regression: 기존 생존18·시전13·HUD13·버프12·마을16개가 모두 통과했다. 기존 실행기/개인 정책 값/레거시 자산 경로는 변경하지 않았다. 필드 타이머 합성 통과와 실제 감독 갱신 실패를 별개로 기록한다. Human acceptance: 보류, returned layer: spec.
