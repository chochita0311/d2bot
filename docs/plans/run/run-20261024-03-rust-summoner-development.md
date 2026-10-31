# RUN-20261024-03: Rust 서모너 개발 재개

## Metadata
- Status: `active`; Attempt: `1`; Feature: [FEAT-0031](../feature/feat-0031-rust-summoner-navigation-refinement.md); Active Spec: [SPEC-0031](../spec/spec-0031-rust-summoner-navigation-refinement.md); Profile: `backend-product`; Date: `2026-10-24`.
- Type/surface: `product/backend`; Required evaluators: `contract`, `functional`. Primary owns selection, implementation and final acceptance; named evidence workers only supply bounded read-only wiring/report evidence.

## Orchestrator Decision

현재 사용자 요청은 CPU 수치 개선 단독 작업에서 기존 서모너 개발의 Rust 실행으로 전환한다. 기존 FEAT-0013의 단계A/북쪽 단독10연속·성공/실패3회 점검을 재개한다. Python61번 실패 뒤 연속0과 포팅 검증의 별도 수를 보존하고 Rust-R01부터 새3회 묶음으로 기록한다. 사용자가 마지막 Act1 저장 종료를 확인했고 이후 코드 복귀도 같은 상태로 마쳤으므로 복구 확인은 다시 요구하지 않으며 실제 현재 화면/HUD/포커스는 새로 확인한다.

첫 변경은 ordinary reaction300/180→240/144의 설정/호버/전투 일치와 outbound의 현재 위치 대응 소실 시 화면 바닥의 굴곡 연속성을 사용하는 것이다. G 보고의 일부 조준 반전은 실제 F2 전송/변위가 없어 실제 왕복으로 세지 않으며 새 자료에서 구분한다. 현재 region 지형 자산은 계단 두 종류이므로 일반 portal 유형 분류가 이미 있다고 가정하지 않는다. 필요성이 확인된 공통 자산만 추가한다.

WGC/GPU·게임 내부 접근·TP/수동 마을 복귀·시한 완화는 없다. 소스/자산/개인 설정의 기존 무관한 변경을 보존한다. 새 동작의 실게임 결과는 아직 미검증이다.

## Preparation And Regression

일반 반응 반축240/144(기존300/180)의 색상 후보 crop·타원, pending 호버의 현재 범위 확인, 직접 target/visual 근접 전투를 연결했다. 별도 보스/정지 방어는300/180이다. 설정 로더가 새 필드를 버리는 문제를 검사에서 발견해 명시적으로 보존했고, 잘못된 값은 입력 전에 거부한다. 기존 설정 parity는 이 승인한 Rust 전용 필드만 비교에서 제외하고 별도 정책 검사가 실제 값/오류를 확인한다. 누락/오류 결과 자체는 변경하지 않는다.

현재 graph 위치가 불명인 outbound는 과거 pending port 대신 현재 floor route-flow를 사용한다. 실제 소스 시각·현재 바닥·기존 반전 재확인과 복귀의64px 목표 우선은 유지한다. 새 경로 선택 사유와 유효 반경을 report에 기록한다. 새 runtime 자산은 아직 추가하지 않았다.

정적 fmt/check/Clippy PASS 뒤 release 라이브러리102 PASS/5 ignored와 관련 integration33 PASS(설정10·field control3·field runtime12·호버3·경로 실행2·반경1·조향2)를 확인했다. 초기 test-only 오류(호버 제안0.5초 cooldown 미반영, parity helper의 오류 JSON 변경)를 수정한 뒤 해당 검사를 재실행했다. ignored native 검사는 이 수에 포함하지 않는다. EXE/실게임 검증은 아래 배치에서 별도로 기록한다.

G baseline은 outbound247frames/37.05초 관찰 span에서 hold_nova31·hover_probe13·post_combat_loot4, F2/F4 dispatch97/17이며 return147frames/20.40초에서17·5·2,38/7이었다. 관찰 frame 수와 첫/마지막 source span은 실제 전투 지속 시간이 아니다. 기존 proposal foot=(x,y+80)의 정확한 타원 판정에 새 반경을 대입하면 outbound130→95/131회, return79→58/79회로 줄었다. 여러 frame의 같은 후보를 포함하므로 적 수/공격 감소로 해석하지 않는다.

## Batch 1 — Rust-R01–R03

첫 후보 배포는659files/23,908,455B, Windows DLL만 의존하며 portable typed assets/dry3-room·fmt/check/Clippy가 통과했다. Flash/hell/SCREEN/240×144 설정으로 실제 GUI에서 반복3과 녹화를 시작했다. compiler/test와 실게임은 겹치지 않았다.

| Attempt | 실제 결과 | 집계/다음 행동 |
| --- | --- | --- |
| Rust-R01 | 포탈 유형 북쪽의 비서모너 상자 끝 `wing_end_observed`→중앙 `hub_arrived`→`north_room_closed`/Act1 선택 화면 확인. errors=[]; outbound539/return246 고유 source frame. | 단계A 정상 마감1, 연속1. 보스 처치/열쇠/전체 수용 성공으로 세지 않는다. |
| Rust-R02 | 새 방 계단 굴곡에서26frames 뒤 `opposite_wing_aim_rejected`/`field_paused`. seq21 floor-flow의 West 굴곡→22–25 graph 현재→26 다시 비국소화 때 detour가 초기화되어 반대 후보를 거부했다. 해당 반대 조준은 전송되지 않았다. | 실패1, 연속0. F10 뒤 held 입력 종료/녹화 마감/GUI exit0. 새 방3회 묶음은 아직2/3이다. |
| Rust-R03 | 새 방의 비서모너 상자 끝→중앙→`north_room_closed`/Act1 선택 화면 확인. errors=[]; outbound187/return159 고유 source frame. | 정상 마감2/실패1, 현재 연속1. 첫3회 검토를 완료하고 같은 계단 상태 전환을 보완한 다음 묶음으로 진행한다. |

R01 outbound는 첫/끝 source15.655–89.366(73.711초), return89.738–122.721(32.983초)다. hold_nova62/23·hover_probe16/10·전투 뒤 검사7/3이며 frame 수는 전투 지속 시간이 아니다. outbound153/539에서 current_floor_flow, return은 원래 graph를 사용했다. 유효 반경240/144가 보고서에 기록됐다. interval별 dispatch counter가 초기화되므로 phase 합계는 마지막 row 값으로 계산하지 않는다. report의 원래 `acceptance_room=false`/`acceptance_rooms=0`은 보존하며 위 단계A의 새 방 정상 마감 집계와 구분한다.

R02 복구1은 기존 north return으로 실제 중앙에 도달했지만 `central_reacquisition_required`로 정지했다. F10 후 중앙의 새 화면에서 같은 복귀 절차를 재시작했다. 이 복구는 개발 시도/성공에 더하지 않는다. TP/수동 마을 이동은 하지 않았다. 현재 원본 증거는 후보 package의 `recordings/summoner/evidence/gui-20261024-195904-13a6812b82/` 및 `probe-return-20261024-200409/`, 새 session scratch의 자원 로그에 있으며3회 검토·필요한 fixture 승격 뒤 정리한다.

## Batch 1 Review And Batch 2 Change

R02 복구2(`probe-return-20261024-200816`)는 중앙의 새 화면으로 시작해 `hub_arrived`→`act1_then_character_select_observed`/errors=[]를 확인했다. 이후R03까지 마치고 녹화/GUI exit0, 최종 선택 화면을 확인했다. 복구 두 번은 새 방 시도/성공에서 제외한다.

R03 outbound/return 관찰 span은27.710/20.325초, Nova44/0·호버9/7·전투 뒤 검사5/0frames였다. R01과 맵/종료 길이가 달라 빨라졌다는 인과 비교로 쓰지 않는다. 실제 phase source span 안의 key_down은R01 outbound216/30·return90/12(F2/F4), R02 10/0, R03 58/22·60/0이다. 이전 interval-local 최종 counter는 phase 합계가 아니다.

첫 묶음(R01+R02) 최대 WS608,141,312/private588,115,968/live heap476,455,946B, R03 최대487,264,256/467,742,720/376,835,644B다. 최종 idle은 각각127,799,296/105,299,968/21,222,746B,119,369,728/98,758,656/19,775,874B였고 GUI Drop 뒤R03 live heap은13,425,249B였다. peak가 이전G보다 커질 수 있으며 서로 다른 경로/길이의 관찰이므로 메모리 개선을 주장하지 않는다. 세션 종료 시 풀·runtime·녹화가 회수된 것과 peak 최적화는 구분한다.

첫 실패 원인은 안전 반전 보호 자체가 아니라 graph↔floor 전환 때 현재 굴곡이 지워진 것이었다. 현재 graph가 고른 통로를 floor-flow에 동기화하도록 보완했다. 같은 계단의 두 현재 viewport만 [region 참고 자산](../../../assets/regions/arcane-sanctuary/remastered-ko-1922x1140/README.md)으로 승격했고 runtime 배포에서 제외한다. 재생은 graph-current West→다음 새 바닥의 관측 South 굴곡을 허용하되250ms 화면을 거부하는 것을 확인했다. 고정 전체 지도나 무조건 역방향 허용은 추가하지 않았다. 다음3회R04–R06은 이 보정의 실게임 검증이며 아직 시작하지 않았다.

| 첫 묶음 지연(p50/p95, ms) | R01 | R02 | R03 |
| --- | --- | --- | --- |
| capture 시작→완료 | 26.343/46.641(n785) | 24.669/49.717(n26) | 26.169/42.762(n346) |
| source→근접 판독 | 59.454/80.219 | 64.459/85.982 | 59.260/77.383 |
| source→처리 완료 | 115.788/148.168 | 117.462/140.548 | 116.887/155.263 |
| source→새 F2 backend 반환 | 138.890/176.619(n140) | 154.228/156.837(n3) | 138.929/178.968(n64) |
| 판단 준비→새 F2 backend 반환 | 12.808/28.775 | 13.886/14.510 | 11.827/26.860 |

source 시각/캡처 sequence로 phase와 navigation alias를 중복 제거했다. 새 F2는 sent key_down 중 현재 held가 아니던 것만 세고 주기 재전송은 제외한다. 비음수 유한값만 사용하며 정렬 위치(n−1)×p의 선형 보간이다. 종료별 모수·장면이 다르고R02는26frames만 있어 대조 실험이 아니다. 이전G를 같은 계산으로 재집계하면394frames/near58.283/71.974, 처리116.998/180.157, 새 F2 138.096/172.096(n66), 준비→backend12.135/25.361이었다. 원래EVAL-0033의 freeze 수치를 변경하거나50ms 전체 반응 달성을 주장하지 않는다.

## Batch 2 — Rust-R04–R06

계단의 graph↔floor 보정 후 library103+관련 integration33 release PASS(5 ignored 제외), fmt/check/Clippy PASS, 새 EXE/typed portable assets를 확인했다. source239파일 `2ab2f30d64f8054ac745cdc1fad2a11c343078aa27f3a9e8dae7aceb147f0c0d`, EXE16,875,008B/SHA256 `8d32b1a9d4c4a7f0a3fd73eab661f0cd92edf63453926d2210a3e481b74baa0e`, 후보B659files/23,911,070B다. 새 테스트 참고2장은 배포되지 않는다. runtime manifest SHA256은 `3a1a70c31e663913c05af38b8a467ff50936cb8391e0cc41f3bb929d22311461`이다.

| Attempt | 실제 결과 | 후속 |
| --- | --- | --- |
| Rust-R04 | 계단 지형의 현재 서모너 발판 `summoner_platform_observed`/journal_and_surrounding_runes까지 도달. 전용 encounter79관찰에서 target/corpse0·Nova0·접근1, `kill_unconfirmed`. 입력 종료 전 errors=[]; 이후F10의 interrupted는 감독 종료다. | 실패, 연속0. 기존 north return1은 hub_arrived 뒤 Act1 이동 직전 `Current Arcane waypoint missing after bounded map restoration.`로 정지. 중앙의 새 runtime 복구2는 `north_room_closed`/Act1 선택/오류없음. |
| Rust-R05 | 현재 서모너 발판까지 도달했으나 encounter 첫 화면에서 `target_floor_unverified`, target/corpse0·Nova0·접근0. 후속 실제 화면의 파랑/금색 누운 외형은 아직 프로그램의 처치 승인으로 세지 않는다. | 실패, 연속0. 기존 north return1회로 `north_room_closed`/Act1 선택/오류없음. |
| Rust-R06 | 다른 북쪽 상자 끝 부근에서 `terminal_reacquisition_required`/field_paused, errors=[]. 종점 재획득 중 마지막 새 화면의 player는 유효했으며 미확인 종점으로 이동을 승인하지 않았다. | 실패, 연속0. F10 뒤 녹화/GUI exit0; 기존 north return1회로 Act1 선택/오류없음. |

두 번째3회는0정상 마감/3실패, 누적 실제 새 방2정상 마감/4실패다. 사망은 관찰되지 않았으며 북쪽10연속/보스·열쇠·협곡/남쪽 수용은 미완료다. R04–R06은 R02의 반대 조준 거부보다 뒤의 목표/전투 단계에 도달했지만 모든 계단의 수용으로 확대하지 않는다.

현재 진단 owner는 후보B `recordings/summoner/evidence/gui-20261024-202700-6d2d823507/room-01/`(R04), `gui-20261024-203733-67eea53a79/room-01/`(R05), `gui-20261024-204709-9449196f7b/room-01/`(R06)과 같은 owner의 복구 보고서다. 원본은 현재 판독 분석에만 필요하며 session 마감 전에 필요한 최소 자산/fixture 승격 뒤 정리한다.

R04의 encounter-before/after에 현재 룬 박스·각 part 오프셋의 정확한 검색 ROI를 복원해29외형을 독립 개발 OpenCV로 비교했다. native BGR 최고 생존 .678596/.662747, 시체 .610706/.610099로 각 기존 최종 기준 미달이다. 이는 전체 ROI 최대값이며 실제 반해상도 top16→국소 native 승인·유일성/색상 검사 결과와 구분한다. 전체 기준을 낮추지 않고 필요한 새 외형의 작은 공용 crop과 독립 양성/반대 상태·몸체 제거/복제 음성을 검토한다. R05의 목표 바닥과 R06의 종점 근거 소실은 별도 재생으로 원인을 확인하고, 안전/원래 시한을 유지한 보정 뒤 다음3회를 진행한다.

## Current Correction Work

2026-10-30 재개: 기존6회와 미완료 보정을 이어간다. 현재 목표는 R05의 안전 바닥 선택과 R04의 발판 중심/적 위치 혼동을 고친 뒤 R06의 현재 종점 근거 소실을 실제 저장 화면으로 검증하는 것이다. 북쪽 단독/3회 검토·TP 금지·연속0은 유지하며 새로운 실게임 상태는 입력 전에 다시 관찰한다.

R06 복구(`probe-return-20261024-205213`)도 기존 코드로 한 번에 `north_room_closed`/Act1 선택/오류없음을 확인했다. 모든 시험 GUI·녹화 process는 exit0이며 현재 게임은 Flash 선택 화면이다. 새 방은 재생 보완 전 시작하지 않는다. R05 복귀 GUI 정리 중 Computer Use의 직접 입력 갱신 알림은 해당 종료 창을 다시 관찰한 뒤 처리했으며 당시 새 게임 실행은 시작하지 않았다.

입력/캡처 없는 [Rust 진단 example](../../../examples/summoner-frame-diagnostics.rs)을 추가해 실제 현재 목표/외형/바닥과 종점 full/static/relocalization을 읽는다. R05 현재 목표[1449.5,570]에 기존240px 선호는[1196,568]을 골라 목표와253.508px 떨어져100px 검사에 거부됐다. 보스 접근의 선호만 원래 최대 후보800px로 맞추면[1452,560]/10.308px로 같은32px 안전 바닥 검사를 통과한다. 후보 상한·최종100px·2회 접근/입력·시간 보호는 바꾸지 않았다. 후속 release 결과는 아래에 기록하며 이 보정의 실게임은 아직 미검증이다.

R04 후속 정지 화면의 시체는 기존 `summoner-robe-corpse-lower-band-right`로 native .990331/ground[1002,639]가 나온다. R05 처음 encounter는 생존 서모너였고, 나중 정지 화면의 누운 외형과 시간상 구분한다. R05 before/after는 동일 SHA256 `da7a1168ee2b14d44fdea20b48227be9d1a05d02ab5b825a1de60b963f396899`이며 둘을 독립 시체 관찰로 세지 않는다. 이 최초 생존 화면의 기존 최고 BGR .862990도 .92 미달이다. 기존28참고 화면의11생존/17시체 양성을 새 자산 검토 전에 동결했으며 새 후보는 그 양성을 잃거나 반대 상태를 만들지 않아야 한다.

2026-10-30 오프라인 보정: R04는 발판 중심[1161.5,714]이 player[962,580]의 전투 타원 경계에 걸려 외형 미확인 호버만 계속했다. 외형/현재 hostile이 없을 때만 목표100px 이내로 현재 바닥 접근 후 호버하도록 바꿨다. 현재 외형의 전투300/180·시체 확인·2회 접근·12초/250ms·커서/해제는 그대로다. 입력 없는 encounter7검사 PASS(원래 동작 비교는 새 미확인 진단1장만 정확히 제외), R05 실제 바닥/공유 설정 불변과 자원/정리 오류를 포함한다. 추가 증거는 기존4화면 deque에서 한 장만 골라 총24장 안에서 보존하며 새 캡처/상시 buffer를 만들지 않는다.

R06의 최초 현재 종점source45.024815/seq223 뒤45.1641938/seq224에서 위치 대응이 달라지고 배경 motion은 correspondence_insufficient였다. 마지막 F2는 최초 source 직전45.0242604이며 이후 추가 F2는 없다. 이동의 물리 원인은 확정하지 않았다. map-first-hidden45.7523203/seq226(원래 기준+0.7275053초)에는 왼쪽 금속 상자만 남고 나머지 두 상자는 화면 밖이다. 원래 두 부위 재국소화는 null이다. 새 보조 floor loop는 최초 현재 상자3개와 같은 화면에서 기하로 연결한 최대3개 metadata reference로만 쓰며 최초 분류/도착은 승인하지 않는다. 실제 두 화면의 접근 복원·한 부위 제거 음성·원래2초 만료/reset 회귀가 debug PASS했다. 이 검사는 새 라이브 도착/마감 성공이 아니다. 바닥 후보의 첫 crop은 기존 기하 또는 native .92에 미달해 폐기했으며 임계값을 낮추지 않았다. 최종 runtime crop은56×42px 한 장이고 세 참고 fixture는 runtime에서 제외한다. Release 전체/새 EXE는 현재 검증 중이다.

재개 시 Computer Use의 첫 게임 관찰 승인이 timeout됐으나 재시도는 성공했고 Flash 선택 화면을 확인했다. 이 동안 실제 게임 입력은 없었다. 다음 실제3회는 새 release 검증 후 시작한다.

실게임 전 호출 경로 검사에서 기존10점 호버 격자가 모두 player 위쪽/동일 y를 훑는 것을 확인했다. R04/R05처럼 발판보다 아래에 선 서모너의 몸통을 위해 현재 발판100px 접근 뒤에만 최대5개의 body 후보를 사용한다. 기존 body→foot+80px·300/180 타원/화면 경계를 통과한 현재 후보와 새 패널/50ms/250ms/실제 커서·0.5초 제안 cooldown이 필요하다. 후보 자체로 Nova/사망을 승인하지 않는다. 발판 소실 때의 원래 근접 방어와 corpse 대기는 그대로다. 새 row `platform_hostile_proposals`로 실제 소비를 진단한다.

이 마지막 호버 보정 전 release 전체 기준은 library106+integration233(51 integration result groups) =339 PASS/0 fail/6 ignored였다. 보정 뒤 관련 debug8 PASS, 최종 release library107+field-runtime12+hostile-probe3+life1+terminal8 =131 PASS/0 fail/5 ignored를 재검사했다. 바뀌지 않은 나머지 integration은 그 직전 기준 실행이 검증한 범위이며 최종 소스 전체340개를 다시 실행했다고 주장하지 않는다. fmt/check/all-target Clippy는 마지막 소스에서 PASS했고, 명시 native fixture/GDI balance/listener3 및 읽기 전용 native capture1도 PASS다. 최초 두 검증 worker 세션은 결과 집계 없이 중단되어 어떤 통과 수에도 더하지 않았다. 실제 최종 EXE와 자원 제어판 링크는 진행 중이다.

## Candidate C And Batch 3 — 2026-10-30

C 링크/패키징 완료: `dist/d2bot-windows-x64-summoner-dev-c`,660파일23,945,765B(659 payload23,815,377B+manifest). EXE16,899,072B/SHA256 `a0d4b1fd8f5463fc7eed1a1a99d85553ba1a65e5fee43fc03b9f4af261daa084`, package manifest `8f7e0269451b3a37a5f6009bdef509d7388335d278c0545c88f8be5b9a5e25c1`, terminal rules `3941a4e6bd6279a05a262c1257762bdab89edec11b8954ec1919e2b7d7db04d3`이다. source239파일은 모든 src 파일+Cargo.toml/Cargo.lock의 상대 POSIX경로/공백/파일SHA/LF를 정렬해 집계한 `aa4b64824e08cacaa3bed9d193decb055ac72b017e570a57f0de68e3be365a19`다. 이전 집계와 알고리즘 동일성을 추정하지 않는다.

659 payload의 길이/SHA·경로 containment/미등록0·외부 CWD typed loader(profile3/config path5/input0/capture0) PASS다. Python/OpenCV/FFmpeg/Xvid/비시스템DLL은 없으며 필수 region과 작은 새 floor template이 포함된다. 참고 이미지는 제외한다. 실제 C EXE의 Flash/SCREEN/Idle GUI를 전면에서 확인하고 정상 exit0했다. 이후 측정은 같은 production backend를 사용하는 기존 resource-probe 제어판으로 구분한다. 기본 배포 config는 변경하지 않고 task-owned config-c에만 Flash 우선/SCREEN/exact title을 사용한다.

창 접근 승인 동안 미해결 외형3장/중앙WP2장을 기존 region의 개발 회귀로 승격했다. C freeze 뒤의 이 추가는 참고 파일/출처 metadata/회귀 검사뿐이며 runtime 규칙·template은 바꾸지 않았다. 살아 있는 두R04에 거짓 corpse가 없어야 하고 뒤R05 시체를 앞의 용병과 혼동해 alive로 읽지 않아야 한다. 외형1검사 PASS와 hub/ground 등11검사 PASS, near point의 잘못 적은 기대값961/503을 실제 blue candidate966/502로 고친 뒤 해당 hub1검사 PASS다. 임계값은 바꾸지 않았다. 실패R04 도착 화면은 full hub null/upper-left .8413194 한 개/near[966,502]이며, fresh retry는 left .9852917+upper-left .9073086/full[961,503]다. 전자는 나중 TownFinish 실패 순간 자체의 사진이 아니므로 물리 원인 확정으로 확대하지 않는다.

시작 환경 검사 `gui-20261030-154917-ab83f3335e`는 entry의 foreground 전환 실패로 방 생성 전에 종료됐다. report input_events=[]/errors는 Arcane entry failed지만 포커스 helper 자체의 기존 Alt fallback까지 input0이라고 주장하지 않는다. 녹화/GUI를 정상 마감(exit0)한 뒤 같은 binary를 Windows sandbox 밖에서 실행하자 창 전환/Act1 입장/Arcane 이동이 성공했다. 실행 환경 차이로 해소했으며 focus FFI를 변경하거나 권한 실패를 무시하지 않았다. 이 시작 실패는 실제 새 방3회에 더하지 않는다. 이후 live 제어판은 명시적으로 sandbox 밖에서 실행한다.

| Attempt | 결과 | 후속 |
| --- | --- | --- |
| Rust-R07 | 실제 새 방 북쪽 계단→현재 서모너 발판 도달. encounter3관찰/접근2/target·corpse·Nova0, kill_unconfirmed. 목표 거리는491.60→131.31→126.80px로, 새100px 목표 접근과 기존2회 상한이 만난 상태에서 종료됐다. | 실패/연속0. F10 감독 종료 뒤 녹화/GUI exit0. 기존 north return `probe-return-20261030-160329` 1회로 north_room_closed/Act1 선택/errors=[]; 복구는 새 방 수에서 제외한다. |
| Rust-R08 | 같은C로 아케인 입장/버프 뒤 약290 nav frames 탐색. source59.03 전투 뒤 전리품 검사에서 라벨이 초상을 가려 mercenary_unverified, OFF 정리도 같은 이유로 막혀 labels ON/item_labels_not_off로 중단했다. 뒤 현재 감시도 미확인이라 기존 현재 메뉴 안전 종료가 character_select_observed로 완료됐다. | 실패/연속0. report operator_review_required=mercenary_portrait_unverified; 사람의 새 상태 확인을 기다린다. 사망으로 단정하거나 자동 부활하지 않는다. 현재 선택 화면의 저장 지역은Act2다. 녹화/GUI는 정상 마감(exit0). |
| Rust-R09 | 아직 미시작. | 세 번째 묶음은2/3이며, R08 안전 정리 보정과 사람의 재개 확인 후 마지막1회를 진행한다. |

R07 원본 owner는 `gui-20261030-155602-e94593fb90/room-01/`이다.254고유 nav frame/source16.687–53.041(36.354초), hold_nova59/probe13/전투 뒤 검사7이며 nav 실제 F2/F4 key_down82/32다. 별도 encounter 접근은 source age228.147/231.053ms에서 sent였다. 이후 paused-survival Nova는 전용 encounter0과 구분한다. 이 방의 초기 화면은 생존 서모너였고 뒤 누운 외형은 아직 프로그램의 두 새 corpse 승인으로 세지 않는다. 실제 R07 최대 WS452,247,552/private433,119,232/live heap335,737,365B, 마지막 timed idle121,061,376/101,801,984/19,591,277B, GUI Drop112,812,032/89,477,120/13,478,376B다. 다른 장면/실행 환경·정지 방어 시간을 포함하므로 이전G 대비 성능/메모리 개선의 인과 비교로 쓰지 않는다. 누적 완료 집계는2정상 마감/5실패, 연속0이며 북쪽10연속과 남쪽은 미완료다.

2026-10-30 이전 원시 진단 정리: R01–R06의 필요한 경로/접근/종점 fixture와 외형 반대 상태·중앙WP 회귀를 정식 assets owner로 승격하고 실제 소비 검사를 마쳤다. 모든 관련 GUI/녹화 writer가 정상 종료한 뒤 구 후보A/B 전체와 이전G benchmark/중앙 원시 진단의 정확한 네 경로를 containment/reparse 검사 후 제거했다(1906files/2,158,961,792B). 기본 G 배포와 사용자 source/config는 보존한다. 앞 절 A/B 원본 경로는 당시 실행 위치이며 현재의 재생 의존 경로가 아니다. 현재C의R07–R08 및 새 보정용 scratch는 진행 중 진단/검증이 끝날 때 같은 규칙으로 정리한다.

R08 보정은 안전 실패여서3회 완료 전에 분리 적용한다. 기존 prepared+ON은 이 controller가 정상 현재 HUD 확인 뒤 켠 상태라는 소유 관계를 사용한다. 현재 건강 비율/닫힌 벨트/패널 없음/포커스/Stop/250ms를 유지하고, 오직 자기 라벨 OFF 정리에만 초상 미확인 예외를 둔다. OFF 뒤50ms/새 전체 HUD(.75초 제한)를 통과하기 전에는 별도 pending 상태가 이동을 막는다. 여전히 미확인·저자원·패널·오래된 화면·포커스/중단·키 전송 오류면 이동 금지/원래 안전 종료를 유지한다. 재확인한 중단 검사는 inspection_interrupted/현재 시각으로 남기며 완료/획득/처치로 채우지 않는다. portrait template .92/가시50%는 바꾸지 않았다.

원래 conditional-loot parity3검사, 새 OFF/전체HUD 재확인 안전 회귀와 실제R08 HUD 회귀가 debug PASS했다. 새 회귀는 복원 뒤 계속 미확인·저자원·패널·250ms·Stop/포커스·OFF 전송 실패·정리 전 다른 위험의 입력 거부를 포함한다. 실제 HUD의 생명/마나1.0과 용병 미확인을 그대로 재현하는 작은 참고 파일은 survival owner로 승격했으며 runtime에는 배포하지 않는다. D 후보의 fmt/check/Clippy PASS, 관련 release 재검사는 진행 중이다. 현재까지 실제 새 방은2정상 마감/6실패/연속0이며 남쪽 단계는 시작하지 않는다. 이전 commentary의 ‘입장 직후’ 표현은 전체 보고서를 확인해 탐색 뒤 전투 후 검사로 정정했다.

## Checkpoint D — Operator State Confirmed

D의 관련 release164 PASS/0 fail/5 ignored(라이브러리107+통합57,11 result groups), fmt/check/all-target Clippy PASS와 실제 EXE/examples 링크를 완료했다. 단일 HUD 진단 frame의 소유권 전달 후 픽셀이 해제되는 회귀를 숫자·지속 초상 미확인·복원 후 재확인으로 확장해 해당1검사를 다시 통과했다. 새 테스트만 확장한 뒤 fmt/Clippy도 다시 통과했다. 전체342개를 최종D에서 다시 실행한 것은 아니다.

현재 검증된 수정판은 `dist/d2bot-windows-x64-summoner-dev-d/`다.660files/23,955,242B, EXE16,901,632B/SHA `e4d9465043112f9b6f2b069f4e786885442ada2f067e2beb08790c229dc482e0`, source239집계 `c813392561ee82d5569af50d6c4ae86228f65151f25b73166e1b920b37b30e95`이며 집계 방식은 위C와 같다. package manifest SHA `5586c033729564caf311201473c375b59866b9df08c7f7d5d5b3f4b54b256f10`다. payload659개 길이/SHA·containment·미등록0·실제19 Windows DLL imports 일치·외부 CWD diagnose/typed assets(profile3/config5/input0/capture0)/north3 dry PASS다. 필수region과 작은floor crop이 포함되며 HUD/경로 참고는 제외한다. 기본G 배포는 바꾸지 않았다.

R08은290고유source/source15.809–59.031(43.222초), Nova95/probe22/전투 뒤 검사11frames, 실제 nav F2/F4 keydown75/45였다. 마지막 frame의 F4 held snapshot 뒤59.1145984 keyup이 기록됐다. 안전 종료는21관찰로61.6302543에 선택 화면을 확인했다. 최대 WS421,453,824/private399,851,520/기본힙312,175,569B, 마지막timed idle119,648,256/95,907,840/19,613,608B, GUI Drop112,009,216/90,292,224/13,484,343B다.

최종 계약/기능 판정과 두 방의 지연/녹화 검증 범위는 [EVAL-0034](../evaluation/eval-0034-rust-summoner-navigation-refinement.md)가 소유한다. C의 source→처리 p50/p95는R07 116.046/145.994ms, R08 113.836/150.107ms이고, 새F2 backend 반환은149.029/169.410(n34),125.203/184.150(n41)ms다.50ms 달성이나 D 실게임 개선은 주장하지 않는다. 실제 수동 AVI4개는8fps/1922×1140/양의frame수와 첫·마지막 decode를 확인했으며 모든 중간frame품질/drop수 검사는 아니다.

모든 시험 제어판·녹화 process는 exit0로 닫았고 실제 입력은 중단했다. 마지막 감독 관찰은 Flash 선택 화면/Act2 저장 지역이다. 새 R08 용병 미확인 안전 종료의 사람 상태 확인 질문은 아직 미답이며, R09나 새 방/마을 입력을 시작하지 않는다. 확인 뒤 현재 장면/HUD/포커스를 다시 읽고 D로 마지막1회를 진행한다. 기존 Hell entry는 Act2 시작일 때 검토된 Nightmare/Hell 준비 reset을 사용하도록 설정되어 있으며, 그 준비 방은 실제 아케인 새 방 수와 구분한다. 사람 확인이 Act1 저장 종료이면 그 현재 상태로 시작한다. TP나 임의 마을 보행을 넣지 않는다.

대기 경계의 정리도 완료했다. R07/R08 요약·지연/자원·녹화 검사·한계는 이 run과 EVAL-0034에 남기고, R08 실제 HUD 등 필요한 회귀는 정식 assets owner와 검사에 연결했다. 알려진 writer의 exit0와 현재 시험 창 부재를 확인한 뒤 후보C 전체794files/949,509,961B와 task scratch108files/11,945,778B를 절대 경로/containment/reparse 검사 후 제거했다. 위 구 후보/G owner 정리까지 합계2808files/3,120,417,531B이며 D EXE SHA는 정리 후에도 일치한다. 현재 보존하는 task 임시 경로는 없다. `config-c`와 이전 raw 경로는 당시 실행 위치이며 재개 의존 경로가 아니다. 다음 입력 없는/실제 시험의 overlay는 현재 source config에서 필요할 때 다시 만들거나 D GUI에서 Flash·SCREEN·실제 D2 제목을 선택한다. 기본G 배포·D 수정판·사용자 source/config·보통 Cargo target cache는 보존한다.

2026-10-30 사람의 새 확인: **용병 정상·Act1 저장 종료**. R08의 재개 경계는 해소됐으며 현재 Flash 선택 화면을 새로 관찰했다. D source/EXE는 그대로 두고 북쪽R09 한 회를 녹화와 병행한다. 현재 task overlay/log owner는 `target/tmp/summoner-development-20261030-r09/`이며 D config 사본의 정확한 게임 제목/SCREEN만 지정한다. 실게임 시작은 GUI에서 Flash/지옥/반복1을 확인한 뒤 진행한다. 세 번째3회 검토 뒤 필요한 보정을 결정하며 TP/임의 마을 보행을 넣지 않는다.

## Batch 3 Complete — R09 And Evidence Memory

R09(`gui-20261030-174530-67d22ddb48/room-01`)는 D production backend를 쓰는 resource-probe GUI로 녹화 병행 실행했다. 북쪽 포탈 지형의 현재 종점→중앙WP→Act1→저장 종료가 `north_room_closed`, errors=[]로 완료됐다. `acceptance_room=false`인 북쪽 개발 시험이며 보스/열쇠/협곡 성공은 아니다. item_labels_final=off, loot15검사의 OFF 전송·복원 오류0을 확인했다. HUD 재확인 예외 metadata가 없어 R08의 실제 가림 상태에서 새 예외가 실행됐다고 주장하지 않는다. 새 operator_review_required는 없다. 녹화/GUI 정상 종료(exit0), 새 입력 중단 상태다.

세 번째 묶음은1정상 마감/2실패, 누적 실제 새 방3정상 마감/6실패/연속1이다. outbound542frames/source15.665685–89.6000409(73.934356초), return394frames/source89.9164479–146.7075548(56.791107초)다. 두 phase와 navigation alias를 중복 제거하면936frames다. 실제 F2 down/up369/192, F4 down/up62/28이며 두 phase Nova68/59frames·전투 뒤 검사8/7frames다. 포탈 지형의 반복 조준/현재 위치 대응과 R07의127px 부근 접근은 여전히 검토할 문제다.

R09 지연 p50/p95(ms): capture26.056/43.121, source→near60.777/81.397, source→processed117.537/148.844, source→새F2 backend138.698/183.233(n192), ready→backend12.699/28.631. 앞 두 방과 같은 dedup/held 규칙이며 다른 지도/길이여서 인과 전후 비교가 아니다. 자원 최대 WS665,993,216/private650,838,016/기본힙533,771,834B; 마지막 timed idle111,222,784/107,368,448/21,139,591B; GUI Drop99,880,960/95,670,272/13,482,750B다. 실제 capture backlog는 최신1장, 수동 녹화는 동기1frame write이며, nav 선택 진단의 Arc frame 목록이 interval/방 종료까지 누적되는 경로를 확인했다.1922×1140 BGR 한 장은6,573,240B다.

승인된 자원 효율 범위에서 native nav 진단을 단일 PNG worker/대기2장으로 분리하는 보정을 진행한다. submit은 큐·disk를 기다리지 않고 포화된 선택 진단은 파일명과 partial 상태로 명시한다. 기존 입력/판독/수동 녹화·전용 encounter/비상 증거의 계약을 보존하며 실제 I/O/worker 실패는 정리 오류로 보고한다. 작은 mutex에는 파일 I/O/worker join을 넣지 않는다. 재생 환경은 원래 Vec 경로를 유지한다. 다음 묶음은 관련 정적/회귀 검사와 새 EXE 링크 후 이 보정의 실게임 자원을 측정한다.

## Checkpoint E — Bounded Navigation Diagnostics

선택 PNG를 native 연속 탐색/복귀의 각 step 뒤 drain해 한 worker/대기2장에 넘겼다. 성공한 enqueue의 이름만 interval metadata에 넣으며, 이전 interval의 임시 목록을 새 interval에서 지운다. 포화 생략/실제 저장 실패/worker panic/닫힌 채널을 구분한다. 입력/캡처/관찰 정리 후 송신을 닫고 잠금 밖에서 join한다. 모의/재생의 Vec 경로·원래 입력 순서/결과는 유지한다.

release110 library+continuation2+field-preparation16+field-runtime12+recording4=144 PASS/0 fail/5 ignored다. 이 release compile 뒤 Clippy용 if 결합/TestResult 표기와 포화 큐의 반환 파일명 assertion을 정리했으며, 최종 그2개 개발 검사/fmt/all-target Clippy PASS와 새 EXE/examples release 링크를 확인했다. 전체 최종 소스의 모든 검사를 다시 실행했다고 확대하지 않는다. 최초 check의 incremental hardlink 경고는 파일 복사 fallback이었고 이후 검사는 CARGO_INCREMENTAL=0으로 실행했다.

후보E `dist/d2bot-windows-x64-summoner-dev-e/`는660files/23,998,250B다. EXE16,944,640B/SHA256 `66b8be360915d26a33dd59bb4372d02d6b2df7b7cc20118cd659b0274b7f7bcf`, manifest `06b3ba4d09f99f42a9b67242ffa492976898b2eda3f8655f2a6b1d89fceef030`, source240파일(같은 상대경로/파일hash/LF 알고리즘) `fcd91d715560b10c80db3ade19b7fe6d9f53a094c1bffbfc176a983262bd1011`이다. 모든659 payload 길이/SHA·경로 containment·미등록0·19 Windows DLL import 일치, 필수4region/기존 floor template 포함, 외부 CWD diagnose/typed assets3profiles5paths/input0/capture0/north3 dry PASS다. Python/OpenCV/FFmpeg/Xvid runtime은 없으며 라이선스 고지는 runtime binary와 구분한다. 기본G와 검증한D는 보존한다.

E production backend resource-probe GUI에서 Flash/지옥/SCREEN/exact title/새 방3회를 확인하고 녹화 `diablo2_20261030_181416.avi`와 함께 R10부터 시작했다. 추가 앱 승인이나 새 상태 질문은 필요하지 않았다. 녹화 시작 첫 UI 호출의 geometry unavailable은 현재 창/녹화 미시작을 다시 관찰한 뒤 한 번 재시도해 해소했다. 현재 자원 로그는 task owner의 `e-r10-r12-resource.log`다. R09 녹화는1483frames/8fps/1922×1140/158,757,150B와 첫·마지막 decode PASS이며 실제 wall time 대비drop/전체 중간frame 검사는 아니다.

## R10 — Current Stair Guard And Diagnostic Burst

R10(`gui-20261030-181429-c9a0200ac8/room-01`)은 E로 실제 새 방 한 회만 실행했다. 북쪽 탐색1189고유frame/source15.4229379–177.5634198(162.140482초)에서 `field_paused/opposite_wing_aim_rejected`로 정지했다. 종점 승인·복귀 전환/phases는 없었다. 초반 commentary의 ‘상자 끝 확인 후 복귀 중’은 실제 report를 확인해 **종점 부근이 보였지만 아직 탐색 중**으로 정정했다. F10 감독 종료 뒤 root interrupted/error=[field input interrupted or game unfocused]는 앞의 길찾기 중단 사유와 구분한다. 새 operator_review_required는 없고 현재 생명/마나 정상·사망 미관찰이다. R11/R12는 시작하지 않았다. 네 번째 묶음1/3, 누적3정상/7실패/연속0이다.

마지막 current player[962,578], global north[1326,523], current floor frontier[270,539]/bend_entered, graph chart28 unlocalized였다. flow는 y×2의 등각 투영으로128.6도를 선택했지만 입력 직전 raw 각도150.2도가150도 보호에 걸렸다. 과거 port가 아니라 현재 floor 후보임을 확인하고 이 서로 다른 각도 기준을 같은 함수로 맞추는 보정을 진행한다. 진짜 반대·오래된 화면과 원래 실제 조준/입력 재확인은 유지한다.

E stream31장 enqueue/31장 저장/저장 오류0, 생략28장으로 evidence partial이다. 최근 회전8장을 interval 종료에 한꺼번에 넘겨 시작/마지막 이미지도 포화로 생략된 것을 확인했다. null final_frame은 없는 파일을 성공으로 기록하지 않은 결과다. 정지 뒤 GUI Snapshot으로 현재 동일 계단을 별도로 저장해 관찰했지만 이 화면을 원래177.563초의 정확한 source로 재라벨하지 않는다. 보정은 native 최근 회전2장/시작·마지막 우선 제출이며, 버프 단계 화면도 같은 worker에 즉시 넘겨 갱신마다 원본을 쌓지 않는다. 예외 metadata 명칭은 diagnostic_evidence_stream으로 일반화한다. 원래 재생 Vec/8장 순서는 유지한다.

R10 p50/p95(ms): capture27.443/43.165, near61.372/81.093, processed113.522/141.867, 새F2 backend137.436/182.233(n184), ready→backend12.109/27.545. F2 down/up476/184, F4 51/21; loot12검사 모두 labelsOFF/복원 오류0이다. peak WS557,461,504/private537,227,264/기본힙452,483,386B, 마지막timed idle133,783,552/116,420,608/21,870,879B, GUI Drop118,661,120/99,012,608/13,652,946B다. 지도·수행 단계·시간/정지 방어가 달라 R09 대비 인과 개선 수치로 쓰지 않는다.

F10 뒤 수동 녹화와 E GUI를 exit0로 마감했다. 기존 `probe-return-20261030-182342` 한 번으로 north_room_closed/Act1 저장 종료/errors=[]를 확인하고 복구 GUI도 exit0로 닫았다. 복구는 새 방 집계에서 제외하고 TP/수동 마을 보행은 없었다. 복구의E stream은6저장/8생략/오류0이며 같은 종료 burst의 참고 증거다. 현재 복구/시험 writers는 모두 닫혔고 게임은 Flash 선택 화면이다.

## F Correction In Verification

입력 직전 역방향 검사를 path flow와 같은 등각150도 함수로 바꿨다. R10 벡터 허용/진짜 역방향 거부/오래된 화면 거부와 native 최근2회전·시작/마지막 우선 제출 회귀가 개발 PASS다. 버프 trace는 동시에 두 출력 owner를 지정하지 못하게 하고 native에서는 각 단계의 frame을 같은 worker에 넘긴다. before/after trace·포커스 상실·stale·callback 오류에서 기존 입력/시각/결과가 유지되고, 단계 사이와 종료 뒤 Weak frame 해제가 확인됐다. 실제 I/O 실패는 기존 writer 결과로 남기며 callback의 메모리 전달이 저장 성공/버프 효과로 바뀌지 않는다.

전체 library 개발 검사에서 기존 renderer 없는 UI 배치 검사가 egui/epaint0.37의 미소비 TexturesDelta debug assert에 걸렸다(111 PASS/1 FAIL/5 ignored). 이는 이번 게임 제어 변경과 별도인 테스트 renderer 소유 문제다. 설치된 tagged crate의 clear 계약을 확인해 이 검사에서만 texture delta를 명시적으로 clear하도록 수정했고 재검사 중이다. production GUI 렌더링/자산/게임 입력은 바꾸지 않았다. 아직 F release/실게임을 통과했다고 선언하지 않는다.

수정 뒤 전체 library112+field-preparation17=129 개발 PASS/0 fail/5 ignored, fmt/all-target Clippy PASS다. 현재 최종 source는 고정했고 release EXE/두 examples 링크와 관련 release integration을 실행한다. 이는 E에서 수행한144 release 전체 그룹이나 F의 최종 internal library release 재실행과 구분한다. F의 실제 첫/마지막 PNG·버프 전달/메모리·새 방 움직임은 다음 실제R11/R12에서 확인한다.

F 최종 release integration은 continuation2+field-preparation17+field-runtime12+recording4=35 PASS/0 fail/0 ignored이며 최종 EXE/resource-probe/typed-loader 링크가 완료됐다. F internal library의 최종112개는 개발 profile에서 확인한 범위이고 release로 다시 실행하지 않았다. 최종 source240 SHA256은 `f9e2dcb77ac6e0c50a96175e21924cef24e8a378c03427b21d2aac22296869ba`다.

후보F `dist/d2bot-windows-x64-summoner-dev-f/`는660files/24,014,122B(659 payload23,883,734B+manifest), EXE16,960,512B/SHA `0c0fbff6d70b2c4158bbe08a23d5dc40d427daa2e6aaaaa2fe11180165f26661`, manifest SHA `6b5a8ff6f1830f7b5bf3b5405df6d05e51d88708e2971cc142399484165b6625`다. payload경로/길이/SHA·미등록0·실제19system DLL imports 일치·필수4region/기존floor crop·금지runtime 없음이 확인됐다. 외부CWD의 actual EXE diagnose/north2 dry와 typed assets3profiles5paths/input0/capture0 PASS다. 새 실제 GUI/플레이는 production panel/backend를 그대로 감싼 resource-probe로 구분한다.

Flash 선택 화면을 다시 관찰한 뒤 F backend의 Flash/지옥/SCREEN/exact title/반복2와 수동 AVI `diablo2_20261030_185455.avi`를 확인하고 R11부터 실행했다. 현재 owner는 F의 `recordings/summoner/evidence/`와 task `f-r11-r12-resource.log`다. E 녹화는3001frames/8fps/1922×1140/220,063,312B·첫/마지막 decode PASS다. R10 정지 뒤 GUI Snapshot은E `assets/private/gui_snapshot.png`이며 현재 실패 지형을 육안 확인하는 데만 썼다. 새 source regression이 이 사진에 의존하지 않으므로 진단 종료 후 정리한다.

## Batch 4 Complete — R11/R12

R11/F(`gui-20261030-185508-2529aa2858/room-01`)은938 outbound frames/source16.9399888–139.644094(122.704초)에서 현재 종점 승인 후 복귀로 전환했다. return158frames/source140.1008119–165.0031446(24.902초)에서 `stuck_no_observed_escape`로 정지했다. localized chart1/place6의 유일 north entry가 실패/방문 이력에 걸려 `graph_entry_retreat_exhausted`였으며 현재 실패 조준을 제외한 대안이 없었다. F10 종료 뒤 root field_paused/errors=[]·새 사람 review 없음, 녹화/GUI exit0다. 이 실패는 R10의 등각 불일치와 다르다.

R11은1096고유frames/capture25.882/43.546, near59.261/77.260, processed112.287/145.980, 새F2 backend138.082/178.578(n152), ready→backend11.848/28.786ms다. stream49저장/회전 참고4생략/오류0이며 원래 시작/마지막 PNG가 실제 존재한다. 버프PNG14장이 각각 저장됐다. peak WS461,402,112/private440,135,680/기본힙352,802,947B, 마지막idle127,766,528/109,948,928/21,682,103B, GUI Drop116,482,048/93,835,264/13,709,026B다.

R11 별도 `probe-return-20261030-190233`은 같은 시작 부근에서 north_room_closed/Act1 저장 종료/errors=[]로 한 번에 마쳤고 GUI exit0다. 새 graph는 chart0/place0/방문·실패0에서 west 대안을 골랐다. 버프7입력/현재 건강·라벨OFF·닫힌 패널 조건은 같지만 복구 모드는 merc monitor가 false이고 화면/효과도 새로 관찰됐으므로 ‘이력만이 물리 원인’이라고 단정하지 않는다. 복구는 새 방 집계에서 제외한다.

R12/F(`gui-20261030-190619-fa68612ed9/room-01`)는 북쪽217frames/source15.3330418–45.4476532(30.115초)→복귀325frames/source45.7173944–91.3436242(45.626초)→Act1 저장 종료/north_room_closed/errors=[]다. 542고유frames/capture25.447/44.123, near57.478/77.037, processed116.725/145.380, 새F2 backend137.760/174.573(n102), ready→backend11.450/25.907ms다. loot9검사 최종OFF/복원오류0/click·teleport0, stream30저장/회전3생략/오류0과 정확한return bookend 파일 존재를 확인했다. peak WS304,390,144/private301,015,040/기본힙203,234,337B; 마지막idle119,734,272/102,350,848/20,192,938B; GUI Drop110,936,064/90,107,904/13,448,981B다. 녹화/GUI exit0, Flash 선택 화면이다.

네 번째 묶음은1정상/2실패, 누적4정상/8실패/연속1이다. F는 선택 이미지/버프 trace를 제한된 큐로 넘기고 bookend 제출을 실제로 확인했으나, 다른 지도/길이/정지 구간의 관찰 최고값 비교만으로 총 메모리·지연의 인과 개선율을 계산하지 않는다. 북쪽10연속·보스/열쇠/협곡·남쪽 수용은 여전히 미완료다. 다음 보정은 반환 경로 이력이 소진된 정확한 경우만 phase당1회 새 현재 지역 기준으로 재관측하고 **다음 새 HUD/바닥 전에는 이동하지 않는** 제한된 복구다. 원래 시한·최대3회 복구·실패 조준 제외·현재 종점 승인은 유지한다.

## Bounded Return Reobservation — Candidate G

복귀의 stuck 대안이 정확히 graph_entry_retreat_exhausted이고 phase에서 미사용일 때만 현재 mapping/source/250ms를 검사해 새 graph를 만든다. 이때 failed aim 제외는 남기고 조향/과거 chart만 해제하며 route_graph_reacquire_hold로 입력 없이 다음 새 frame을 기다린다. phase flag는 다음 interval에도 유지한다. 도착/성공·버프/전체 시한·원래3회 한도를 바꾸지 않았다. 실제 원인이 이력 하나라고 단정하지 않으며 재획득 뒤 물리적 탈출도 실패하면 원래 정지를 유지한다.

최종 내부 library114 PASS/0 fail/5 ignored, 이후 같은 frame에서 cast가 없음을 시간값이 아니라 capture/keydown 이벤트 순서로 강화하고 Stop 직후 새 cast 거부를 추가한2검사도 PASS, fmt/all-target Clippy PASS다. Native2검사는 mapping 없음/다른source/오래된frame을 거부하고 현재mapping에서만 old chart를 해제함을 확인했다. 현재 source는 고정했고 새 EXE/examples 및 관련 release integration을 실행 중이다.

G 검증 중 목적이 끝난D/E 후보와 그 원시 녹화/진단을 정리했다. 모든 해당 GUI/recorder/복구 writer exit0와 source/tests/examples/scripts/config의 실행 의존 참조0을 확인했고, 정확한 두 절대 경로의 workspace containment·root/하위 reparse 없음 검사 후 제거했다. D742files/415,703,073B와 E742files/466,212,240B, 합계1484files/881,915,313B다. 기본G 배포·현재F 후보·F의R11/R12 진단과 활성 task scratch는 보존한다. 앞 절 D/E 경로는 당시 실행 위치이며 현재 재생 의존 경로가 아니다.

독립 source 조사에서 SCREEN은 전체 monitor가 아니라 window outer rectangle만 GDI로 복사하고 이후 client/layout을 정규화하는 것을 확인했다. 현재DC/DIB는 frame마다 만들고 닫으며 recorder는 별도 ScreenCapture다. 이는 다음 CPU 최적화 조사 근거이며 이번 재관측 보정에서 FFI/캐시/캡처구독·WGC/GPU를 변경하지 않았다.

G 최종 release integration은 continuation2+field-runtime12=14 PASS/0 fail이며 내부114개의 release 재실행은 아니다. source240 SHA256은 `3dfc15aee4f0c4b9b693ce1b7abbfa6efb17a5c0af71d1a87147b1634476fe4a`다. 새 개발 후보G `dist/d2bot-windows-x64-summoner-dev-g/`는660files/24,021,802B(659 payload23,891,414B)이며 EXE16,968,192B/SHA `4925ebb10392c0441a517f20ee17d0c6e8ba48c3077b4cc5dc5a08430bb30b64`, manifest130,388B/SHA `3f8b224a23a56e1fa4407250a1b78a61d3bc7ab020bc25f2a8e3bc889dbb3152`다. 모든 payload 길이/SHA/경로 containment·미등록0, 실제19system imports·manifest 일치, 필수4region/기존 floor crop·금지runtime 없음이 독립 확인됐다. 외부CWD actual EXE diagnose/north3 dry와 typed assets3profiles5paths/input0/capture0 PASS다. 기본 배포의 과거G와 다른 개발 후보이다.

G production panel/backend resource-probe를 같은 SCREEN/exact-title config overlay로 실행했다. R13–R15는 아직 시작 전이며 sourceconfig는 변경하지 않는다. 계측 helper 실행과 actual packaged main EXE의 dry 검증 범위를 구분한다.

## R13 — Opposite Floor Candidate Before Return

G/Flash/지옥/반복3과 수동녹화 `diablo2_20261030_193728.avi`를 확인하고 시작했다. 첫 새 방 `gui-20261030-193736-5507ed2b74/room-01`은 outbound127고유frame에서 `field_paused/opposite_wing_aim_rejected`, errors=[]였다. 현재 graph chart10/place16이 비국소화되고 motion0matches/0inliers인 마지막 frame에서 현재 floor후보[606,867]/player[962,580]이 north[1,-.5]의 진짜 반대 범위로 들어갔고 관측된 corner 예외가 없었다. 이는 R10의 raw/등각 불일치가 아니라 기존 보호가 의도대로 거부한 경우다. 종점 승인/복귀 전환은 없어 새G 재관측 분기의 실제 검증으로 세지 않는다. F10 감독 종료/paused guard operator_stop·새 operator review 없음, 녹화/GUI exit0다. 다섯 번째 묶음1/3, 누적4정상/9실패/연속0이다.

R13 p50/p95(ms): capture27.105/43.924, near61.603/77.455, processed117.890/146.055, 새F2 backend137.519/187.523(n25), ready→backend12.142/27.391. F2down46(새25/반복21)/up25, F4down17/up9; loot5검사 모두labelsOFF/복원오류0/click·teleport0이다. stream13/13저장/회전2생략/오류0, 정확한before/after PNG 존재/상태partial이다. peak WS249,659,392/private235,991,040/기본힙157,463,268B, 마지막idle118,243,328/94,842,880/19,299,497B, GUI Drop110,317,568/86,855,680/13,490,390B다. 길이가 짧고 다른 지도이므로 앞 실행 대비 인과 개선율로 쓰지 않는다.

같은 방 `probe-return-20261030-194005`의 기존 north return으로 중앙WP→Act1→저장 종료/north_room_closed/errors=[]를 확인했다. TP/수동 마을 이동은 없으며 복구는 새 방 횟수에 더하지 않는다. 복구 GUI 마감 뒤 남은R14/R15를 이어간다.

R13 frame source는15.2021502–33.9429803(18.7408301초)이며 복구 GUI도 exit0로 닫았다. AVI는574frames/8fps/1922×1140/XVID handler/45,458,926B의RIFF metadata를 확인했다. 현재 사용 가능한 두 개발 Python에서 cv2 import가 실패해 이번 AVI 첫/마지막 decode는 미검증이다. 앞선D–F의 decode PASS를 이 새 파일에 적용하지 않는다.

## R14 — Terminal Lost After Approach

G `gui-20261030-194247-cf36dd7c28/room-01`은 outbound432고유frames/source16.0991865–82.6142163(66.515030초)에서 `field_paused/terminal_reacquisition_required`였다. completed segment1/4·phases=[]이며 F10 감독 종료 중 root interrupted/error=[control capture stopped]로 기록됐다. 새 operator review/사망은 없다. 다섯 번째 묶음2/3, 누적4정상/10실패/연속0이다. 녹화 `diablo2_20261030_194239.avi`와GUI는exit0다.

source59.1709551의 slow scan 뒤 frame347/source59.5880237에서 현재3부위 nonboss_chest_platform/[1534.67,602.83]을 읽었다. 목표거리573.2px 부근 정착 뒤 pointer-settling19행/source61.496–67.448과 실제 접근cast1회/source67.6988427를 거쳐 원래 목표를 잃었다. reacquire hold50행/source68.9781077–82.6142163, 지도숨김1회/source70.0140716 뒤에도 현재terminal이 없었고 기존 재획득 제한에서 정지했다. 저장 마지막 화면이 종점처럼 보이는 것과 현재3부위 판독/도착 승인은 구분한다. 기존 reference2초/시한·재획득 안전을 무조건 연장하지 않는다.

R14 p50/p95(ms): capture25.804/45.338, near58.642/77.943, processed116.065/241.050, 새F2 backend137.481/179.369(n45), ready→backend12.529/26.543. loot4모두OFF/복원오류0/click·teleport0, stream21/21저장/회전1생략/오류0·마지막interval정확한bookends존재다. peak WS291,434,496/private278,151,168/기본힙191,773,392B; idle121,909,248/98,807,808/19,930,417B; Drop113,139,712/90,443,776/13,459,528B다. RIFF1144frames/8fps/1922×1140/XVID handler/86,644,784B, decode미검증이다. 별도north return으로 방 정리 중이며R15는 아직 시작하지 않았다.

R14의 별도 `probe-return-20261030-194648`은925return frames/source5.4511149–128.1832438·3interval에서 중앙WP→Act1→선택화면/north_room_closed/errors=[]로 완료됐다. GUI exit0와 실제Flash 선택화면을 확인했다. R13/R14 복구 모두 새G의route_graph_reacquire_hold/reacquired marker는0이며 generic graph relocalization과 새 분기 검증을 혼동하지 않는다. 복구는 새 방 집계에서 제외한다. 이후 Flash/지옥/반복blank=1/SCREEN/exact title와 녹화 `diablo2_20261030_195123.avi`로R15를 시작했다.

## Batch 5 Complete — R15 And Current CPU Work

R15/G(`gui-20261030-195140-94d12f8a70/room-01`)는 outbound477frames/source18.7017564–83.9223413(65.220585초)의 현재 종점→return925frames/source84.2303100–210.2534672(126.023157초)→중앙WP→Act1→선택화면으로 north_room_closed/errors=[]다. acceptance_room=false이며 보스/열쇠/협곡 성공이 아니다. outbound에 현재 종점7관측(three_parts6/static_two_parts1), 접근 dispatch0/정착0이 있었고 return navigation만 본 최초 집계의 ‘종점0’은 폐기했다. 새 재관측 marker0/새 사람 review 없음, 녹화/GUI exit0다. 다섯 번째 묶음1정상/2실패, 누적5정상/10실패/연속1이다.

1402고유frames의 p50/p95(ms): capture24.939/43.124, near56.782/76.281, processed115.198/146.413, 새F2 backend139.203/185.111(n267/반복310제외), ready→backend12.718/29.797이다. loot15모두OFF/복원오류0/click·teleport0, stream54/54저장/회전7생략/오류0·정확한return bookends가 존재한다. peak WS468,381,696/private452,042,752/기본힙367,955,540B; 마지막idle127,488,000/111,263,744/22,237,487B; Drop114,823,168/104,697,856/13,664,392B다. 지도/실행길이·정지 방어가 달라 인과 개선율로 쓰지 않는다.

R15 AVI2165frames/214,557,128B와R13/R14의 실제 첫·마지막decode가 모두PASS다(8fps/1922×1140/XVID handler). 앞의 미검증 기록은 기본/Anaconda Python만 확인한 중간 상태였다. 프로젝트 `.venv/Scripts/python.exe`의OpenCV4.13.0으로 검증해 해소했으며 외부runtime배포에는 포함하지 않는다. 전체 중간frame/실제wall-time drop수 검증은 아니다.

다섯 번째 검토의 H 보정은 R13 초기 굴곡의 짧은 현재 측면을 기존64px 탐침과 동일하게 검토한다. 정확한 실패frame을 재생한 최초 검사가 같은[606,867]/opposite_exit_without_observed_corner로 실패했고, 보정 후 현재East 바닥을 선택했다. 새 fixture는 field 밖을0으로 만든 개발 전용1장으로 정식reference/manifest/실제unit consumer를 가졌고 runtime 자산은 추가하지 않았다. stale/실패좌표 제외/바닥 제거까지 포함한 새 검사와 전체library115 PASS/5ignored·기존steering2/continuation2 개발PASS다. 실제H 이동은 미검증이며 R14 재획득은 아직 미해결이다.

추가 CPU 조사에서 근접 hazard의 같은 ROI가 template마다 동일한 BGR 누적합을 다시 만들고 uint8 유한성 loop를 반복함을 확인했다. 현재 bounded scan 안에서 불변 ROI 통계를 한 번만 공유하는 보정을 검토한다. 값/채널 합산 순서·threshold·후보 순서·화면수명/시한을 유지하고 고정화면전후비교/동일출력검사 뒤 채택하며, 현재 게임입력과 모든writer는 닫혀 있다.

F 원시R11/R12/복구의 분석 목적은 종료됐다. R11 실패 graph와 별도 복구의 다른 조건·R12 정상 결과는 위 요약/EVAL에 남겼고 G의 phase당1회 재관측은 source의 가짜 입력/실제현재 mapping 회귀가 소비한다. F PNG/AVI/report 자체를 소비하는 source/test/config나 새 승격 fixture는 없으며 다음 실게임은 새G/H가 소유한다. F 삭제 첫 요청은 자동 검토가 ‘진단이 아직 필요할 수 있다’는 이유로 거부해 실행되지 않았다. 이후 세 resource log의 native_gui_dropped와 이미 확인한 exit0, 전체659 payload manifest 일치·추가 파일이 정확한두AVI/두town-loop/R11/R12/복구 디렉터리뿐임을 확인하는 추가 소유권 검사를 수행한다. 이는 새 분기의 실게임 효과 미확인을 성공으로 바꾸지 않으며 기본 배포/G/H source·필수자산은 보존한다.

추가 검사는 payload mismatch0·134개 extras 모두 위 task 소유로 확인됐다. 그 근거/종료·consumer0·workspace containment/reparse0을 포함한 재요청은 허용돼 F794files/795,086,636B를 제거했다. 첫 거부를 우회하지 않고 진단 필요성/소유 불확실성을 해소한 뒤 같은 정확한 경로만 정리했다. 현재 G raw와 task scratch는 H 수정/전후 계측의 활성 owner이며 기존 Cargo 공용cache는 보존한다.

H의 borrowed BGR matcher는 ROI를 복사하지 않고 scan의 view를 빌려 누적합 하나를 공유한다. 홀수 stride/여러크기/세 matching mode의 uncached score와 모든bit가 같고 float NaN 거부가 유지됐다. 첫Clippy는 새 회귀의 Copy context.clone 한 곳에서 실패해 context 그대로 전달하도록 고쳤다. 최종fmt/check/all-target Clippy PASS, 내부116 PASS/5ignored와 continuation2/encounter3/hazard1/steering2/vision18=26개 개발integration PASS다. HSV/Lab 전색·OpenCV 점수/FFT parity도 포함한다. 최종 source240 SHA는 `ae6a674af1a0ed51ed02f4a734f006ecbd3308e274dc168e661cc52f5ec9f10b`다. release 링크/관련release 검증·정적화면 전후·실게임H는 아직 별도다.

통계 공유 전 고정화면 기준은 기존 static-CRT benchmark EXE SHA `e09708bab9ef4316ec62f6f5014de78d170dc36c125bead4c2046df3ea56a4b6`로 R13원본BGR SHA `0b88de4943f713c92c9001dfcb063607877b635a75bb76a9e7d002b4abe96bf5`를 읽은 입력0/캡처0측정이다.30샘플/3warmup/nearest-rank p50/p95(ms): near_hazards17.8504/23.5594, near_hazards_route_aim18.6484/21.3079, hover20.0129/22.5616, path18.0352/18.8833, ORB pair19.04/19.5326이다. 이 기존benchmark binary는 실제 실행hash로 고정하며 최신H소스 전체와 동일한 빌드로 재라벨하지 않는다. near 비교는 이번 변경이 직접 바꾼 component 범위이고 전체 실시간 반응으로 확대하지 않는다.

H main/자원probe/typed assets/benchmark의static-CRT release링크는4m20s에 완료됐다. 새H660files/24,023,845B(659payload23,893,457B), EXE16,968,704B/SHA `5ebc9b9b03a5fd49e445d52f450921fd6985424c31f05352961f9adc552c9751`, manifest130,388B/SHA `14233ffdfd626d4039aad1f1886c0cdad2877aed47bb5b63d1a469dbd01d201e`다. 독립 검사에서 모든payload 길이/hash·containment·미등록0·reparse0/19system imports·4region을 확인했고 새 개발계단fixture는배포에제외됐다. 기존gold-filigree-lower-loop-support runtime crop은포함됐다. 외부CWD actual EXE diagnose/north3 dry, typedassets3profiles5paths/input0/capture0 PASS다. Release library/통합과 고정화면전후/실제H는진행중이다.

R14 재획득 진단의 개발OpenCV native크기 회색조 최고점은 최초/숨김직후/마지막에서 engraved-stone-plate .920679/.393895/.416605, dark-arched-chest-right .954154/.919360/.854936, square-chest .972897/.727492/.930198이었다. 현행.92와 원래독립3부위로 현재종점을 다시 확인하지 못한 근거이며 exact native Rust 점수로 재라벨하지 않는다. 초기검출 뒤 외형/가림 변화의 원인은 아직 확정하지 않았다. 대기/시한을 늘리거나 threshold를 낮추지 않고 미해결로 유지한다. 별도 임시Rust 진단도구는외부crate 링크에실패해 판독근거로쓰지 않았으며 task scratch와함께정리한다.

H 최종 release는 library116 PASS/5ignored+continuation2/encounter3/field-runtime12/hazard1/steering2/vision18=38integration PASS, 총154 PASS/0fail/5ignored다. debug142 PASS와 별도 실행 범위이며 새로운 ignored 검사를 성공으로 포함하지 않는다.

같은BGR/입력0/캡처0의 H 고정화면 p50/p95(ms)는 near17.2038/21.6604, near+route aim18.2736/21.3417, hover20.637/24.2473, path18.2576/19.2264, ORB pair19.2719/20.4677이다. 이후 같은 이전binary 반복은 near18.8459/23.4623, near+aim18.9011/21.1239, hover20.2847/22.5599, path18.2189/18.7777, ORB19.1876/20.2584였다.30샘플/3warmup/nearest-rank·source/floorhash 동일이다. 일부 근접 중앙값의 소폭 감소이며 p95/다른stage의 변동도 있어 큰 인과 개선율이나 실게임50ms를 주장하지 않는다. H 자원probe를 시작해 다음 R16–R18의 현재화면/입력·녹화를 확인한다.

## Batch 6 — H Live In Progress

Flash/지옥/SCREEN/exact title/반복3과 녹화 `diablo2_20261030_203211.avi`를 확인하고 `gui-20261030-203226-dedc2b10fe`로 시작했다. R16/room-01은 outbound391frames/source15.7908478–68.1156394(52.324792초)/recent_terminal_layout_and_player_settled→return570frames/source68.4354644–147.1221074(78.686643초)/hub_arrived→Act1→선택화면/north_room_closed/errors=[]다. acceptance_room=false·새 사람review 없음, 누적6정상/10실패/연속2·여섯 번째 묶음1/3이다. 자동R17이 입장했고 녹화/GUI는 계속 활성이다.

R16의961고유frames p50/p95(ms): capture24.988/44.290, near56.641/76.678, processed115.676/147.445, 새F2 backend138.247/173.588(n193/반복199제외), ready→backend12.587/26.709이다. loot10모두OFF/복원오류0/click·teleport0; stream33/33저장/회전5생략/오류0·정확한return bookends가 존재한다. observed_corner_probe와 새route_graph재관측 marker는0이므로 이 방의 정상 마감을 새 분기 효과로 해석하지 않는다. 자원 최고값/AVI는 같은3회 writer 마감 뒤 집계한다.

R17/room-02는1983고유frames/source175.2094028–433.8876734(258.678271초)/4구간을 소진해 north_segment_observed/interval_complete/errors=[]로 끝났다. 종점/복귀 승인이 없어 실패로 집계하며 root batch는 requested3/completed1/needs_navigation_or_encounter_review다. 누적6정상/11실패/연속0·여섯 번째 묶음2/3이며R18은 미시작이다. F10 감독 중단 뒤 녹화/GUI exit0, 새 사람review/사망 없음이다. 별도north return을 시작해 방 정리 중이다.

R17 p50/p95(ms): capture26.622/45.499, near59.020/79.959, processed111.355/143.922, 새F2 backend137.107/199.183(n286/반복504제외), ready→backend11.395/26.236이다. graph는96places/12references/114relocalizations/46방문출구/6실패출구까지 사용했고 마지막 localized=true/place=null이었다. 인접619쌍의 조준·사유가 같고 최장15행이지만 이것만으로 전체 실제 경로의 반복 횟수로 환산하지 않는다. source282.7190658에 observed_corner_probe1회/current floor/unlocalized/East detour가 있고 held F2 count135→136이다. 같은reason만으로 새 초기 분기와 원래 굴곡 중 분기를 식별하지 않는다.

loot15중14완료/1rune click_unconfirmed이며 모두labelsOFF/복원오류0, 후자에 rune접근teleport1/클릭1·pickup_confirmed=false가 있다. 아이템 접근이고 TP 사용/줍기 성공으로 기록하지 않는다. stream65/65저장/회전5생략/오류0·정확한4번째interval bookends가 존재한다. 두방합계 resource process peak WS579,096,576/private563,793,920/기본힙404,407,202B, 마지막idle140,906,496/121,933,824/24,471,025B, GUI Drop123,334,656/104,988,672/13,590,222B다. 한 방의 최고값으로 잘못 귀속하지 않는다. Drop committed/reserved151,592,960/341,848,064B는 실제live할당13.59MB와 구분한다. AVI4018frames/8fps/1922×1140/XVID handler/420,500,310B·첫마지막decode PASS다.

## R17 Recovery — New Partial Buff Review Gate

`probe-return-20261030-204521`은 복귀 중 buff_renewal_due 뒤 자동 버프가 W→A→A→S에서 중단됐다. 요청source139.646492/140.1978053/140.7516239/141.2951261이며 최종W 미전송, actual status failed/errors=[automatic buff dispatch: buff_resources_unstable, paused guard: buff_resource_stability_timeout]다. paused guard observations0/시작141.9513791/종료149.9739121, 필드 retained이며 Act1 복귀나 새 방 성공이 아니다. H 보고서의 automatic_buff/buff는 첫 성공한 균형7입력만 남고 실패 순서가 누락됐다. input_events로 새부분전송을 확인했으며 사망·초상 미확인은 없다. F10 감독 중단/GUI exit0·Drop기본힙13,479,775B다.

현재 화면의 생명/마나는 회복됐지만 메인 무기 시작 전제를 다시 보장할 수 없다. [부분W 규칙](../../features/summoner-run.md)에 따라 추정W/TP/수동보행/새 자동입력을 하지 않고 사람에게 이번 새 메인 무기·용병/Act1 저장 종료 확인을 요청했다. R08 용병 확인은 이번 부분 전환의 확인을 대신하지 않는다. 답변 전 R18은 미시작이며 여섯 번째 묶음은2/3에서 유지한다.

안전 보정I는 AutomaticBuffDriver가 실패 결과와 무기 상태를 작은 metadata로 보존하고, 부분/전송 오류 뒤에는 동일driver의 재시전·native 정지방어 새입력을 차단한다. Snapshot/cleanup이 automatic_buff_failure와 operator_review_required=buff_weapon_state_unverified를 남긴다. 자동W/효과/타이머 승인은 추가하지 않으며 정상·균형이 확인된 실패의 기존 동작은 유지한다. 현재 신규/기존회귀를 검증 중이고 H source/EXE 검사 결과를 I에 소급하지 않는다.

R17 graph source 조사:96place는 eviction 없이 유지돼 새 장소가None이 되어 방문/실패 출구 이력을 기록할 수 없었다. 참조 없는 과거 chart의 도달 가능성과 chart번호 생성도 다음3회 검토 대상으로 삼는다. Motion은 판독 실패 때 이전 feature 기준을 이미 보존하므로 ‘실패마다 기준을 교체한다’는 추정은 폐기했다. 이 조사만으로 game loop의 단일 원인을 확정하거나 graph 코드를 변경하지 않았다.

후속 exact report 조사에서 index911/source294.5857076,958/301.1591314,1018/314.5958483,1211/339.1676273에96place/place=null/new_local_chart가 모두chart43을 재사용했다. 특히1018의 직전chart36→새43은 이전43과 다른 좌표계 생성이 같은번호로 합쳐짐을 보여준다. 배열의최대chart+1을 쓰며 cap뒤새place가 추가되지 않는 원인이다. 별도 단조checked counter의 안전 보정을I에 포함했다. 최초4place cap 재생은기존chart1/기대2로FAIL, 수정뒤 새ID/과거chart재대응/usize상한오류·기존graph oracle28cases46graphs221operations/참조해제3검사PASS다.96place/12reference 상한은 유지했고 과거chart회수/슬롯재활용·경로와근접판독병렬화는 아직 구현하지 않았다.

I의 부분버프보정은 신규 ‘불균형/전송오류 뒤 환경호출0·재시전0·기존deadline보존/균형실패재시도’ 회귀와 최초fmt/check/Clippy·library116/5ignored+준비18/field-runtime12=146개 개발검사PASS다. 이후chart보정을 더한 최종source240 SHA는 `191a3c29eaced442dad300cac1fbeeb6a01f4f01829bfaa3a5203921a41d77b0`이며 최종quality/release/배포를 다시 검증한다. H의154release/실게임결과는I검사가 아니다.

## I Ready — Human Equipment Handoff Pending

최종I의fmt/check/all-target Clippy PASS, library117+준비18/field-runtime12/steering2/continuation2=151개 개발검사 PASS/0fail/5ignored다. 같은 최종source의static-CRT release도 library117+integration34=151 PASS/0fail/5ignored이며 링크3m54s를 완료했다. 첫 verification packet의 존재하지 않는 test target은 실행 전에 목록을 바로잡았고 그때 중단한 lib compile에는 검사 결과가 없었다. 위 숫자는 완료한 재실행만 센다.

`dist/d2bot-windows-x64-summoner-dev-i/`는660files/24,027,941B(659payload23,897,553B), EXE16,972,800B/SHA `3b4a9997fb9bbdeb1a542793e2d8a67f0240dc96d7275740a4b66f17274f4c75`, manifest130,388B/SHA `b32ee580c2eae506c64650dab8fec7ceb47dcceb832f7eea2afc677593ea7340`다. 모든payload 길이/hash·containment/미등록0/reparse0, actual AMD64 PE32+19system/API DLL imports와manifest 일치, 필수4region/기존supportcrop 포함·개발reference제외·Python/OpenCV/FFmpeg/Xvid/libav runtime 없음이 독립 확인됐다. 라이선스353files/1,887,725B·161dependencies는외부runtimebinary와 구분한다. 외부CWD actual EXE diagnose/north1 dry 및 typedassets3profiles5paths/input0/capture0 PASS다. I는 아직 실게임입력/새 부분버프 실패/새 chart 번호를 실제게임에서검증하지 않았다.

자동 GUI·recorder·복구 프로세스는 모두 exit0로 종료했고, Computer Use 창 목록에도 helper가 없다. 새 메인 무기·용병/Act1 저장 종료 확인을 기다리므로 R18은 미시작이다. 여섯 번째 묶음은2/3, 누적6정상/11실패/연속0이다. 확인 뒤 I backend/Flash/지옥/SCREEN/exact title/반복1·녹화로 R18을 이어가3회 검토를 완료한다. 종점 재획득·보스/열쇠/협곡·북쪽10연속·남쪽 수용·전체50ms는 미완료다. 이후96개 장소 슬롯 재활용, 도달 불가 좌표계 정리, 경로·근접 판독의 직렬 구조를 검토하며 아직 구현하지 않았다.

진단 중 필요했던 R13 한 장은 정식 asset/unit owner로 승격했다. 미해결 R14 외형3장, R17 마지막 경로1장, 부분 갱신의 W/S 이전2장과 축약 metadata/provenance만 task `target/tmp/summoner-development-20261030-r09/retained-diagnosis/`에 보존했다. 총11files/16,658,176B이며 원본과 해시가 일치한다. blocked-D의 실패 source 이미지는 원래 저장되지 않아 이전 S 이미지를 그 화면으로 라벨하지 않는다. 외형·경로·자원 원인을 해결하거나 필요한 회귀를 정식 owner에 승격하면 이 폴더를 제거한다. 같은 task의 `config/` overlay는 다음 감독 시험 재개용11files/38,768B이며 해당 시험 목적이 끝나면 제거한다. 배포·source/test는 두 임시 폴더에 의존하지 않는다.

전체 payload hash, 추가 파일의 정확한 소유, 원시 writer 종료, consumer0, workspace containment/reparse0와 필요한 진단의 별도 보존을 확인한 뒤 구개발 G867files/959,304,356B와 H recordings170files/919,002,827B를 제거했다. 기본 배포와 깨끗한 H660file fallback·현재 I660file 배포는 보존한다. 이번 D/E/F/G/H raw 정리는3315files/3,555,309,132B이며 이전 turn의 별도 정리 수와 중복 합산하지 않는다.

최종 읽기 전용 검사에서 관련 문서9개의 상대 링크156개 누락0, source240 SHA 재일치, 깨끗한 H/I 각660files의 모든 payload 길이·해시·정확한 inventory·containment·reparse0을 확인했다. 완료한 빌드·측정·진단 로그와 임시 benchmark/진단 소스 등 나머지 task scratch67files/8,881,430B도 정확한 경로·소유·종료를 확인한 뒤 제거했다. task root에는 위 `config/`와 `retained-diagnosis/`만 남는다. 일반 Cargo target/cache는 이 정리 대상이 아니다. 삭제한 raw/log의 수치는 이 run과 평가의 관찰 요약으로 남으며, 삭제 파일의 재실행을 약속하지 않는다.

## I Resume — R18

2026-10-30 사용자가 이번 메인 무기·용병/Act1 저장 종료 확인 요청에 “정상이야 계속 진행해”라고 답했다. 부분 전환에 대한 사람 확인 경계는 해소됐으며 같은 상태를 다시 묻지 않는다. I production backend의 resource-probe GUI/Flash/지옥/SCREEN/exact title/반복1·녹화로 R18을 준비한다. 현재 화면·포커스/HUD 보호는 그대로 유지한다. R18 전 누적6정상/11실패/연속0·여섯 번째 묶음2/3이며 새 결과는 실행 후 기록한다. 사용자가 목격한 룬/상급 보석 줍기는 공용 설정과 실제 소비 경로를 읽어 설명하며, 이 질문을 품목 변경 지시로 해석하지 않는다.

제어판 실행의 첫 sandbox 외부 요청은 자동 승인 검토가 현재 전제조건/실행 승인 근거 부족으로 거부했다. 사용자 직접 확인, 읽기 전용 Flash 선택 화면, mode0의 Idle 생성자와 버튼을 통한 명시 시작 경로를 추가 확인한 뒤 같은 명령의 재검토가 통과했다. probe EXE SHA는 `db954c0461c71228521bc3081dba44637e1980062bbf9d4972681d88a6a0a21f`다. 실제 제어판에서 Flash/지옥/반복blank=1/SCREEN/exact title을 확인하고 녹화를 시작했다. 이어 Summoner Run 버튼은 자동 검토가 실제 게임 입력의 승인/전제조건을 입증하지 못했다고 다시 거부했다. 우회/다른 입력 경로로 실행하지 않았으며 R18은 미시작, 새 방 분모와 누적 수는 변하지 않는다. 사람의 상태 확인은 이미 해소됐고, 이번 정확한 R18 실게임·기존 코드 복귀 범위를 명시한 재승인 질문만 대기한다.

게임 선택 화면만 담긴 녹화 `diablo2_20261030_223144.avi`를 Stop Recording으로 마감하고 Idle/Start Recording 활성·Stop 비활성 상태를 확인했다. 제어판을 닫아 resource-probe exit0를 확인했다. GUI Drop WS89,083,904/private82,067,456/기본힙13,254,300B는 미시작 GUI 정리 값일 뿐 게임 성능 측정이 아니다. 이 녹화와 resource log는 이번 차단 진단 목적이 끝났으므로 정확한 소유/경로·writer 종료 확인 후 제거한다. 패키지 payload는 변경하지 않았다.

줍기 조사: source/I 배포/선택 config overlay의 loot JSON은 SHA `00d1554bfb1d5976705344cbe9672437bc8e0069d03bfa519f4b9949ac01da7d`로 같다. 공용 fixed_items는 key of hate/rune/amethyst이며 Summoner 전용 keep_labels/templates 추가는 없다. 실제 native scanner는 이 설정을 소비한다. 일반 이동은 노바 전송 증가 후3frame/1초 quiet, encounter는 Nova 전송 또는 처치 확인 뒤 검사한다. 기본3초/12frame/2click 상한·현재 후보/착지 후 라벨·근접 재확인·라벨 OFF 복원을 유지한다. 룬 종류/보석 등급 OCR이나 inventory_template 획득 확인은 없으므로 상급만 선별하는 기능이나 특정 목격 품목의 실제 획득을 주장하지 않는다. R17 rune의 click_unconfirmed도 그대로다. 설정·품목·코드는 바꾸지 않고 guide에 현재 동작/한계만 보완했다.

위 미시작 녹화/자원 로그2files/24,869,813B는 종료·정확한 경로·소유/containment·reparse가 아닌 일반 파일임을 확인한 뒤 제거했다. I는 원래660files로 돌아왔고 task scratch에는 기존 재개용 config와 retained-diagnosis만 남는다. R18 입력 차단의 재승인 질문은 아직 대기한다.

문서 보완 뒤10개 문서의 상대 링크157개 누락0, 정리 뒤 I660files의 모든 payload 길이/해시·정확한 inventory 일치를 확인했다. 코드·런타임 JSON·자산은 바꾸지 않았으므로 새 빌드/회귀 검사를 수행하지 않았다.

## R18 Approval And Restricted Loot Configuration

사용자가 정확한 R18 실게임 범위에 “실게임 시험 승인 룬은 이스트 룬 이상만 수집”으로 답하고 이어 보석 수집 안 함을 지정했다. 자동 검토 때문에 요청한 실행 재승인은 해소됐다. 사용자는 이름별 캡처가 없는 다른 룬의 실현 가능성도 물었으며, 현재 실제 이름 crop은 이스트/오움뿐임을 확인·설명했다. 전체 이스트 이상 판별을 가장하지 않고 공통 룬/보석 후보를 제거하며 현재 이름 자산 두 종과 증오의 열쇠만 남긴다. 다른 고급 룬은 미지원·미확인은 생략이다. 품목 변경은 사용자 지시에 따른 의도적 동작 변경이다.

룬 threshold0.95의 새 설정에 대해 실제 scanner를 쓰는 회귀를 추가했다. 모든 후보보다 큰 동일 바탕에서 이스트/오움 각각의 정확한 이름·위치·점수를 확인했고, 공통 룬 접미/자수정 음성을 확인했다. 개발 action-recognition6/0fail/0ignored PASS다. 첫 fmt check의 새 assert 줄바꿈을 formatter로 정리한 후 실행했으며 이는 모든 미보유 하위 룬에 대한 음성 검사나 실제 획득 검증이 아니다. runtime src와 I 바이너리는 바꾸지 않고 source 설정/선택 overlay/새 패키지를 일치시킨 뒤 R18을 진행한다.

최종 fmt/check/all-target Clippy PASS, 같은 action-recognition static-CRT release6/0fail/0ignored PASS다. `tests/rust-action-recognition.rs` SHA는 `293c911766bd59c9099e94d9acf3a6f97a00d29b36d0338f30bbced0196f3708`이며 runtime source240 SHA는 기존 I의 `191a3c29eaced442dad300cac1fbeeb6a01f4f01829bfaa3a5203921a41d77b0`와 같다. main EXE를 다시 링크하지 않고 I와 동일한 EXE SHA `3b4a9997fb9bbdeb1a542793e2d8a67f0240dc96d7275740a4b66f17274f4c75`를 재사용했다.

새 설정 패키지 `dist/d2bot-windows-x64-summoner-dev-i-loot/`는660files/24,029,222B(659payload23,898,834B), manifest SHA `5f7f98f2416a234569a1e41f81926beeafb822c0684f885be6341d4f9cacad50`다. source/overlay/배포 loot JSON은 SHA `75ccd991bac12d2918c30e5472774a864a9f746f4d456c217f00fca22f123cf6`로 일치한다. 독립 검사는 모든 파일 길이/hash·정확한 inventory·containment/reparse0·필수4region·외부 Python/OpenCV/FFmpeg/Xvid runtime 없음과 두 이름 crop의 포함을 확인했다. 패키지 typed loader도 입력0/캡처0으로 통과했다. 전체 등급 판별/이름별 실게임/획득 수용은 아니다.

R18은 위 I-loot 패키지 cwd의 production backend resource-probe/Flash/지옥/반복blank=1/SCREEN/exact title과 수동 녹화 `diablo2_20261030_230700.avi`로 시작했다. 최초 Record 입력의 geometry unavailable은 새 screenshot-backed 상태 뒤 한 번 재시도로 해소됐고, Summoner 시작 버튼은 이번 명시 승인 뒤 통과했다. 실제 새 입장/아케인 탐색을 관찰했으며 room owner는 `gui-20261030-230709-47ffbfac71/room-01`이다. 게임은 감독 중이며 아직 종료/성공·실패 집계 전이다.

## R18 Result And Batch 6 Review

R18은 현재 단상까지 도달했으나 전용 encounter4관찰에서 생존/시체가 모두 null이고 접근2회를 소진해 kill_unconfirmed였다. 그 전에 navigation은 F4 12전송/F2 62전송(hold_nova28행)을 기록했고, 전용 encounter의 Nova는0이었다. 이후 대기 방어는 처치 판독을 재개하지 않고 호버를 반복했다. 최종 status=failed/errors=[field pointer mapping mismatch: requested [3114,1025], observed [3027,991]; paused guard: field pointer mapping mismatch: requested [2714,985], observed [2112,1226]], field retained/Act1 마감 미완료다. 포인터 불일치 원인을 사용자 입력으로 단정하지 않는다. 사망·용병 미확인·부분 버프 실패로 바꾸지 않는다. 여섯 번째 묶음 R16정상/R17실패/R18실패=1정상/2실패, 누적6정상/12실패/연속0이다.

최종 report의185고유 nav frames p50/p95(ms; 선형 보간)는 capture26.243/46.104, near59.436/77.736, processed113.421/138.197, 새F2 backend135.816/152.986(n22/반복40제외), ready→backend11.993/24.320이다. loot4검사는 클릭/접근0이며 최종OFF다. 현재 맵/단계가 다른 앞 방과 인과 개선율로 쓰지 않고 전체50ms 미달을 유지한다. resource283관찰의 최대 WS487,882,752/private469,057,536/기본힙381,277,972B; 마지막idle121,765,888/99,418,112/19,640,963B; GUI Drop100,737,024/93,720,576/13,565,655B다. Drop committed46,526,464/reserved76,546,048B와 live할당을 구분한다. 실제 프로세스 exit0를 확인했다.

Computer Use는 사용자 물리 Esc로 한때 중단됐다. 코드/저장 report 조사 중 새 창 조회도 자동 검토가 그 중단을 이유로 거부했고, 사용자의 “Computer Use 재개, 상태 확인·종료 허용” 뒤 기존 helper를 읽어 Stop Action 비활성/Incomplete를 확인했다. Stop Recording 마감 후 Idle/Record 활성·Stop 비활성, Close 뒤 resource-probe exit0다. 추가 게임 입력이나 실패 복구는 하지 않았고 현재 조작 상태는 사용자에게 있다. AVI는3,127frames/8fps header/1922×1140/174,288,840B이며 프로젝트 개발 OpenCV로 전3,127frame decode를 통과했다. 녹화에 메뉴·대기·수동 개입 가능 시간이 포함되므로 실제 game cadence/drop수·자동 처치 원인/시간을 추정하지 않는다. 배포에는 decoder를 넣지 않는다.

## J — Correct Normal Finish And Log Following

사용자는 서모너 사후의 반복 훑기 대신 지도 숨김/재확인 또는 단상 정리·열쇠 검사로 정상 마감을 이어가라고 요청했고, GUI 로그의 바닥 따라가기/위로 읽기 유지/바닥 복귀 시 재개를 추가했다. 주관자가 미확인·키 없음에서 중앙으로 되돌리려던 방향은 사용자가 명시 정정했다. 정상 마감은 서모너 처치→일지→붉은 포탈→협곡 WP→Act1이다. 잘못 추가 중이던 중앙 분기·그 전용 survey/공용 probe metadata·테스트는 실행/배포 전에 제거했다. 기존 north return은 실패 복구용으로만 유지한다.

J 구현은 현재 단상에서 외형을 못 읽을 때 입력 해제→지도 ON 확인→Tab 한 번→새 화면 재관찰→자신이 숨긴 지도 ON 복원/확인을 연결한다. 기존250ms/Stop·포커스/HUD·전체12초/버프 시한은 유지한다. 단상 중심100px와 실제 몸통 위치를 분리해, 현재300×180 안에 있는 몸통 후보를 먼저 호버한다. R18 현재 goal[1082,432]/player[962,580]에 실제몸통[1082,608]이 후보에 포함된다. 새 근접 공격 허가는 여전히 현재 적 패널/커서를 필요로 한다.

R18 마지막 PNG의 시체는 기존 right lower-band32×14 asset과 .858879로 맞지만 .92에서 거부됐다. 독립 blue/gold agreement .816327/.827957을 확인하고 이 part만 native .85+각색 .80을 함께 요구하도록 보정했다.29개 runtime part/PNG·crop/offset은 그대로이며 새 runtime template은 없다. 개발용 원본 그대로의 시체/생존 두 참고 PNG를 정식 owner에 보존했고 source hash는 해당 README가 소유한다. 실제 Rust는 현재 단상·몸체 제거·무채색·생존 음성을 통과했으며 이후 추가한 duplicate와119개 이상 native 비검출 corpus를 최종 검증한다. 이 재생으로 R18 자동 처치·시간이나 새 live corpse pair를 소급 승인하지 않는다.

전용 encounter가 kill_unconfirmed인 경우에만 대기 감시가 이미 캡처한 화면에서 현재 단상/시체 후보를 최대4Hz로 재관찰한다. 신선한 후보는 입력/probe 해제·대기 worker 종료 후 정식 encounter에 한 번 재진입하는 신호다. 정식 두 새 화면의 죽음·정상 전리품 검사를 다시 통과해야 협곡으로 간다. 재진입은 갱신된 버프 시한과 새 pool을 사용하고 이전 보고서/새 증거 파일명을 구분한다. 이 mode에서는 idle hover가 현재 actor 후보만 사용하며 실제 양성 위험/자원 방어는 유지한다. 성공 정리한 encounter vision/pool은 즉시 버리고 늦은 정리 오류 때는 기존 소유자를 보존한다.

GUI는 log revision마다 무한 scroll offset을 강제하던 상태를 제거하고 egui0.37의 stick_to_bottom을 쓴다. 실제 egui wheel 입력 회귀가 바닥의 새 로그 따라가기, 위로 읽는 동안 append 위치 유지, 아래로 내린 뒤 append 따라가기 재개를 통과했다. 기존300줄 제한과 literal text/Stop 가시성은 유지한다. 개발library121 PASS/5ignored와 외형integration2 PASS를 확인했고, 마지막 native corpus/duplicate 추가 뒤 최종 quality·release·EXE/배포는 별도 packet으로 검증 중이다. J 실게임/협곡 마감은 아직 미검증이다.

## J Ready — Normal Portal Test Handoff

최종 fmt/check/all-target Clippy PASS, 추가 debug native corpus1과 duplicate 포함 appearance focused1 PASS다. 변경한 part 단독의 넓은 허용ROI에서120개 native alive/scene 비검출과5개 비정규크기 거부를 확인했다. 최종 static-CRT release는 library122 PASS/5ignored +action-recognition6/canyon-finish1/encounter-contracts3/field-preparation18/field-runtime12/summoner-appearance2=42 integration PASS로 합계164 PASS/0fail/5ignored다. 전체 repo의 모든 integration을 다시 실행한 것은 아니다. source240 SHA는 `d2fceb352869735d9d0ac88978b8a77f9f1ab4aa98d2420100e35268b8671a58`, appearance test SHA `8edb3120705de5d02828f90b68ad8fcaf80d4ecf6b54e7e9d0dbe0f63d7c328e`, action test SHA `293c911766bd59c9099e94d9acf3a6f97a00d29b36d0338f30bbced0196f3708`, shared field fake SHA `d2a560aa9c4ebd0edcb51d83c2b16d5225092ac6f1b0af1442d7ae35f6cb9e81`다.

main/probe/typed-loader static-CRT 링크는1m11s/exit0다. `dist/d2bot-windows-x64-summoner-dev-j/`는660files/24,060,892B(659payload23,930,504B), EXE17,003,008B/SHA `02a9f382af8a995d13d514c6fc0bdad8467c84fe28c9dab1f348628201cc18f2`, manifest130,388B/SHA `230bf867f8456ff49c3c1d7d08262d958eb98210e69ccf48dca951b152bdc6f3`다. 실제 AMD64 PE32+의22 import descriptors는19개 고유 system/API DLL 이름으로 정규화돼 manifest와 일치한다. 모든파일 길이/hash·containment/inventory/reparse0, 필수4region·기존supportcrop 포함, 개발R18 reference 두 장 제외, 외부 Python/OpenCV/FFmpeg/Xvid runtime 없음을 독립 확인했다.29개 runtime part의 PNG hash는 I-loot와 전부 같고 한 part의 문턱/색상/검토 metadata만 바뀌었다.

패키지 stage typed loader는3profiles/5configured paths/input0/capture0 PASS이며, 외부CWD `target/tmp`에서 actual J EXE `--diagnose`와 north1 dry도 exit0다. source/선택overlay/J loot JSON SHA는 여전히 `75ccd991bac12d2918c30e5472774a864a9f746f4d456c217f00fca22f123cf6`다. 현재 source와 실제 main/probe가 일치하며 probe SHA는 `a1afed545763ddba988b2c1ffec769344de5927bd9e1358ba38785bafa569944`다. J 실제 처치/두 fresh corpse/일지·협곡 마감은 아직 미검증이다.

Esc 이후 사용자가 허용한 Computer Use 범위는 상태 확인·종료였으므로, 다음 실제 입력 전 현재 조작 인계를 요청했다. 선택지는 Act1 저장 종료에서 새 북쪽 시험, R18 단상에서 같은 방 정상 포탈 마감 시험, 직접 조작 중 코드 검사까지만이다. 답변 전 새 게임/필드 입력은 보내지 않는다. 봇·녹화/GUI와 빌드/검사 writer는 종료됐고 일반 Cargo cache는 보존한다.

미해결 R18 pointer 오류는 원시 report SHA `1ffc99d59255f5bd30d240b419bfabff54b16f842df38cdeccb225bdbf43a75f`와 geometry/400개 입력/encounter·guard metadata를 task `retained-diagnosis/r18-pointer-context.json`(226,219B)에 한정해 보존했다. 원인을 진단하거나 필요한 회귀를 정식 owner로 승격하면 제거하며 프로그램/정식 검사는 이 임시 경로에 의존하지 않는다. 조명 재생에 필요한 두 PNG는 정식 region owner에 승격했다. R18 원시 AVI/report/나머지 PNG와 종료된 task 로그는 결과 요약·원본/승격본 해시·소유/consumer·writer 종료·정확한 경로 검사를 확인한 뒤 정리한다.

정리 첫 요청은 자동 검토가 과거 Incomplete 창을 활성 writer로 보아 거부해 실행되지 않았다. 기존 resource-probe exec62782 exit0 외에 새 Computer Use 창 목록의 Helper 부재와 Get-Process의 d2bot/probe 부재를 추가 확인했다. 같은 정리 요청의 재검토는 통과했으며 I-loot659 payload 전체 해시/정확한35개 extras·경로/일반 파일/containment·reparse0·원본과 승격 PNG/요약 보존을 확인한 뒤 recordings35files/266,323,943B와 종료된 로그8files/86,869B를 제거했다. I-loot는 깨끗한660files로 돌아왔고 현재 J660files·필수자산/소스·공용 Cargo cache는 보존한다. task root에는 재개용 config11files/38,769B와 미해결 retained-diagnosis12files/16,884,893B만 남는다. config는 감독 시험 목적 종료 뒤, 진단은 원인 해결/필요 회귀 승격 뒤 제거한다. 새 실게임 조작 인계 질문은 아직 대기한다.

최종 문서13개의 상대 링크 대상162개 누락0, source240 SHA 재일치, 현재 J660files의 모든 payload 길이/해시·정확한 inventory 재일치를 확인했다. 후속 변경은 문서/정리 기록뿐이며 새 runtime/EXE 변경이나 J 게임 입력은 없다.

## Source Session Close — 2026-10-31

사용자가 실게임 재개 대신 Rust 전환 세션 정리·모든 Python 코드 제거·리뷰/commit/push를 요청했다. 이 범위에서는 새 입력·방 시도를 하지 않았고 R01–R18의6정상/12실패/연속0과 J 미실게임 상태를 유지한다. 최종 Rust-only 검사/배포·소스 정리·보존 자료·새 세션 인계는 [마감 run](run-20261031-01-rust-source-retirement.md)이 소유한다. 이전 임시 config overlay11파일/38,769B는 참조 없는 종료된 시험 설정이라 제거했으며 다음 시험은 현재 config와 GUI 창/캡처 선택으로 시작한다. R14/R17/R18 원인 분석 자료의 한정 보존/정리 조건은 마감 run receipt를 따른다.
