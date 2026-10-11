# EVAL-0025: 상황 인지 탐색 연결 계약

- ID: `eval-0025`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Feature: [FEAT-0013](../feature/feat-0013-arcane-perceptual-navigation.md); Spec: [SPEC-0012](../spec/spec-0012-arcane-perceptual-navigation.md)
- Run: [RUN-20261009-06](../run/run-20261009-06-arcane-perceptual-navigation.md); Attempt: `1`
- Profile: `backend-product`; Evidence Coverage: `partial`; Created: `2026-10-09`

primary가 native 좌표/viewport·단조 시각/세대·현재 창·공통 입력 소유·관찰 의미와 제품 증거 경계를 검토했다. 입력 없는198개 회귀와 Black/compile8개, diff check가 통과했다. 발밑 가림의 초기 간격은 명시적 seed 반경 이내이며 이후 관찰된 발판을 사용하고 최대 step에 seed까지 거리를 포함한다. 후보 팔레트는 명시적 지역/보정 소유로 분리했다.

요청/입력 응답/실제 도착은 별도다. 한 번 cast 후 새 이동 세대·배경 대응·참고 하한 전에는 다음 cast를 만들지 않는다. 미검출은 정지·도착이 아니며 이전 커서를 무효화한다. 귀환은 현재 화면에 투영한 상대 흔적과 별도 중앙 확인을 요구한다. 빈 끝/보스/중앙/clear의 의미는 검증할 생산자 소유이며 템플릿 없음으로 승격하지 않는다. NPC 호버는 마을 전용이다.

공통 runtime의 최신 판단·actor lock·watchdog·창/나이/포커스/중단 재검사를 경유하는 단일 press adapter가 있다. 생존·버프·전투·전리품 선점은 외부 경로를 허용하지 않는다. 기본 dry 관찰 검증을 실제 GUI/네 방향 완료로 확대하지 않는다.

코드 연결 계약에는 차단 finding이 없다. 실제 발 위치/계단/끝·중앙·안전/효과 생산자와 GUI/live 검증 부재는 PN-01/03/06의 제품 수용에 **blocking**이다. Contract PASS는 제품 기능 PASS가 아니다. Route: `spec-review` — [Functional finding](eval-0026-functional-arcane-navigation.md) 보완.
