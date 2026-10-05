# EVAL-0008: 직업별 시전 조회 동작

## Metadata

- ID: `eval-0008`
- Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Run ID: `run-20261005-02`; Attempt: `1`
- Feature: [FEAT-0004](../feature/feat-0004-class-casting-rules.md)
- Spec: [SPEC-0003](../spec/spec-0003-class-casting-rules.md)
- Execution Profile: `foundation-contract`
- Evidence Coverage: `complete` — 입력 없는 설정/조회 및 기존 회귀
- Created: `2026-10-05`

## Checks And Evidence

프로젝트 venv의 `python -B -m unittest discover -s tests -v`: **47/47** 통과. 시전 조회 13개·생존 18개·기존 마을 16개이며 포맷 뒤 재검증도 통과했다. 변경 Python 3개 `py_compile` 및 Black check도 통과했다. 최초 sandbox venv launcher는 base Python을 시작하지 못했으며 승인된 로컬 검증에서 동일 명령이 통과했다. 설치·게임 입력은 실행하지 않았다.

시전 검증은 실제 config 로드/Flash 선택, 두 표의 임계값 직전/정확값/직후·상한, 같은 FCR의 직업 차이·동일 직업의 다른 이름, 누락/미지원 보류, invalid selection, 표 없는 단독 캐릭터 파일, 잘못된 FCR/필드/표·중복 context, 결과 provenance/불변성·새 규칙/FCR 조회를 다룬다. 추가 합성 직업은 표로 계산이 확장되는지 확인하는 테스트이며 새 실게임 직업 지원의 증거가 아니다.

생존/마을 회귀는 이전 계약의 판단과 상태 전환을 유지한다. 이번 입력 없는 성공을 실제 포션·버프·텔레포트·방 왕복의 신규 성공으로 계산하지 않는다.

## Evidence Gaps And Route

실제 클라이언트 프레임·활성 장비 FCR·이동 완료/다음 입력 가능·안전 선점·시전 속도/캡처 지연 적응은 미검증이다. frame-to-seconds와 실제 이동 간격도 구현하지 않았다. 이번 FEAT-0004에는 비차단이며 별도 관찰/제어 계약이 필요하다.

차단 finding 없음. Route: `pass`; 후속 실행 범위를 승인하지 않는다.
