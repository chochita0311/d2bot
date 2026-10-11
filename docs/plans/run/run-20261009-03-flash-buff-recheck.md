# RUN-20261009-03: Flash 변경 버프 순서 감독 재확인

- ID: `run-20261009-03`; Status: `in-loop`; Attempt: `1`
- Feature: [FEAT-0006](../feature/feat-0006-field-buff-confirmation.md), [FEAT-0008](../feature/feat-0008-field-control-loop.md)
- Spec: [SPEC-0005](../spec/spec-0005-field-buff-confirmation.md), [SPEC-0007](../spec/spec-0007-field-control-loop.md)의 기존 짧은 감독 probe 경계
- Profile: `backend-product`; Evaluators: Contract, Functional
- Created / Updated: `2026-10-09`

## Approved Direction And Budget

사용자가 Flash의 d→F1 변경과 전체 구현·감독 검증을 승인했다. 이전 당시 F1→d의 실제 효과를 새 순서의 증거로 사용하지 않는다. 최신 화면에서 Flash/난이도/Act 1을 확인하고 한 방의 Arcane 중앙에 진입하여 새 7키 순서와 실제 최대값·장비 복귀를 대조한다. 사망/전투 실험이나 모든 날개 이동은 이 재확인의 범위가 아니다.

기존 승인한 probe 조건을 유지한다: fresh 감독 permit ≤15초, 자동 HUD/포커스/벨트/저자원 검사, stop/pause hotkey, 버프 묶음 ≤8초, finally 입력 해제. 모호한 화면·적·위험이면 추가 입력을 멈추고 안전 종료/사람 검토 정책을 따른다. 버프 1회부터 확인하며 필요하면 새 화면과 타이머로 갱신 묶음 1회만 추가한다. 자동 clear/effect 생산자 성공으로 기록하지 않는다.

자료는 workspace 안 task 소유 bounded scratch에만 기록하고 필요한 재사용 crop만 assets로 선별한 뒤 원본/로그를 세션 종료에 정리한다. 상품 아이템 이동·판매·폐기, 구매/사망 후 재정비는 수행하지 않는다. 사람과 정하지 않은 Act 1 정비를 추측하지 않는다.

## Results And Remaining

Sky로 선택/지옥 난이도/로딩/Act 1 도착을 확인했다. 마을 횃불 부근에서 웨이포인트 접근이 막혀 두 번 클릭 뒤 같은 위치임을 확인했고, 바닥으로 우회한 뒤 실제 웨이포인트 목록을 열었다. Act 2 탭/비전의 성역을 각각 새 화면으로 확인하고 중앙에 도착했다. 현재 지역은 화면상 공포의 영역 93이다. 사용자는 이 조건도 이후 검증에 포함하도록 확인했다. 전투는 수행하지 않았다.

벨트를 펼쳐 보라색 12개와 TP 4장, 다시 닫힌 화면을 확인했다. 실제 Python HUD는 1922×1140에서 생명력985/985·마나1075/1075·belt closed로 읽었다. 새 감독 permit으로 기존 backend의 첫 버프 probe를 수행했다. 요청은 w/a/a/s/d/F1/w 순서, 전체 약4.12초, 오류0·held_after=[]·equipment_dispatch_balanced=true였다. 보고는 effect_verified=false로 남겨 입력과 효과를 분리했다. 이후 독립 화면/HUD에서1580/1580·1543/1543, 보호 효과와 이전 전투 스킬 표시를 확인했다. 이는 새 순서의 한 감독 효과 증거이며 자동 장비/효과 판독 완료가 아니다.

두 번째 갱신 시험은 감독 준비가 늦었다. 최초 s 요청 후 약146초에 화면을 재확인했고 그 뒤 stdin 허가 전송이15초 유효 시간을 초과하여 validate_permit가 `supervised permit is stale`로 입력 전 거부했다. 갱신 명령은0개이며 실제140초 갱신은 통과하지 않았다. 이후 화면은985/985·1075/1075로 돌아왔다. 이를 정상 갱신으로 기록하지 않는다. Esc→저장 및 종료→Flash 선택 화면 복귀를 확인했다. 마지막 지역이Act 2인 비상 종료이므로 정상 Act 1 마감으로 세지 않으며 다음 방의 출발 Act를 다시 관찰해야 한다.

다음 시험의 검토 가능한 수정 경계: backend/capture를 미리 준비하고 감독이 갱신 예산 안에 새 화면과 즉시 허가를 제공한다. stale 허가를 허용하거나 과거 clear를 재사용하지 않는다. 실제 제어 통합은 monotonic 갱신 시한·최신 scene 생산자·키 선점 연결이 필요하며 이 감독 probe로 자동140초 갱신을 주장하지 않는다. 사용자의 실패 후 안전 종료/사람 확인 규칙에 따라 재시험 전 검토를 요청한다. 입력 없는 후속 구현은 계속한다.

이후 사용자는 매번 도구로 타이밍을 맞추는 절차 대신 검증된 동작을 프로그램에 연결하고, 자동 타이머와 당시 주변 안전 조건으로 갱신하도록 지시했다. [RUN-20261009-05](run-20261009-05-automatic-buff-renewal.md)가 이 후속 구현을 소유한다. 위 수동 준비 제안은 현재 실행 방향을 대신하지 않으며, 이 run의 실제 140초 갱신 미통과 결과는 유지한다.

raw before/after와 report는 task 소유 scratch의3파일·총6,626,639byte로 한정했다. 재사용 HUD 자산이 이미 같은 최대값을 지원하므로 새 원본을 runtime 자산으로 중복 등록하지 않았다. 보고서 대조와 두 probe 종료 뒤 exact workspace 경로·3파일·reparse point 없음 확인 후 제거했고 부재를 확인했다. durable 문서나 프로그램은 이 임시 경로에 의존하지 않는다.
