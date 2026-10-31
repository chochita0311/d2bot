# SPEC-0016: Rust 설정 문서와 선택 계약

## Metadata
- ID: `spec-0016`; Status: `approved`; Run ID: `run-20261022-02`; Attempt: `1`.
- Parent Feature: [FEAT-0017](../feature/feat-0017-rust-config-contract.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface: `backend`; Execution Profile: `foundation-contract`; Required Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Source Set
[config.py](https://github.com/chochita0311/d2bot/blob/cde68bc2eb1285eddcd8a261cdfe411ed4924f76/diablo2/common/config.py), casting/survival의 `from_dict`와 정책 validators, config guide, existing casting/survival/GUI 선택 테스트, REFACTOR-0002 invariants. 기존 Python load/asdict를 실행 가능한 비교 oracle로 사용한다.

## Implementation Goal
Rust에서 같은 JSON을 로드하고 기존 기본값/정책 검증과 캐릭터/farm 선택을 보존한다. legacy dataclass가 검사하지 않는 필드는 lossless JSON 문서로 유지하고, 실행기용 typed view는 소비자 feature에서 검증한다. 여기서 무조건 엄격한 deserialize를 적용해 기존 입력 허용/실패 시점을 바꾸지 않는다.

## In-Scope Behavior
- JSON 단일 파일/디렉터리, UTF8 BOM, Windows 경로 component의 소문자 비교에 따른 정렬, 재귀 객체 병합과 scalar/list 교체, 삽입 순서 보존.
- capture/record/gui/hotkey constructor의 기본 필드와 unknown keyword 거부, domain builder의 기본값·필드 선택/list 변환·pause key 소문자/float 변환.
- character class, casting profile/catalog, survival/belt/buff/pickup의 현재 필수/허용 필드·타입·범위·중복·종속 합계 검사. 큰 정수는 JSON 문서에서 손실 없이 유지한다.
- 첫 run/character, legacy farm의 Python truthiness 우선, 유효한 선호 run 적용. 잘못된 character 선택 시 fallback active만 바꾸고 farm을 바꾸지 않는다.
- 선택된 등록 farm은 key로 원본 run profile을 참조하며 불필요한 복제 상태를 만들지 않는다. 등록 profile 수정이 farm 조회에 보이는 계약을 테스트한다. action 중 config 변경 금지는 후속 GUI/session 계약이다.
- normalized snapshot은 Python BotConfig의 dataclasses.asdict와 같은 필드/값을 제공한다. 실제 파일은 수정하지 않는다.
- 승인 delta: 선택적 `buff_action_pause_seconds` 값이 float 변환 불가/null/다른 컨테이너이거나 NaN/±Infinity/overflow/음수이면 해당 lowercase override를 제거한다. consumer의 기존 missing-key 기본값을 유지한다(Summoner 0.4초, 공용 executor는 caller의 명시 default). load는 계속하고 JSON pointer·원본 값·조치를 `ConfigWarning`으로 반환해 후속 CLI/GUI가 표시한다. 유효한 0/양수·기존 bool 변환은 유지한다. 잘못된 mapping 구조 자체나 필수 survival/casting 정책은 이 보정 범위가 아니다.

## Out-Of-Scope Behavior
casting/survival 상태 실행, GUI/게임 입력, 설정 쓰기·키/기본값 정리, JSON 표준 밖 NaN/Infinity literal 호환, 임의 Python 객체/임의 프로필 삭제·동적 타입 API 재현. 이 제외는 해당 입력에서 동등성을 주장할 근거가 아님을 기록한다.

## Affected Surfaces
src/common.rs, src/common/config.rs와 private schema module, serde_json의 preserve_order/arbitrary_precision features, Rust config integration tests와 작은 Python oracle test helper. 기존 Python/설정/자산은 수정하지 않는다.

## State And Interaction Contract
전체 문서는 로드 성공 후에만 반환한다. 파일/JSON 오류와 기존 schema의 KeyError/TypeError/ValueError/AttributeError 범주를 구분하며 대표 외부 메시지도 비교한다. 실패한 로드가 기존 config를 부분 갱신하지 않는다. 숫자·null·bool을 임의 보정하지 않는다.

## Data And Contract Assumptions
- 같은 Cargo package 안의 공용 config가 소유한다. raw/defaulted 값은 JSON 문서이고 실제로 검증하는 policy 영역만 schema 검증을 적용한다.
- serializer 자체의 object key 출력 순서는 identity가 아니지만 파일 병합/캐릭터/run 선택 순서는 계약이다. snapshot에 모든 설정 필드를 보존한다.
- 테스트 oracle의 Python은 개발 비교 전용이며 release 앱의 의존성이 아니다. 기존 개인 설정을 새 fixture로 복제/출판하지 않는다.

## Contract Surfaces
Producer: recursive loader + schema builder. Consumer: 후속 typed runtime adapters/GUI/CLI. Source owner: 현재 Python/config docs, 이전 결과는 Rust module. Derived output: 테스트 중 메모리의 snapshot만. tempfile은 Cargo test tmp 아래 unique task 소유 dir를 닫은 뒤 제거한다.

## Required Evaluators
Contract: 모든 model 기본 필드·선택/alias·schema 분기·공유 경로. Functional: 기존 committed config/단일 character config와 synthetic order/BOM/merge/default/null/타입/unknown/schema/selection fixture의 Python↔Rust 비교, 승인된 잘못된 pause의 기본값 복귀·경고·정상 항목 보존 비교, fmt/check/clippy/test/release.

## Acceptance Mapping
FEAT-0017의 로드/선택 보존→normalized snapshot·key 순서·공유 farm 테스트. 오류 보존→대표 잘못된 JSON/schema의 예외 범주·메시지 비교. 기존 불변→Python/config/assets diff와 실제 fixture read-only.

## Evaluation Focus
raw 문서 보존을 실제 실행 타입 검증 완료로 표현하지 않는다. 모든 임의 malformed Python 값의 동등성을 주장하지 않고 감사한 사례·공개 API 범위를 기록한다. JSON 문법 밖 literal은 명시된 제외다.

## Open Blockers
현재 feature 경계의 blocker 없음. typed runtime adapter와 전체 port parity는 후속이며 codec/GUI는 이 feature에 영향 없음.

## Continuity Notes
- `2026-10-22`: 사용자의 전체 실행 위임 아래 FEAT-0016 통과 후 이 입력 없는 계약을 고정했다. 기존의 약한 legacy 값 검증을 임의 강화하지 않는 문서 계층을 선택했다.
- `2026-10-22`: 사용자 답변으로 잘못된 선택적 버프 대기시간의 보정을 승인했다. 해당 입력만 intentional delta이며 정상 계약·다른 오류 계약과 별도로 검사한다. 런타임 대기 변환은 후속 소비자에서도 checked conversion으로 방어한다.
