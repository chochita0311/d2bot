# SPEC-0003: 직업별 시전 규칙 조회

## Metadata

- ID: `spec-0003`; Status: `implemented` — 입력 없는 reference 계약, 검증 결과는 run/evaluation 소유.
- Run ID: `run-20261005-02`; Attempt: `1`
- Parent Feature: [FEAT-0004](../feature/feat-0004-class-casting-rules.md)
- Parent PRD: [PRD-0005](../prd/prd-0005-arcane-navigation-and-recovery.md)
- Surface / Profile: Python 공통 설정·입력 없는 조회 / `foundation-contract`
- Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`

## Source And Boundary

FEAT-0004의 승인 경계를 구현한다. 사용자 확인은 Flash 소서리스/현재 FCR 105다. [원 연구 조사](../../research/class-cast-rate-reference.md)의 두 일반 시전 표를 명시적 reference 자료로만 사용하며 실제 게임 검증을 주장하지 않는다. 새 게임 입력/관측/적응·기존 이동 제어 변경은 제외한다.

## Configuration And Ownership

- `casting_rule_sets`: 공통 JSON 최상위 목록. 각 항목은 `rule_set_id`, `ruleset_family`, `revision`, `source_url`, `evidence_status`(이번에는 `reference`만), `rules`를 가진다.
- rule은 `character_class`, `skill_id`, `form`, `breakpoints` 목록을 가진다. breakpoint는 `minimum_fcr`와 `frames`다. 0에서 시작하며 임계값은 엄격히 증가, 전체 프레임은 엄격히 감소한다. class/skill/form context와 rule-set ID는 중복 금지다.
- `CharacterProfile.character_class`는 선택적 canonical 직업 ID, `casting`은 선택적 `rule_set_id`, `skill_id`, `form`, `fcr`(정수 ≥0 또는 null)의 불변 개인 설정이다. 누락은 미설정이다. bool/실수/음수·빈 문자열·미지원 하위 필드는 오류다.
- Flash만 `sorceress`, 표 `d2r-normal-teleport-reference-v1`, `teleport`, `normal`, FCR `105`를 설정한다. 나머지 프로필 직업/FCR은 추측하지 않는다. 공통 표는 `resurrection` 두 직업만 갖고 ROTW나 특수 시전/형상으로 fallback하지 않는다.

## Query Contract

`diablo2/common/casting.py`는 불변 정책/카탈로그와 입력 없는 조회를 소유한다. `BotConfig.casting_rules`는 카탈로그이며 기존 loader가 로드한다. `get_active_casting_estimate`는 기존 선택된 프로필의 계열·직업·개인 시전 설정을 전달하는 얇은 adapter다. 선택된 캐릭터가 없거나 잘못된 ID이면 `character_missing`으로 보류하며 로더의 다른 캐릭터 fallback을 사용하지 않는다.

조회 결과는 `estimate` 또는 `hold`와 이유, 조회 context/FCR, 선택 임계값·전체 프레임, rule set/revision/출처/reference 상태를 가진다. 현재 FCR 이하의 최대 임계값을 선택한다. 개인 설정/직업/FCR 미확정과 표·계열·context 미지원은 각각 구분하며 다른 기본값을 대신 가져오지 않는다. 빈 카탈로그도 미지원으로 결정한다.

조회는 현재 전달된 불변 설정을 사용하고 결과를 누적/학습하지 않는다. 새 FCR/규칙에는 새 조회를 호출한다. 결과는 설정 기반 예상이며 실제 스킬 사용 가능·활성 장비·도착·다음 입력 가능/시전 종료의 증거가 아니다. frame→초 변환과 대기 상한도 이번 계약에 없다.

## Validation And Regression

CAST-01/05: source/revision, table ownership·context/result provenance·불변성·I/O 없음. CAST-02/03: 실제 Flash 로드/105→8·팔라딘105→10, 모든 임계값 전후/최대 FCR 및 오류. CAST-04/06: 누락/미지원/규칙 변경·이름이 다른 동일 직업·기존 설정/생존/마을 회귀. 변경 Python compile/Black, JSON·문서 UTF-8/링크를 확인한다.

## Open Blockers

이번 reference 조회의 blocker는 없다. 현재 클라이언트 패치 검증·다른 직업/형상·활성 장비/FCR 관찰·시전 종료/도착·시간 상한·적응 및 입력은 후속 계약에 남긴다.
