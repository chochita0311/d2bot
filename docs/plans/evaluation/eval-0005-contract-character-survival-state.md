# EVAL-0005: 캐릭터 생존·버프 계약

## Metadata

- ID: `eval-0005`
- Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Run ID: `run-20261005-01`; Attempt: `1`
- Feature: [FEAT-0003](../feature/feat-0003-character-survival-state.md)
- Spec: [SPEC-0002](../spec/spec-0002-character-survival-state.md)
- Execution Profile: `foundation-contract`
- Evidence Coverage: `complete` — 승인한 입력 없는 계약에 한정
- Created: `2026-10-05`

## Evidence And Checks

primary가 설정/관찰/판단의 의미와 소비자 경계를 검토했다. 읽기 전용 scout는 설정 생성/소비자 호환성을, bounded verifier는 정해진 최종 테스트·compile·Black 명령의 출력만 확인했다. worker 결과를 의미적 승인으로 취급하지 않았다.

- STATE-01/06: 실제 JSON 로드·최근 Flash 순서·다른 두 프로필 유지/미설정·캐릭터 선택·잘못된 비율/시간/계수/중복/미지원 필드 거부를 확인했다.
- STATE-02/05: 실제 시전 증거를 받는 필드/주변 안전 gate, 버프별 monotonic 시각·가장 먼저 도래하는 갱신, 요청과 결과 분리를 확인했다.
- STATE-03/04: 벨트 완전 관찰·종류/용량·확인된 delta·재관찰·epoch/sequence/시각·미관찰을 확인했다. 잔량/타이머 데이터는 열/설정 버프 개수 안에서 유지하며 무제한 이벤트 이력을 보유하지 않는다.
- 새 공통 모듈은 캡처·GUI·입력 의존성이 없다. config는 선택적 정책을 로드하며 run-level 생존 수치나 Flash 이름 fallback을 사용하지 않는다.
- 총 34개 단위/회귀 테스트, Python 3개 py_compile와 Black check가 통과했다. 문서/JSON 소비자와 활성 순서 설명을 정합시켰다.

## Evidence Gaps And Route

실제 인식·입력·획득·무사망은 이번 계약의 통과 주장이 아니다. 이 gap은 FEAT-0003에 비차단이며 후속 실게임 기능에 필요한 증거다. 미확정 TP 목표는 최초 실제 관찰량으로 대치 기준을 잡고, 보충 필요 반환값이 전투 모드 해제를 정의하지 않는다는 한계를 명시했다.

차단 finding 없음. Route: `pass`; 결과 수용은 사람 검토로 반환한다.
