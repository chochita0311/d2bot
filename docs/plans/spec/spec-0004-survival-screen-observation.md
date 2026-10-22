# SPEC-0004: 입력 없는 생존 화면 판독

## Metadata

- ID: `spec-0004`; Status: `implemented`; Run ID: `run-20261005-03`; Attempt: `1`
- Feature: [FEAT-0005](../feature/feat-0005-survival-screen-observation.md)
- Parent PRD: [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Surface / Profile: Python CV·관찰 CLI / `backend-product`
- Required Evaluators: Contract, Functional; Created: `2026-10-05`; Updated: `2026-10-22`

## Sources And Boundary

현재 safe Act 1 town 캡처·펼친 벨트, 기존 HUD 참고 `assets/character/play/status.png`, 기존 마을 reference 화면을 직접 대조한다. shared 자료는 이름 없는 HUD label/digit/item crop와 필요한 replay HUD만 새 owner에 둔다. 전체 새 원본과 diagnostics는 task 세션에만 두고 종료 시 정리한다. 이전 자산은 이동/삭제하지 않는다.

## Implementation Contract

v17의 `5-portal-outline`은 현재 포탈 빛이 인접한 숫자5의 검토된 외형이다. 기존 외곽선 분리·32×40 binary atlas·glyph .72/차이.08·같은 화면 current/max·범위 거부를 유지한다. 저장 숫자쌍928/1543 재생과 숫자 제거 음성은 실제 다른 방 정확도/획득/마감이 아니다. 현재 최대값을 이전 화면에서 채우지 않는다.

현재 v21의 용병 보완은 opt-in인 기존54×54 native 얼굴 영역의 전체 일치 .92를 우선한다. 전체 미일치 때만 현재 라벨 mask에 가린 픽셀을 제외하고 최소50%의 같은 얼굴 텍스처·동일 .92 상관을 요구한다.61번 얼굴 .923433과 검토 양성6개/음성12개에 근거한 보정이며 별도 runtime 외형은 추가하지 않는다. 이전 v19에서는 가림 없는 .934801 얼굴을 놓친 .95/.94와 음성 최고 .634700의 제한 비교로 당시 .93을 선택했다. 부족/불일치/부재는 `unverified`이며 과거 얼굴이나 체력바/이름으로 대신하지 않는다. UI-only 원본/해시·수치와 v16–21 이력은 survival manifest가 소유하고 실제 실행 결과는 통합 run 개발 재개에서 구분한다. ALT 복원·중단/포커스·자원 조건은 우회하지 않는다.

`diablo2/common/survival_vision.py`는 공통 layout/label/glyph/slot 자료를 로드하고 BGR 프레임에서 관찰을 반환한다. 현 reference 캔버스 1922×1140만 명시적으로 지원하며 크기/두 label의 위치·일치 확인이 없으면 보류한다. 숫자는 current/max와 slash를 모두 읽고 0≤current≤max, max>0를 확인한다. 버프/장비로 최대값이 바뀌어도 같은 프레임에서 읽은 current/max를 사용하며 이전 최대값/비율을 보완값으로 쓰지 않는다. 인식 불명확/모호한 glyph·가림은 null; 색이 없다는 이유로 생명력 0이나 포션 0을 만들지 않는다.

벨트 상단 frame을 확인하고 4×4 슬롯을 판독한다. purple potion/TP scroll/empty는 실제 crop로 구분하며 어떤 슬롯이 미확인이면 전체 잔량 확정을 보류한다. 닫힌 벨트에서 보이는 한 줄로 총 잔량을 추정하지 않는다. 정책 연결은 열별 종류·capacity를 대조하고 완전한 관찰만 `observe_belt`에 전달한다. 미확인 resource 값은 그대로 `SafetyObservation`에 전달한다. `location=unknown`, `monsters_clear=None`, 버프 확인 없음으로 안전 필드나 출발을 허가하지 않는다.

`python -m diablo2.tools.survival_observe`는 mutually exclusive live/image 입력, 1–5프레임 상한, 선택적 explicit character/room과 JSON 결과를 제공한다. 정책 진단에는 양의 유한 `--age-limit`도 명시하며 검증된 생존 기본값으로 취급하지 않는다. live는 exact 지정 게임 창/strict window backend이며 최소화/숨김/누락 창을 거부하고 finally에서 capture를 닫는다. focus·입력·이동을 하지 않는다. 시각은 capture 전후 `perf_counter` 경계와 처리 종료를 분리해 기록하며 렌더러 시각을 증명하지 않는다. 각 프레임의 정책 판단은 독립적인 관찰 전용 요청이며 actor가 없다. image/replay와 실제 live는 provenance로 구분한다.

## Validation

실제 마을의 원 숫자/종류·잔량을 primary 육안 대조 후 offline/live 결과와 비교한다. 준비용 원본과 후속 프레임을 구분한다. 빈 슬롯/벨트 가림·닫힘/숫자 오류·레이아웃 불일치, 50%/10%·6/0개 요청은 실제/합성을 구분해서 확인한다. 기본 glyph에 없거나 미지원 icon은 거부한다. 저체력/필드 위험을 일부러 만들지 않는다. 전체 회귀·compile/Black·shared manifest 소비/문서 연결을 검증한다.

## Open Limits

실게임 저체력·버프 전후 최대값·독·격전·가림·판독 오류율·긴급 입력 지연과 다른 UI 배치는 미검증이다. 자료는 한 마을의 HUD crop와 후속 프레임이며 독립 방으로 세지 않는다. 2/6 glyph는 실제 캐릭터 패널에서 가져온 후보이며 다른 HUD 값에서의 검증은 남아 있다. 포션은 큰 완전 활력 외형, empty는 한 upper slot만 실제 확인했다. Windows OCR의 현 Korean engine은 숫자에 문자를 섞고 slash를 잘못 읽어 사용하지 않는다. 현재 검증은 관찰 전용이며 자동 생존 제어 readiness가 아니다.
