# FEAT-0004: 직업별 시전 규칙과 개인 FCR 조회

## Metadata

- ID: `feat-0004`
- Status: `passed` — 입력 없는 reference 조회·회귀 검증. 사람 수용과 후속 실게임 승인은 run에서 구분한다.
- Type: `foundation`
- Surface / Execution Profile: Python 공통 설정·조회 / `foundation-contract`
- Required Evaluators: Contract, Functional
- Parent PRD: [PRD-0005](../prd/prd-0005-arcane-navigation-and-recovery.md)
- Created / Updated: `2026-10-05`
- Approval: 사용자가 Flash의 현재 FCR 105를 확인하고 진행 가능한 다음 PRD 개발을 요청했다. primary는 미확정 실게임 조건 없이 검증 가능한 첫 시전 조회 계약을 선택했다. 다른 이동/입력 기능까지 승인된 것으로 취급하지 않는다.

## Goal And Scope

직업·스킬 공통 표와 개인 캐릭터 FCR의 연결을 실제 코드로 고정한다. 같은 직업 프로필은 한 표를 참조하며 이름이나 키로 직업/FCR을 추측하지 않는다.

포함: 출처/revision을 가진 불변 공통 규칙 카탈로그, 선택적 캐릭터 직업/시전 설정, 현재 FCR 이하 최대 임계값 조회, Flash 소서리스/일반 텔레포트/105, 미설정·미지원의 입력 없는 결과. 초기 배포 자료는 기본 Resurrection의 소서리스·팔라딘 일반 텔레포트 **reference 표** 두 개다.

제외: 관측 지연 적응·타이머/초 단위 이동 간격·캡처/HUD/도착 인식·게임 입력·장비 자동 인식/전환·GUI 설정 편집·다른 직업/형상/스킬/ROTW의 실행 지원·자산 정리. 공개 표는 실제 게임 시전 성공이나 현재 클라이언트 패치의 검증 표가 아니다.

## Acceptance Contract

| ID | 결과 |
| --- | --- |
| CAST-01 | 공통 표가 소서리스/팔라딘별 FCR 임계값·프레임·출처·revision을 소유하고 개인 설정에 표를 중복하지 않음 |
| CAST-02 | Flash의 직업/FCR 105를 로드하고 8프레임 reference 예상; 같은 105인 팔라딘은 10프레임, 선형 보간 없음 |
| CAST-03 | 임계값 직전/정확값/직후와 최상위 구간을 정확히 조회하고 잘못된 숫자·표/중복 context를 거부 |
| CAST-04 | 누락 직업/FCR/설정·미지원 표/게임 계열/스킬/형상은 `hold`; 다른 캐릭터/직업 fallback 없음 |
| CAST-05 | 결과에 선택 context·FCR·임계값·프레임·rule set/revision/출처·reference 상태를 유지; 조회에 I/O/입력/상태 누적 없음 |
| CAST-06 | 기존 캐릭터·선택·생존/마을 회귀 유지; 규칙/FCR 변경 후 새 조회는 새 값, 공개 표를 관측값으로 수정하지 않음 |

## Contracts And Dependencies

`config/game-rules/casting.json`은 공통 표, `config/characters/characters.json`은 개인 직업/현재 FCR/표 참조를 소유한다. 표는 일반 시전 애니메이션의 전체 프레임이며 action frame이나 도착 확인 완료가 아니다. 코드에는 Flash 이름이나 직업별 계산 분기를 두지 않는다.

기존 JSON 로더와 [조사 자료](../../research/class-cast-rate-reference.md)만 필요하다. FEAT-0003 생존 상태와 PRD-0008 runtime은 후속 실제 이동의 조건이며 이 입력 없는 조회의 blocker가 아니다. 현재 FCR은 사용자 보고 설정값이다. 실시간 활성 장비/시전 상태와 같다는 증거는 미래 입력 소비자가 확인한다.

## Checks And Trace

Contract/Functional: 실제 config 로드/선택, 두 직업의 전체 임계값 경계, 이름이 다른 같은 직업, 표/개인 설정의 오류·누락·미지원, 결과 provenance와 불변성, 기존 전체 회귀. Python compile/Black·JSON/문서 연결도 확인한다.

- Spec: [SPEC-0003](../spec/spec-0003-class-casting-rules.md)
- Run: [RUN-20261005-02](../run/run-20261005-02-class-casting-rules.md)
- Evaluations: [Contract](../evaluation/eval-0007-contract-class-casting-rules.md), [Functional](../evaluation/eval-0008-functional-class-casting-rules.md)

## Continuity Notes

- `2026-10-05`: 진행 가능한 다음 개발 요청에 따라 첫 입력 없는 조회 경계를 고정했다. 후속 적응/실제 이동은 별도 경계다.
- `2026-10-05`: 구현과 47개 전체 테스트·변경 Python compile/Black를 통과했다. 현재 게임 시전·패치 지원 검증으로 확대하지 않는다.
