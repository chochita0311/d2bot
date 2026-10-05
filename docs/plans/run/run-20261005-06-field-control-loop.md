# RUN-20261005-06: 필드 유지 입력 실행 루프

## Metadata

- ID: `run-20261005-06`; Status: `passed`; Attempt: `1`
- Feature: [FEAT-0008](../feature/feat-0008-field-control-loop.md)
- PRD: [PRD-0006](../prd/prd-0006-summoner-combat-and-loot.md)
- Spec: [SPEC-0007](../spec/spec-0007-field-control-loop.md)
- Profile: `backend-product`; Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`

## Direction And Current Evidence

이전 북쪽 조우 run의 종료 뒤 사용자가 연속 F2/F4 제어까지 개발을 지시했다. 새 run은 교정된 실행 제어 spec에서 시작한다. 이전 실제 140초 갱신 실패나 89개 계약 회귀를 live 성공으로 바꾸지 않는다.

Sky 표시 크기는1267x753, 프로그램 ScreenCapture 원본은1922x1140이다. 표시 크기로 native 변경을 추정한 초기 판단을 교정했다. Sky press_key에는 유지 시간이 없어 사용자가 승인한 프로그램 backend로 짧은 감독 시험을 실행했다. actual capture→HUD→포커스/독립 watchdog→표준 F2/F4 입력을 연결했다. field/주변 안전/장비/버프 효과와 목표 좌표는 감독 확인이고 자동 생산자 증거는 아니다.

## Selected Loop

Orchestrator→Spec→Builder→Contract→Functional. Primary owns architecture, source-of-truth and semantic evaluation; named read-only scout gathered existing hold/release/worker call paths. No live worker or legacy asset rename/migration.

## Results

공통 단일 키 소유자, 생존/버프/벨트/전리품 선점, 도착 적응, 최신 mailbox와 독립 해제를 구현했다. 새 HUD bridge는 같은 프레임 비율과 벨트 표시를 전달하고 최신 펼친 벨트 절대 잔량으로 포션 대기를 해소한다. 북쪽 기존 경로의 stale-fast/적/전리품/쿨다운/정지 해제를 수정했다. `BuffSequenceExecutor`는 프로필 순서/간격을 한 호출로 연속 실행하되 효과 확인 전에 타이머를 갱신하지 않는다. 기존 staged GUI와 자동 안전/개체 사망/획득 생산자는 아직 연결되지 않았다.

## Actual Probe Evidence

| 감독 probe | 실제 결과와 범위 |
| --- | --- |
| 초기 준비 | 15초를 넘긴 확인은 F2 이전에 거부. 초기화 뒤 stdin에서 최신 확인을 받는 방식으로 교정했으며 freshness 기준을 완화하지 않음 |
| buff 2 | 정확한7키 후 최대값1580/1543와 장비 복귀를 시각 관찰. 공용 hotkey callback 해제의 KeyError로 프로세스 종료1/보고 누락; 정상 종료 성공으로 기록하지 않음. 콜백 분리/전체 cleanup 시도로 수정 |
| travel 2 | 중앙 회색 지점에서 F2 한 번 keyDown, 약0.28초 후 마나 `glyph_ambiguous`로 keyUp. 실제 위치/화면 변화 있음, `observation_unverified`, 남은 키0/오류0. 요청0.5초 완주 성공은 아님 |
| buff 3 | w/a 뒤 HUD 모호로 중단하고 w 복원 입력. 전체 효과 미확인 |
| buff 4/5 | 하나의 호출에서 w/a/a/s/F1/d/w 연속 전송, `dispatched_unconfirmed`, 오류0. 이후 감독 화면에서 생명력1580/1580·마나1543/1543와 원래 장비 확인. 이때 backend press의 내부 pause와 probe의 최소0.4초 gap이 있었음 |
| combat 1 | 요청0.5초 중 약0.46초 F4 유지 후 정상 해제. 생명력1580 유지·마나1543→1500 관찰, 남은 키0/오류0 |
| combat 2/3 | 포커스 상실은 `no_input_focus_lost`; 다른 대기 프로세스는 오래된 준비/버프 유효시간을 보내지 않고 Ctrl+C 종료. 실제 키 입력 없음 |
| combat 4 | 요청1초, 한 번 F4 keyDown 후 약0.95초 유지하고 keyUp.14개 최신 HUD 표본에서 생명력1580 유지·마나1543→1500→1457 및 재생 증가 관찰. 반복 노바 시전 확인, `duration_complete`, 남은 키0/오류0 |

이번 유지 시험은 중앙의 이미 확인한 회색 패드에서 수행했다. 새 북쪽 몬스터 조우·처치·드롭·열쇠 회수나 four-wing 검증을 추가한 시험이 아니다. 포션/스크롤을 사용하지 않았고 앞선 확인 잔량11개를 소비 성공/획득으로 임의 변경하지 않았다. 이전 RUN-05의140초 실제 갱신 실패는 그대로 남는다.

유용한 이름 없는 HUD 두 장을 [공유 manifest](../../../assets/ui/survival/manifest.json) v4에 선별했다. 실제 노바 뒤1457/1543는 긍정 판독 사례, 이동 뒤 사람은1530/1543로 읽으나 CV는 모호한 사례는 거부 회귀다. 임계값을 낮추거나 모호한 그림을 glyph 학습으로 사용하지 않았다.

## Latest Build And Desktop Recovery

초기 성공 시험 이후 `ControllerFieldInput`에 SendInput 반환값 검사와20ms keyDown/keyUp finally 해제를 추가하여 press의 숨은 pause를 없앴다. 프로필은 각 토큰0.4초를 명시하고 probe는 해당값을 그대로 소비한다. 공통 연속 executor/HUD bridge/오래된 벨트 큐 거부도 이후 추가했다. 이 최신 빌드의 재시험을 별도로 수행했다.

새 버프 시험 준비에서 Sky 창 활성화 실패 뒤 한 번 복구 시도한 결과 `GetCursorPos: 액세스가 거부되었습니다 (0x80070005)`였다. 대기 프로그램을 Ctrl+C 종료하고 사용자에게 상태를 요청했다. 사용자가 기존 Python 동작을 지적하여 입력 없는 Python 캡처/포커스를 확인했다. 캡처는 성공했고 게임 포커스는 false였다. 기존 `focus_window`를 한 번 호출하여 실제 foreground 일치 true와 새 화면을 확인했고 Sky 관찰/조작도 다시 가능했다. Windows 잠금이나 Python 입력 실패로 확정한 근거는 없다. 방식 승인을 다시 요구하지 않았다.

목적지 행을 잘못 선택해 신비술사의 협곡에 도착한 것을 새 화면으로 확인하고 웨이포인트로 비전의 성역에 교정했다. 협곡에서 버프/전투 입력을 보내지 않았다. 첫 stdin 준비의 EOF는 입력 전 실패였고 tty 준비로 교정했다.

최신 **buff 6**: 하나의 연속 호출에서 w/a/a/s/F1/d/w,7번 모두 전송 성공/오류0/균형 복원. 첫 요청1791191659.127569, Orders 요청1791191660.9833739, 마지막 w1791191663.2281067. 요청 간격은 약0.48–0.90초로0.4초 대기에 캡처/판독·잠깐의 HUD 가림 대기가 더해졌다. 원본 after는 생명력1490/1580·마나1538/1543, 이후 최신 HUD는1580/1580·1543/1543. 감독자는 자원 증가·효과 외형과 인벤토리 활성 전투 세트를 확인했다. 입력 보고 자체는 여전히 `effect_verified=false`이며 감독 효과 확인과 구분한다. 공통 `BuffSequenceExecutor`는 주입형 검증이고 live probe는 별도 감독 전송 루프다.

최신 **combat 5**: 요청0.5초에 F4 한 번 keyDown→약0.46초→keyUp,7개 HUD 표본, 생명력1580 유지·마나1543→1500, `duration_complete`, 남은 키0/오류0. 확인된 Orders 갱신 시한을 probe 상한에 포함했다. 최신 backend 자체의 전송/해제 재검증이며 적 조우나 전체 제어기의 자동 의미 생산자 검증은 아니다.

최종 중앙 웨이포인트→Act 1 마을 귀환 뒤 생명력985/985·마나1075/1075·용병 생존을 확인했다. backtick으로 펼친 벨트는 포션3/4/4개, TP4개였고 다시 닫힘을 확인했다. 인벤토리 닫힘, 활성 입력/대기 probe 프로세스 없음. 의도적인 위험/0개 고갈은 실행하지 않았다.

## Evaluation And Review

- [Contract](../evaluation/eval-0015-contract-field-control-loop.md): PASS, 승인한 공통 계약 coverage complete.
- [Functional](../evaluation/eval-0016-functional-field-control-loop.md): PASS, coverage partial. 기능 평가 시129개 전체 검증·13개 Black/compile 종료0. 공유 reference13개 geometry와 선언한 SHA2563개 일치; 신규2장은 이름 없음.
- 기능 평가 시 문서18개 UTF-8/실제 한글·replacement 문자 없음, 상대 링크259개 존재를 named scout가 확인했다. 현재 상태는 Python 복구/최신 재시험 완료이고 이전140초 갱신 실패는 역사/남은 운영 검증으로 구분한다.
- 세션 마감 전 `docs-structuring`/`docs-shaping`의 읽기 전용 서브에이전트 리뷰를 반영했다. 공통 도착 적응 구현과 기존 경로 통합 미완료를 구분하고, 단계 요청 API·연속 버프 실행기·감독 probe의 역할을 명시했다. 실행 결과는 run, 표본 값·geometry는 asset owner로 연결하며 기존 실패·미검증 조건은 보존했다.
- 마감 검증:129개 테스트와 변경 Python24개 Black 종료0. 변경 Markdown53개 UTF-8/상대 링크446개 존재 확인, JSON5개 구문과 공유 PNG56개의 구조·manifest geometry/선언 checksum 확인. 커밋 후보138개에 private 자산·task scratch·검사한 민감정보 패턴 없음. 새 실게임 시험은 수행하지 않았다.
- FC-01–FC-05 공통 동작은 주입형 검증, physical 입력은 짧은 감독 probe에서 확인. FC-06에 따라 자동 안전/죽음/획득/전체 런 성공은 선언하지 않는다. 전체 PRD-0006, staged GUI 통합, 실게임140초 재버프 지속 운영은 미완료다.
- 현재 승인한 공통 제어/가능한 짧은 실제 입력 경계는 통과. 사람의 전체 제품 수용/다음 운영 확대 결정은 별도이며 요청한 backend 방식 승인은 유지된다. 다음 통합은 실제 scene/도착/안전 생산자와 버프·포션·종료 소비자를 연결하고 감독 재생으로 검증해야 한다.
- 필요한 공유 crop/요약을 옮긴 뒤 task 소유 원본/보고/진단·compile/formatter cache 약71.5MB를 정리했다. 실제 probe 프로세스0개, exact task 경로가 workspace 내부이며 reparse point 없음과 제거 뒤 부재를 확인했다. 영구 소비자는 scratch 경로에 의존하지 않고 별도 retained 진단 경로는 없다.

## Live Method Approval

사용자는 유지 입력 시험에 한해 준비한 프로그램의 표준 PydirectInput 백엔드를 직접 실행하는 방식을 명시적으로 승인했다. Computer Use JS API만 사용하는 스킬 지침의 해당 시험 제한을 사용자 지시로 변경한다. 최대 유지 시간/현재 화면·포커스·중단·정리 검증을 갖춘 짧은 감독 시험부터 진행하며, 승인만으로 자동 안전/처치/완전 런 증거를 대체하지 않는다.
