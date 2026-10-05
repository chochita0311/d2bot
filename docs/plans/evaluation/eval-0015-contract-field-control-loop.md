# EVAL-0015: 공통 필드 제어 계약

## Metadata

- ID: `eval-0015`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Run ID: `run-20261005-06`; Attempt: `1`
- Feature: [FEAT-0008](../feature/feat-0008-field-control-loop.md)
- Spec: [SPEC-0007](../spec/spec-0007-field-control-loop.md)
- Execution Profile: `backend-product`
- Evidence Coverage: `complete` — 승인한 공통 계약; 자동 의미 생산자/완전 런 제외
- Created: `2026-10-05`

## Producer And Consumer Review

primary가 [제어 계약](../../../diablo2/common/field_control.py), [runtime/HUD bridge](../../../diablo2/common/field_runtime.py), [입력 backend](../../../diablo2/common/controller.py), [연속 버프 실행](../../../diablo2/common/buffing.py), 기존 북쪽 경로 수정과 관련 검증을 검토했다. 캐릭터/방/순번/단조 시계/현재 프레임 맥락을 보존한다. field/clear/적 근접/도달 가능/사망·전리품은 외부 생산자의 명시적 근거이며 템플릿 후보나 미검출을 긍정값으로 자동 승격하지 않는다.

단일 유지 키 소유자는 F2를 해제한 뒤 F4를 누르며 같은 hold에 keyDown을 반복하지 않는다. 화면/버프/전투/도착 시한과 pause/stop/포커스/예외에서 해제한다. 부분 keyDown 실패와 keyUp 실패에도 소유 상태를 버리지 않고 해제를 재시도한다. 최신 mailbox/독립 tick은 지연된 비전·저장 소비와 입력 시한을 분리한다. OS/프로세스 정지 복구 보증은 범위 밖이다.

확인된0개는 오래된 화면에서도 종료 요청을 우선한다. 포션은1회 전송 후 확인을 기다린다. 확인된 소비 delta와 새로운 펼친 벨트 절대 잔량을 구분해 이중 차감하지 않는다. 가려진 슬롯은 잔량을 미확인으로 두며 소비 시점에 오래된 큐 자료는 덮어쓰지 않는다. 같은 프레임 자원/최대값과 긍정 벨트 표시만 HUD bridge가 생산한다. source의 field/clear/도착 의미를 HUD가 생성하지 않는다.

직업/FCR 카탈로그가 시전 참고값을 소유하고 개인 프로필은 class/FCR/키/생존 정책을 소유한다. 확인된 도착 지연만 조준 간격에 반영하며 미확인 목표 재전송으로 상한을 늘리지 않는다. Flash <=6개 보충 모드는12개까지 유지하고 획득 확인 없이 잔량을 올리지 않는다.

공통 버프 실행기는 프로필의 반복 키/순서/각 간격을 연속 소비한다. 자동 관찰·중단 가능한 대기·명시적 timeout을 주입하며 적/저자원/벨트/중단/시한은 남은 키를 취소한다. 전송 완료는 `await_effect`이고 효과/전체 순서/장비 복귀의 별도 긍정 확인 전에 타이머가 생기지 않는다. 표준 press의 반환값 검사와 finally 해제는 가짜 backend와 최신 실제 probe에서 확인했다.

## Verification And Assets

named verifier의 최종 고정 packet: 전체129/129, 변경 Python13개 Black check/compile 모두 종료0. compile/cache 출력은 task scratch에만 만들었다. 필드29개·probe6개·버프15개와 생존/HUD/시전/조우/마을 회귀가 포함된다. 최종 의미 수용은 primary가 실제 결과와 별도로 검토했다.

named scout는 [HUD manifest v4](../../../assets/ui/survival/manifest.json)의 reference13개 경로/geometry와 선언된 SHA2563개 일치를 확인했다. 신규2장에 캐릭터/방 이름이 없음을 시각 확인했다. 공통 숫자/표시 자산과 개인 정책을 분리하고 이전 private 자산의 일괄 이동/이름 변경은 수행하지 않았다. 자료와 테스트는 임시 원본을 참조하지 않는다.

문서18개의 UTF-8/replacement 문자 없음과 상대 링크259개 존재를 최종 read-only packet으로 확인했다. 최신 backend 재시험의 완료와 남은 전체 운영/140초 실제 갱신 범위를 구분하며 현재 데스크톱 차단이라는 오래된 주장을 남기지 않았다.

## Gaps And Route

자동 clear/개체 사망/도착/아이템 획득 생산자, 버프 효과 자동 판독, 실제 고갈 종료 소비자, GUI 통합은 이 공통 계약에 비차단이며 자율 운영 확대 전에 필수다. 공통 executor의 실제 scene 생산자 조합은 live probe와 구분한다. [Functional](eval-0016-functional-field-control-loop.md)이 실제 입력과 해당 한계를 소유한다. 차단 계약 finding 없음. Route: `pass`.
