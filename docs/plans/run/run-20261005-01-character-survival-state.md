# RUN-20261005-01: 캐릭터별 생존·버프 상태

## Metadata

- ID: `run-20261005-01`
- Status: `passed` — 입력 없는 정책/상태/판단과 회귀 검증.
- Feature: [FEAT-0003](../feature/feat-0003-character-survival-state.md)
- Parent PRD: [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Active Spec: [SPEC-0002](../spec/spec-0002-character-survival-state.md)
- Surface / Profile: Python 공통 설정·상태 / `foundation-contract`
- Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`

## Approval And Loop

수정 PRD와 입력 없는 첫 계약 기능에 대한 사용자 승인으로 시작했다. 마을 밖 필드에서만 버프하며 Flash의 최근 순서를 설정에 적용한다. primary가 정책·상태 계약·구현·최종 의미 검토를 소유한다. 읽기 전용 evidence scout는 기존 설정 소비자 호환성을 확인했다. 실제 게임 입력/캡처/전투·자산 변경은 제외한다.

## Attempts

- Attempt 1: 공통 `survival.py`, 선택적 `CharacterProfile.survival`, Flash 확인값/최근 입력 순서, 입력 없는 검증을 SPEC-0002 범위로 추가했다. 상태는 캐릭터/방에 귀속하고 잘못된 종류·계수·순서·시각·context는 진행 근거가 되지 않는다.
- 프로젝트 Python 3.7.9 venv는 샌드박스에서 base Python 실행을 만들지 못했다. 승인된 로컬 검증으로 테스트/formatter/compiler가 정상 실행됐다. 설치·게임 입력·네트워크 요청을 하지 않았다.

## Evaluation Coverage

- [Contract 평가](../evaluation/eval-0005-contract-character-survival-state.md): PASS, 이번 계약의 증거 complete.
- [Functional 평가](../evaluation/eval-0006-functional-character-survival-state.md): PASS, 입력 없는 기능/회귀 증거 complete.
- 새 테스트 18개 + 기존 마을 16개 = **34/34**. 포맷 뒤 재검증도 통과했다. 변경 Python 3개 `py_compile` 및 Black check 통과. 관찰 시간 상한은 합성 테스트의 입력 값이며 실게임 안전 수치로 승인된 것이 아니다.
- 최종 문서 15개의 strict UTF-8·로컬 링크 150개·섹션 앵커 13개와 `git diff --check`가 통과했다. 변경 Python/문서 18개의 숫자 Unicode 범위 검사에서도 의도하지 않은 Han/Kana 문자가 없었다.
- 실제 HUD/벨트/버프 인식·회복/종료 입력·보충 사냥·획득·무사망/장시간 안정성은 검증하지 않았다. 승인 기능에 비차단이며 후속 기능에서는 자료·관찰/입력 경계를 해소해야 한다.

## Post-Contract Regression Check

설정 생산자·캐릭터 선택·버프 순서 소비자·주변 문서·기존 마을 회귀를 확인했다. 기존 두 캐릭터의 정책은 미설정, 입력 순서는 유지한다. 기존 run-level `life`는 호환 필드이며 새 계약의 fallback이 아니다. 현재 Arcane 토큰 실행은 새 관찰/판단기에 연결하지 않았다. 활성 문서의 과거 Flash 순서 차이 표현을 정리하고 이전 마을 기록에 후속 정합의 위치를 연결했다.

## Human Review Outcome

승인 범위 FEAT-0003의 구현/입력 없는 검증을 완료했다. 사람의 최종 결과 수용은 아직 기록하지 않는다. 다음 경계는 실시간/기록 기반의 필요한 조건과 실제 HUD·벨트·버프/안전 필드 관찰이며, 자동 포션·종료·전투로 이번 승인을 확대하지 않는다.

## Session Artifacts

검증 캐시/compiled diagnostics는 한 task 소유 임시 폴더에만 저장했다. 모든 검증 프로세스 종료 뒤 제거했다. 런/평가나 실행 파일은 이 폴더를 입력으로 읽지 않는다.
