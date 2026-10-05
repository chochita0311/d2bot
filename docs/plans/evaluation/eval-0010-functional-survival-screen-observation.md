# EVAL-0010: 생존 화면 판독 동작

## Metadata

- ID: `eval-0010`; Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Run ID: `run-20261005-03`; Attempt: `1`
- Feature: [FEAT-0005](../feature/feat-0005-survival-screen-observation.md)
- Spec: [SPEC-0004](../spec/spec-0004-survival-screen-observation.md)
- Execution Profile: `backend-product`
- Evidence Coverage: `complete` — 제한한 화면/진단·합성 경계·기존 회귀
- Created: `2026-10-05`

## Checks And Evidence

primary가 실제 마을 화면을 육안으로 대조했다. 생명력 **985/985**, 마나 **1075/1075**이며 펼친 벨트는 포션 **12**/TP **4**였다. 실제 포션 하나를 인벤토리로 옮긴 상태는 1열 3개·2/3열 4개·TP 4개로 읽었고, 닫힌 벨트는 잔량 미확인으로 남겼다. 포션을 다시 벨트에 복구한 독립 후속 캡처도 일치했다. 네 자료는 이름 없는 HUD crop로 보존하며 같은 마을 표본이다.

최종 입력 없는 `--live --frames 3` 진단은 세 프레임 모두 native 1922×1140, 985/985·1075/1075, 포션 12·TP 4를 반환했다. explicit Flash/room 정책은 모두 `hold / buff_requires_field`, 전송 입력은 0이었다. 고해상도 호출 시각 기준 캡처는 **36.96–46.84ms**, 판독은 **3.15–3.64ms**였으며 같은 장면의 짧은 관찰값이다. 분위수·장시간 성능·비상 입력 지연 상한으로 사용하지 않는다. 버프를 마을에서 시전하지 않았다.

[unittest](../../../tests/test_survival_vision.py) 포함 전체 **60/60** 통과: 화면 관찰 13개, 기존 시전 13개·생존 18개·마을 16개. 지정 새 Python 4개 compile, 코드/테스트 3개 Black check도 통과했다. sandbox의 venv launcher 제한을 승인된 로컬 검증으로 해결했고 환경을 변경하지 않았다.

합성 검증은 label/숫자/슬롯 가림·잘못된 크기·잘못된 숫자 범위·모호함·설정 오류, 새 최대값 및 <50%/<10% 경계, 6/0개 정책 요청, 잘못된 종류/capacity/중복 위치, CLI 상호 배타/프레임 상한/명시 설정과 최소화 창의 close를 확인했다. 6/0개는 슬롯/정책 자료의 합성이며 실제 벨트 고갈이나 게임 종료 테스트가 아니다. 입력과 무관한 판독 결과를 실제 회복·탈출 성공으로 세지 않는다.

## Evidence Gaps And Route

실제 저체력·버프 전후 최대치·독/격전/가림·다른 방/배치/언어, 일반 활력 포션·모든 빈 슬롯 위치와 실게임 오류율은 미검증이다. CLI 결과를 기존 실행기에 연결하지 않았다. 이 기능에는 비차단이며 자동 제어/필드 확대에는 필요한 후속 증거다.

차단 finding 없음. Route: `pass`; 후속 기능 실행과 사람 수용은 별도다.
