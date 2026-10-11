# RUN-20261009-04: 승인한 자원 종류 정책

- ID: `run-20261009-04`; Status: `passed`; Attempt: `1`
- Feature: [FEAT-0011](../feature/feat-0011-potion-pickup-policy.md); Spec: [SPEC-0010](../spec/spec-0010-potion-pickup-policy.md)
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Created / Updated: `2026-10-09`

사용자가 확정한 포션 대체 조건과 TP 4장 목표를 입력 없는 설정/공통 자원 상태에 연결한다. 정비 절차·다른 보급 수치는 보류한다. 별도 실제 버프 재확인에서 첫 효과는 확인했고, 140초 갱신은 stale permit으로 입력 전 거부됐다. 게임은 안전 종료 후 선택 화면이며 재시험은 사용자 확인 대기다. 이 run은 게임 입력이나 실제 회수를 하지 않는다.

primary가 schema·판단과 의미 수용을 검토했다. 선택적 `PotionPickupPolicy`와 현재 잔량/최신 벨트 기반 종류 반환을 구현했고, Flash의 TP `target_count`를 4로 설정했다. 다른 null 목표의 첫 관찰 baseline은 유지한다. 보충 모드가 12개까지 이어져도 현재 7개부터 작은 포션은 반환하지 않는다. 종류 반환은 안전/위치/획득 확인이나 입력 허가가 아니다.

전체 160개 unittest, 변경 Python 2개 파일의 Black/compile, diff check가 통과했다. 신규 5개 테스트는 TP 고정 목표, 6↔7 변화·0/12개, 미확인/stale/future, 개인 정책 분리와 잘못된 schema를 검사한다. 기존 null baseline 테스트는 명시적 null 프로필로 계약을 유지했다. sandbox Path.resolve WinError5는 동일 명령의 일반 환경 재실행과 구분했다. verifier의 상태 전후는 같고 compile 임시 파일은 자동 정리됐다. [Contract](../evaluation/eval-0021-contract-potion-pickup-policy.md), [Functional](../evaluation/eval-0022-functional-potion-pickup-policy.md)은 입력 없는 경계의 PASS다.

실제 지상 포션/TP 식별·작은 포션 벨트 외형·접근·획득과 잔량 증가·정비는 미완료다. 나머지 보급 목표는 정하지 않았고 자동 폐기/구매를 실행하지 않았다.
