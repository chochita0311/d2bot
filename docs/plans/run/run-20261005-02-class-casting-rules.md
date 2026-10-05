# RUN-20261005-02: 직업별 시전 규칙 조회

## Metadata

- ID: `run-20261005-02`; Status: `passed` — 입력 없는 reference 조회 및 회귀 검증.
- Feature: [FEAT-0004](../feature/feat-0004-class-casting-rules.md)
- Parent PRD: [PRD-0005](../prd/prd-0005-arcane-navigation-and-recovery.md)
- Active Spec: [SPEC-0003](../spec/spec-0003-class-casting-rules.md)
- Surface / Profile: Python 설정·조회 / `foundation-contract`
- Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`

## Approval And Readiness

사용자가 Flash의 FCR 105를 확인하고 진행 가능한 다음 PRD 개발을 요청했다. primary가 입력 없이 검증 가능한 직업 공통 시전 조회 경계를 선택·고정하고 spec을 준비했다. 원 자료의 reference 조회는 readiness를 충족하며 미확정 실제 이동/안전/패치 검증으로 실행을 확대하지 않는다.

primary가 요구·설계·구현·최종 의미 검토를 소유한다. evidence scout는 설정 소비자 호환성, bounded verifier는 고정한 검증 명령/표·문서의 출력만 확인한다. 게임 입력/GUI/자료 캡처·자산 변경·외부 게시를 하지 않는다.

## Attempts

- Attempt 1: 직업별 공통 reference 표, 개인 직업/FCR, 입력 없는 조회와 검증을 구현했다. Flash 105를 소서리스 표에 연결하고 미설정·미지원은 보류한다. 다른 개인 프로필을 추정하거나 현재 이동 executor를 변경하지 않았다.
- venv launcher가 sandbox에서 base Python을 실행하지 못해 승인된 로컬 검증으로 동일 테스트/formatter/compiler를 실행했다. 환경 설치·게임 조작은 수행하지 않았다.

## Evaluation Coverage

- [Contract 평가](../evaluation/eval-0007-contract-class-casting-rules.md): PASS, 승인 reference 조회 계약의 증거 complete.
- [Functional 평가](../evaluation/eval-0008-functional-class-casting-rules.md): PASS, 입력 없는 조회/회귀 증거 complete.
- 테스트 **47/47**: 새 시전 13개·생존 18개·기존 마을 16개. 포맷 후 재검증 통과. 변경 Python 3개 compile/Black check 통과.
- 관련 문서 15개의 strict UTF-8·로컬 링크 152개·앵커 15개, 문서/Python 18개의 숫자 Unicode 범위 검사와 `git diff --check`가 통과했다. 첫 문서 검증은 존재하는 디렉터리 링크를 파일로 한정해서 거부했으며, 패킷의 존재 검사 조건을 바로잡은 뒤 통과했다. 제품 파일 수정으로 회피하지 않았다.
- 실제 게임 시전·적응 이동·활성 장비/FCR·패치 지원은 이번 범위의 통과 주장이 아니다.

## Post-Contract Regression Check

선택적 필드를 기존 config loader에 연결하고 선택된 캐릭터의 직접 조회만 제공했다. 누락/잘못된 선택에 기존 fallback을 가져오지 않는다. 기존 소비자는 새 필드를 요구하지 않으며 GUI 선택·생존·마을 동작을 유지한다. 설정 안내/아키텍처/조사·서모너 문서의 과거 미구현 설명을 현재 조회와 후속 적응 제어로 구분했다.

## Human Review Outcome

승인한 첫 입력 없는 시전 조회 구현/검증을 완료했다. 사람의 결과 수용은 아직 기록하지 않는다. 다음 경계는 기존 북쪽 자료를 우선 검토한 HUD·벨트·안전 필드/버프와 텔레포트 도착·다음 입력 가능 관찰이다. 실제 이동/안전 입력은 필요한 관찰과 runtime 조건을 확인한 별도 경계이며 이번 통과로 확대하지 않는다.

## Session Artifacts

검증 cache/compiled diagnostics를 한 task 소유 임시 폴더에만 저장했다. 관련 검증 프로세스 종료 뒤 workspace 내부의 정확한 경로를 확인하여 제거했다. 런/평가·실행 파일에 임시 폴더 의존은 없다.
