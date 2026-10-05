# EVAL-0016: 필드 유지 입력·연속 버프 검증

## Metadata

- ID: `eval-0016`; Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Run ID: `run-20261005-06`; Attempt: `1`
- Feature: [FEAT-0008](../feature/feat-0008-field-control-loop.md)
- Spec: [SPEC-0007](../spec/spec-0007-field-control-loop.md)
- Execution Profile: `backend-product`
- Evidence Coverage: `partial` — 주입형 전체 계약과 실제 짧은 중앙 입력; 자동 운영 제외
- Created: `2026-10-05`

## Acceptance And Evidence

| Feature contract | 확인한 결과 |
| --- | --- |
| FC-01 | 주입 backend에서 유지/해제/전환/중단/포커스·입력 오류/시한 검증. 실제 F2 약0.28초 이동 후 모호한 마나에 해제, F4 한 번 keyDown 후 정상 keyUp |
| FC-02 | 합성 정리→전리품 확인→이동, 불확실 사망/미검출/시간 상한에서 재개 차단. 실제 적 처치나 전리품 획득 시험 아님 |
| FC-03 | 주입 검증에서 저자원/0개/140초 갱신이 이동·공격 선점; 새 화면이 없는 tick에서도 해제. actual probe는 확인된 버프 갱신 시각을 상한으로 소비 |
| FC-04 | 단일 포션 전송/확인 대기/새 펼친 벨트 절대 잔량 재확인/중복 차감 금지/오래된 큐 거부. 최종 actual town 포션11개/TP4와 닫힘 확인 |
| FC-05 | 직업/FCR 참고값·확인된 도착 지연/정체 상한의 주입 검증. 현재 장비와 느린 장비의 실제 속도 비교는 미검증 |
| FC-06 | 실제 캡처/감독 입력·합성 의미 관찰·공통 executor 검증을 분리. 입력이나 corpse 후보만으로 완주/자동 처치/획득을 선언하지 않음 |

[Run](../run/run-20261005-06-field-control-loop.md)이 실제 과정/실패/복구/귀환을 소유한다. 사용자 승인한 Python backend로 짧은 시험만 실행했다. 이전 backend의 F4 약0.95초 한 번 hold에서 마나1543→1500→1457과 반복 노바를 관찰했다. 최신 SendInput 검사/20ms press 수정 뒤7키 연속 버프와0.5초 F4 재시험은 정상 전송/해제·오류0이었다. 버프 순서는 w/a/a/s/F1/d/w이며0.4초 프로필 대기에 관찰 처리/잠깐의 HUD 가림 대기가 더해졌다. actual 요청 간격은 약0.48–0.90초다.

버프 after 원본은 생명력1490/1580·마나1538/1543, 후속 화면은1580/1580·1543/1543. 감독 효과/활성 전투 세트 확인은 전송 보고의 `effect_verified=false`와 별도다. 공통 executor의 프로필 간격/적 접근/중단/timeout 검증은 주입형이고 최신 probe의 감독 전송 루프를 공통 executor의 자동 live 연결로 부르지 않는다.

Sky 활성화 오류 뒤 Python 캡처는 정상·foreground는 false였다. 기존 focus 함수로 foreground true와 최신 화면을 확인하고 시험을 재개했다. Windows 잠금 또는 Python 실패로 확정하지 않는다. 초기 EOF/오래된 확인은 입력 전 거부, 첫 buff cleanup 오류는 실패로 기록하고 수정했다. 실패한 프로세스를 성공 probe 수로 합치지 않는다.

최종129개 회귀와13개 Black/compile 종료0. 실제 노바 후 마나1457/1543 crop은 긍정 읽기, 실제 이동 후 사람은1530/1543로 읽으나 CV가 거부한 crop은 음성 회귀로 보존했다. 임계값 완화 없이 모호함이 release로 이어진다. 최종 Act1 town의 자원985/985·1075/1075, 용병 생존·포션3/4/4·TP4와 벨트/인벤토리 닫힘을 확인했다.

프로그램 writer 종료와 원본 요약/공유 자산 소비자 확인 뒤 task 진단·raw 캡처·compile/cache를 제거했다. exact workspace 소유 경로/reparse point 없음/제거 뒤 부재를 확인했으며 이 문서나 프로그램은 임시 파일에 의존하지 않는다.

## Evidence Gaps And Acceptance Impact

이번 승인한 공통 제어와 가능한 짧은 감독 backend 시험에는 비차단: 자동 scene/clear/도착/개체 사망/획득 생산자, 공통 버프 executor의 실제 scene 조합, 실제 포션 소비/보충·고갈 종료, 실게임140초 갱신 지속 운영, 느린 장비 비교, 네 방향/서모너 처치/열쇠 회수/방 반복은 미검증 또는 미구현이다. 후속 자율 운영/전체 PRD 완료에는 차단 조건이다. 이전 RUN-05의 실제 갱신 실패는 이번 주입 검증으로 해소한 live 성공이 아니다.

FC-01–FC-06의 한정된 실행 경계는 통과했고 자동 운영 확대나 전체 PRD의 수용을 선언하지 않는다. Route: `pass`; 다음 연결은 실제 관찰/동작 확인 생산자와 공통 제어 소비자의 감독 검증이다.
