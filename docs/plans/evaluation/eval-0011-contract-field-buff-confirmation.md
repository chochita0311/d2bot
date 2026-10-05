# EVAL-0011: 필드 버프 확인 계약

## Metadata

- ID: `eval-0011`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Run ID: `run-20261005-04`; Attempt: `1`
- Feature: [FEAT-0006](../feature/feat-0006-field-buff-confirmation.md)
- Spec: [SPEC-0005](../spec/spec-0005-field-buff-confirmation.md)
- Execution Profile: `backend-product`
- Evidence Coverage: `complete` — 입력 없는 계약·감독 증거 소비 경계
- Created: `2026-10-05`

## Contract Evidence

primary가 [BuffSequence/BeltInspection](../../../diablo2/common/buffing.py), [시각 보조](../../../diablo2/common/buffing_vision.py), [HUD 생산자](../../../diablo2/common/survival_vision.py)를 검토했다. 캐릭터/방별 상태와 최신 순번/단조 시각, 개인 순서/지속시간/벨트 설정을 소비한다. 키 전송 결과와 효과 증거를 분리하며, 최대값 증가나 I/II 탭만으로 전체 버프·개인 장비 복귀를 선언하지 않는다. 실제 필드/주변 적 없음과 전체 효과 생산자는 감독자이고 자동 판독기가 아니다.

BF-01/02: 중복 a와 단계가 유지되며 town·안전 불명확·적 있음·오래된/다른 방 관찰은 요청을 차단한다. 시작 후 차단/입력 실패는 취소하며 회복/고갈 종료가 선점한다. 미확인 전송을 재요청하지 않는다.

BF-03/04: 효과·전체 순서·전투 장비 복귀를 명시한 동일 최신 관찰이 있어야 확인한다. 타이머는 해당 단계 요청 직전 시각을 사용해 검토 지연으로 연장되지 않는다. 늦은 확인/중복 확인과 갱신이 필요한 완료 상태는 진행을 허가하지 않는다. 별도 Python 프로세스의 단조 시각을 합치지 않는 소비자 계약을 기록했다.

BF-05: expanded/closed/unknown을 구분하고 닫힘 전용 증거를 요구한다. 모호한 펼침은 잔량을 무효화하고 숨은 행을 다시 세지 않는다. 토글 중·열림·가림·오래된 닫힘은 이동 조건을 통과하지 못한다. 경로/전투 안전은 별도 증거다.

BF-06: 기존 토큰 실행기와 새 증거 계약은 아직 연결되지 않았다. 공유 자산에는 HUD/일반 탭만 있고 이름·장비 원본은 없다. 기존 자산 이동/이름 변경 없이 관련 config/기능 안내/아키텍처/PRD의 생산자·소비자 가정을 갱신했다. [테스트](../../../tests/test_buffing.py)와 전체 72개 회귀가 통과했다.

## Evidence Gaps And Route

자동 주변 안전·전체 버프 효과·기존 입력 실행기 통합, 다른 개인 전투 세트와 독립 방/언어/배치 검증은 없다. FEAT-0006의 감독 증거 소비/입력 없는 계약에는 비차단이며 자동 필드 진행 수용에는 필요한 후속 근거다. Flash 150초는 사용자 확인 정책이며 이번 실행에서 실측하지 않았다.

차단 finding 없음. Route: `pass`. 전체 생존/전투 완성과 사람의 후속 필드 확대 수용으로 해석하지 않는다.
