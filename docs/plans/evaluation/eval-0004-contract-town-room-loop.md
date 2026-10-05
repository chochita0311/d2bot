# EVAL-0004: 화면·입력·중단 계약

- Status: `complete`
- Evaluator Type / Result: `contract` / `PASS`
- Evidence Coverage: `partial`
- Run / Attempt: [RUN-20261004-02](../run/run-20261004-02-supervised-town-room-loop.md), 최종 `192534`
- Feature / Spec: [FEAT-0002](../feature/feat-0002-supervised-town-room-loop.md), [SPEC-0001](../spec/spec-0001-supervised-town-room-loop.md)
- Profile: `backend-product`
- Created: `2026-10-04`

## Contract Evidence

생산자 `ScreenCapture`→정규화/인식→`TownLoopPlanner`→소비자 `RunLifecycleSession`→GUI/CLI를 소스에서 검토했다. JSON/private 템플릿, 명시적 반복/난이도, kind별 입력, 이벤트/프레임/result/timings가 계약 표면이다. 입력 전 창/포커스 guard와 완료 뒤 상태 확인을 사용한다. WP 커서 이동 허용은 예상 변위와 메뉴/도착 증거에 한정된다.

`tests/test_town_loop.py`의 16개 테스트는 잘못된 초기 화면/캐릭터/모달, 빠른 로딩, 잘못된 도착, 추가 방 방지, 1–3 상한, 이동·복귀 접근 상한, 홀드 해제와 NPC Esc, 캡처 모양/템플릿 크기, 중단 후 클릭 억제, WP 커서 이동 제한, 정리 오류 뒤 실행 상태 해제, 캡처/녹화 자원 소유권을 검증했다. 변경 여섯 Python 파일의 py_compile와 Black 검사도 수행했다.

기존 room-only는 `town_loop=False`가 기본이고 Summoner의 create/exit 호출 계약은 유지된다. GUI의 기존 사용자 수정은 보존했다. `SessionRecorder.close`가 영상만 해제하고 새 `ScreenCapture.close`가 MSS를 소유하도록 구분했다. 기존 room-only/전투의 실제 반복은 이번 회귀 실행에 포함하지 않았다.

## Evidence Gaps And Route

SceneVision과 입력 mock은 합성 계약 검증이며 실제 OS 이벤트를 대신하지 않는다. 실제 F10·포커스 상실·리사이즈의 모든 순간과 다른 DPI/화면 크기는 미검증(TOWN-03). 현재 창의 유한 실행에는 non-blocking이며 지원 환경/무감독 확대 전에 직접 검증해야 한다. `pass`: 관측 범위와 한계를 함께 보고한다.
