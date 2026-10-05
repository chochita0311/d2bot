# EVAL-0007: 직업별 시전 조회 계약

## Metadata

- ID: `eval-0007`
- Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Run ID: `run-20261005-02`; Attempt: `1`
- Feature: [FEAT-0004](../feature/feat-0004-class-casting-rules.md)
- Spec: [SPEC-0003](../spec/spec-0003-class-casting-rules.md)
- Execution Profile: `foundation-contract`
- Evidence Coverage: `complete` — 승인한 reference 조회 계약
- Created: `2026-10-05`

## Checks And Evidence

primary가 요구·표의 적용 한계·공통/개인 소유권과 구현 의미를 검토했다. evidence scout는 캐릭터 선택·설정 소비자/작성 경로의 호환성을 조사했고, bounded verifier는 고정된 테스트·compile·Black 결과를 확인했다. worker 결과를 설계나 사람 수용 승인으로 사용하지 않았다.

- CAST-01/05: [공통 카탈로그](../../../config/game-rules/casting.json)와 [불변 조회 모델](../../../diablo2/common/casting.py)이 두 직업의 명시적 class/skill/form, 출처·revision·reference 상태를 보존한다. 개인 설정에는 표를 중복하지 않는다. 조회에는 I/O·타이머·입력·학습이 없다.
- CAST-02/03: [검증](../../../tests/test_casting_rules.py)이 Flash 105→8, 팔라딘 105→10, 두 표의 모든 경계와 최상위 구간, 숫자/중복/순서/빈 자료의 거부를 확인했다.
- CAST-04/06: 누락/미지원과 잘못된 활성 캐릭터가 다른 표/프로필로 대체되지 않는다. 선택적 새 필드를 로드하며 기존 생존·캐릭터 선택·마을 테스트를 유지한다. FCR/규칙을 바꾼 새 조회는 새 결과를 반환하고 이전 결과/공통 표는 바뀌지 않는다.

## Evidence Gaps And Route

자료 revision은 참조 자료를 편입한 날짜이며 현재 D2R 패치 검증이 아니다. 현재 장비/스킬 사용 가능 여부·시전 종료·도착·관측 지연·모든 직업/형상은 미검증이다. 이번 입력 없는 reference 계약에는 비차단이며 후속 실제 제어에서 해결한다.

차단 finding 없음. Route: `pass`; 실제 게임 실행·후속 기능 승인이나 사람 수용을 기록하지 않는다.
