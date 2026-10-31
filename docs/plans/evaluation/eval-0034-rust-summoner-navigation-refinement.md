# EVAL-0034: Rust 서모너 경로·근접 반응 개발

## Metadata
- Result: `FAIL` — 새 방 정상 마감의 기능 수용. I-loot의 R18도 미완료이며 J의 관련 release164 검사·배포는 통과, 실게임은 미검증이다.
- Evidence Coverage: `partial`; Date: `2026-10-30`; Attempt: `1`; Profile: `backend-product`; Evaluators: Contract, Functional.
- Feature: [FEAT-0031](../feature/feat-0031-rust-summoner-navigation-refinement.md); Spec: [SPEC-0031](../spec/spec-0031-rust-summoner-navigation-refinement.md); Run: [RUN-20261024-03](../run/run-20261024-03-rust-summoner-development.md).

현재 checkpoint는 R18/I-loot 실패 뒤 J의 처치/정상 포탈 마감과 GUI 로그 보정 EXE 준비다. 여섯 번째 묶음은 R16정상/R17실패/R18실패, 누적6정상/12실패/연속0이다. R18 봇·녹화/GUI는 정상 종료했다. 사용자가 정정한 정상 마감은 처치 확인→일지→붉은 포탈→협곡 WP→Act1이다. 미확인/키 없음의 중앙 복귀 분기는 실행·배포 전 제거했다. J는 관련 release164 검사와24.06MB 배포/외부CWD actual EXE dry를 통과했으나 실제 처치/협곡 마감은 미검증이다. Esc 뒤 조작 인계를 기다리며 아래 D–I의 당시 검사·크기·hash/실게임을 J 결과로 소급하지 않는다.

이전 검증 범위: R09/D는 정상 마감, R10/E는 북쪽 탐색 중 각도 기준 불일치로 정지했다. F의 등각 보호/단계별 버프 전달/최근2회전·bookend 우선 제출은 개발129검사·release integration35/fmt/Clippy/실행파일 및 portable package 검사를 통과했다. F 실게임R11은 종점 확인 뒤 복귀에서 정지했고 R12는 정상 마감했다. G의 R13–R15는1정상/2실패이며 새 복귀 재관측 분기는 입력 없는 회귀만 통과했고 실제 해당 분기는 미실행이다. H의 초기 짧은 굴곡과 BGR 통계 공유는 아래 별도 checkpoint로 기록한다.

## Evaluated Scope

일반 탐색/복귀 반경240×144, graph 비국소화 때 현재 바닥 굴곡의 연속성, 보스 접근의 기존800px 후보 활용, 발판 몸통 호버, 확인했던 종점의 현재 보조 문양 재접근, 자기 전리품 라벨 OFF 뒤 전체 HUD 재확인을 검사했다. 보스/정지 방어300×180·현재250ms/실제 커서·2회 접근·12초·종점2초·미확인 생사/획득·Stop/포커스·자원 회수는 유지한다. WGC/GPU·게임 내부 접근·TP·자동 부활을 추가하지 않았다.

D 검증 당시 배포는 `dist/d2bot-windows-x64-summoner-dev-d/`였다. 구D–G 사본과 원시는 필요한 요약/회귀 승격 뒤 정리했으며 기본 배포와 깨끗한 H fallback·현재 I를 보존한다. D EXE16,901,632B/SHA256 `e4d9465043112f9b6f2b069f4e786885442ada2f067e2beb08790c229dc482e0`, source239파일 집계 `c813392561ee82d5569af50d6c4ae86228f65151f25b73166e1b920b37b30e95`다. 집계는 모든 src 파일+Cargo.toml/Cargo.lock을 정렬한 상대 POSIX경로/공백/각 SHA256/LF의 SHA256이며 이전 다른 집계와 동일성을 추정하지 않는다.

이전F checkpoint는660files/24,014,122B, EXE16,960,512B/SHA256 `0c0fbff6d70b2c4158bbe08a23d5dc40d427daa2e6aaaaa2fe11180165f26661`, manifest `6b5a8ff6f1830f7b5bf3b5405df6d05e51d88708e2971cc142399484165b6625`, source240 SHA `f9e2dcb77ac6e0c50a96175e21924cef24e8a378c03427b21d2aac22296869ba`였다.659 payload 경로/길이/SHA·미등록0·실제19system DLL imports·필수4region·금지runtime 없음·외부CWD diagnose/typed assets/input0/capture0/north2 dry가 확인됐다. 실제 native GUI/플레이는 production backend를 쓰는 resource-probe로 검증했고 actual F EXE GUI 자체의 직접 실행과 구분한다. F 후보/원시 진단은 분석 종료·소유 확인 후 정리했으며 현재 재생 의존 경로가 아니다.

## Contract Evidence

- C 직전 기준 전체 release339 PASS/0 fail/6 ignored 후, 마지막 발판 호버 보정은 관련 release131 PASS로 재검사했다. C의 명시 native fixture/GDI/listener3+읽기 전용 capture1 PASS도 확인했다.
- D의 공통 라벨 정리 변경은 관련 release164 PASS/0 fail/5 ignored(라이브러리107+통합57,11 result groups), fmt/check/all-target Clippy PASS다. HUD 진단 한 장의 소유권 전달/해제 회귀를 숫자 미확인·지속 초상 미확인·복원 후 재확인까지 확장한 뒤 해당1검사를 다시 통과했다. 전체342개를 D에서 다시 실행했다고 주장하지 않는다. D는 새 native FFI를 추가하지 않았다.
- 라벨 OFF 예외는 prepared controller가 켠 ON에만 적용된다. 현재 건강 비율/닫힌 벨트/패널 없음/250ms/포커스/Stop을 통과해야 한다. OFF 뒤50ms와 새 전체 HUD의 기존 .75초 제한을 통과할 때까지 별도 pending 상태가 이동을 막는다. 계속 초상 미확인·위험 비율·패널·오래된 화면·Stop/포커스·키 전송 실패는 입력을 거부한다. 보고는 중단/재확인 시각을 구분하며 줍기/처치 완료를 만들지 않는다.
- 실제R08 HUD에서 생명/마나1.0과 초상 미확인을 그대로 재현했다. portrait .92/가시50%를 낮추지 않았다. 보존한 HUD는 개발 회귀이며 runtime에서 제외한다.
- R02의 실제 graph↔floor 두 화면, R05의 실제 넓은 바닥 접근, R06의 실제 시간 간격0.7275053초의 상자/보조 문양 접근을 재생했다. 부위 제거/시간 만료에서 거부하며 보조 문양만으로 최초 종점이나 도착을 승인하지 않는다. R04/R05의 외형 회귀는 반대 생사 상태를 만들지 않는다. 새 runtime crop은56×42px floor loop 한 장이다.

## Distribution Evidence

D는660files/23,955,242B(659 payload23,824,854B+manifest)다. package manifest SHA256은 `5586c033729564caf311201473c375b59866b9df08c7f7d5d5b3f4b54b256f10`이다. 모든 payload의 경로 containment·길이·SHA가 맞고 미등록 파일은0이다. 실제 PE의19개 Windows system DLL import가 manifest와 일치하며 외부 Python/OpenCV/FFmpeg/Xvid/CRT DLL은 없다. 필수4개 region과 새 floor template이 포함되고, 물리 references는 명시한 town loading/exit 두 장뿐이다.

외부 CWD에서 D `--diagnose`, 실제 typed assets 로더(profile3/config asset path5/input0/capture0), CLI의 입력 없는 north3-room dry를 통과했다. C의 실제 배포 EXE GUI/Flash·SCREEN·Idle/정상 exit0을 확인했다. D의 패키지 main EXE GUI 자체는 직접 실행하지 않았고, 아래 R09는 production backend를 감싼 resource-probe GUI의 실게임이다. source의 UI/진입 routing은 D 변경 대상이 아니며 두 실행 범위를 구분한다.

## Functional Evidence And Open Failures

R01–R06은2정상 마감/4실패였고 C의R07–R08은 둘 다 미완료다. 사람의 새 상태 확인 뒤 D의R09가 정상 마감해 세 번째 묶음은1정상/2실패였다. R10/E 실패·R11/F 실패·R12/F 정상 마감까지 네 번째 묶음도1정상/2실패, R12 당시 누적4정상 마감/8실패/연속1이었다. 현재 R17까지의 집계는 위 checkpoint와 아래 수용 영향이 소유한다. 북쪽10연속·북남 이후 단계·보스/열쇠/협곡 수용은 미완료다.

| Case | 관찰과 한계 |
| --- | --- |
| R07 | 북쪽 계단→현재 서모너 발판.3관찰/접근2/전용Nova0/생존·시체 판독0으로 kill_unconfirmed. 목표 거리가491.60→131.31→126.80px여서 새100px 접근과 기존2회 상한의 결합을 더 검토해야 한다. 뒤 정지 방어 Nova와 누운 외형은 자동 처치 승인으로 세지 않는다. |
| R07 복구 | 기존 north return→중앙WP→Act1→저장 종료1회/errors=[]로 완료. 새 방 성공에 더하지 않는다. |
| R08 | 아케인 입장/버프 후290고유 nav frames 탐색. 전투 후 라벨이 초상을 가렸고 OFF 정리도 mercenary_unverified로 거부됐다. 이후 현재 안전 종료는21관찰/선택 확인61.6302543초에 완료됐다. 실제 사망/자동 부활을 추론하지 않는다. D는 이 정리 경로의 입력 없는 보정이며 실게임 재발 해소는 아직 미검증이다. |
| R09 | 사람의 용병 정상·Act1 확인 뒤 D backend로 북쪽 포탈 지형 종점→중앙→Act1→저장 종료/errors=[]를 확인했다. acceptance_room=false인 개발 정상 마감이며 실제 가림 예외/보스·열쇠 성공의 증거가 아니다. |
| R10 | E backend의 북쪽 탐색 중 current-floor bend_entered[270,539]가 raw150.2도 보호로 거부됐다. flow의 등각 각도는128.6도다. root interrupted는 이후 F10 감독 종료이며 그 전의 field_paused/opposite_wing_aim_rejected와 구분한다. 종점 승인/복귀 단계는 없었고 R11/R12 미시작이다. |
| R10 복구 | 별도 기존 north return 한 번으로 중앙→Act1→저장 종료/errors=[]를 확인했다. 새 방 성공에 더하지 않으며 새 사람 상태 gate는 발생하지 않았다. |
| R11/F | 현재 종점 승인 후 복귀에서 graph_entry_retreat_exhausted→stuck_no_observed_escape로 정지했다. F10 뒤 root field_paused/errors=[]다. 기존 north return 새 실행1회는 중앙→Act1 저장 종료로 완료됐으나 실제 화면/버프/merc-monitor 설정도 달라 원인을 이력만으로 확정하지 않는다. |
| R12/F | 현재 종점→중앙→Act1 저장 종료/north_room_closed/errors=[]다.9 loot 검사OFF/복원오류0, stream30저장/회전3생략/오류0, 실제 return bookend2장 모두 존재한다. 개발 정상 마감이며 보스/열쇠/협곡 수용은 아니다. |
| 시작 환경 | 최초 sandbox 실행은 창 전환 단계에서 실패했다. report action input_events=[]이며 native focus helper의 기존 Alt fallback까지 무입력이라고 주장하지 않는다. 같은 binary를 sandbox 밖에서 실행하자 입장에 성공했다. focus 함수를 바꾸거나 실패를 무시하지 않았다. 이 시작 실패는 새 방 수에서 제외한다. |

R08의 `operator_review_required=mercenary_portrait_unverified`와 [재개 규칙](../../features/summoner-run.md)에 필요한 새 사람의 **용병 정상·Act1 저장 종료** 확인을 받고 R09를 실행했다. D production backend의 resource-probe GUI에서 북쪽 포탈 지형의 현재 종점→중앙WP→Act1→저장 종료가 `north_room_closed`, errors=[]로 완료됐다. acceptance_room=false인 개발 시험이며 보스 처치/열쇠/협곡 성공이 아니다.15 loot scan 모두 라벨OFF 전송/최종OFF, 복원 오류0이지만 HUD 재확인 예외 metadata는 없어 실제 가림 예외가 사용됐다고 확대하지 않는다. 새 operator review는 없다. GUI/녹화는 exit0로 마감했다.

## Observed Timing And Resources

아래는 녹화 병행 C 두 방의 p50/p95(ms)다. phases/navigation은 source 시각·capture sequence로 중복 제거하고, 새F2는 이미 held인 반복 keydown을 제외했다. 유한 비음수만 사용하며 정렬 위치(n−1)×p의 선형 보간이다.

| 측정 | R07 | R08 |
| --- | --- | --- |
| capture 시작→완료 | 26.230/43.668 (n254) | 25.731/43.812 (n290) |
| source→근접 판독 | 61.247/79.755 | 60.097/79.093 |
| source→처리 완료 | 116.046/145.994 | 113.836/150.107 |
| source→새F2 backend 반환 | 149.029/169.410 (n34) | 125.203/184.150 (n41) |
| 판단 준비→새F2 backend 반환 | 11.243/27.860 | 10.806/18.141 |

R09의936고유frames를 같은 방식으로 집계하면 capture26.056/43.121, near60.777/81.397, processed117.537/148.844, 새F2 backend138.698/183.233(n192), ready→backend12.699/28.631ms다. 같은 계산의 G 기준은 처리116.998/180.157, 새F2 138.096/172.096ms였다. 맵/전투/실행 환경·정지 시간이 달라 인과 개선 비교가 아니며50ms 전체 반응 달성을 주장하지 않는다.

| 자원(B; WS/private/기본 힙 할당) | R07 | R08 |
| --- | --- | --- |
| 개별 counter의 관찰 최대 | 452,247,552 / 433,119,232 / 335,737,365 | 421,453,824 / 399,851,520 / 312,175,569 |
| 마지막 timed idle | 121,061,376 / 101,801,984 / 19,591,277 | 119,648,256 / 95,907,840 / 19,613,608 |
| GUI Drop 뒤 | 112,812,032 / 89,477,120 / 13,478,376 | 112,009,216 / 90,292,224 / 13,484,343 |

최대값은 같은 한 순간의 세 값이 아니며 정지 방어도 포함한다. 기본 힙 할당은 전체 private memory나 모든 driver/GPU heap과 같지 않다. 관찰한 해제와 일반적인 무누수·peak 최적화 보장은 구분한다.

R09 최대는665,993,216/650,838,016/533,771,834B, 마지막 timed idle111,222,784/107,368,448/21,139,591B, GUI Drop99,880,960/95,670,272/13,482,750B다. 실제 진단 PNG67장/193,025,620B가 저장됐으며 원본 BGR은 장당6,573,240B다. Vec의 Arc 개수와 서로 다른 실제 backing buffer 수는 같다고 가정하지 않는다. native nav 선택 이미지가 interval/방 끝까지 남는 경로는 확인했으며 현재 제한된 저장 worker로 보정 중이다. 수동 녹화는1frame 동기 write이며 별도 무제한 frame queue가 원인이 아니다.

R10은1189고유frames/capture27.443/43.165, near61.372/81.093, processed113.522/141.867, 새F2 backend137.436/182.233(n184), ready→backend12.109/27.545ms다. 최대 WS/private/기본힙557,461,504/537,227,264/452,483,386B, 마지막idle133,783,552/116,420,608/21,870,879B, GUI Drop118,661,120/99,012,608/13,652,946B다. 수행 단계/지도/버프 갱신2기록과 정지 방어 시간이 달라 R09와 인과 비교하지 않는다. E의 stream31저장/28생략/실제 저장 오류0은 정상 작동과 진단 소실을 함께 보여준다. 최근8회전의 종료 burst가 before/after를 밀어냈으며 metadata final_frame은 null/상태partial로 남았다. F는 이 제출 순서와 버프 trace 보유를 보완한다.

R09 단계 차이의 p50/p95(ms)를 분리하면 outbound/return은 capture완료→ground10.099/17.719·8.925/10.851, ground→near24.213/32.302·24.300/30.337, near→planning제출12.613/26.785·41.925/52.706, planning제출→path합류17.868/20.977·0.010/0.015, terminal합류→motion완료15.767/21.871·14.428/23.623다. stage 사이 경과값은 각 작업의 순수CPU시간이나 서로 독립적인 합계가 아니다. return의 hub 전체 판독이 planning 제출 전에 동기 수행되는 호출 경로와 함께 다음 반응 지연 조사 대상으로 삼으며, 이번 진단 메모리 보정이 이 지연을 없앤다고 주장하지 않는다.

수동 AVI4개는 모두8fps/1922×1140, 양의 frame count와 첫/마지막 decode를 확인했다. 순서대로994frames/65,822,470B,2295/152,417,042B,995/81,843,596B,3829/278,489,532B다. 개발용 OpenCV를 검증에만 사용했고 배포에는 포함하지 않는다. 이 검사는 모든 중간 frame의 품질/실제 wall-time 대비 drop 수를 검증한 것은 아니다. codec 구현은 이번 변경 대상이 아니다.

추가 R09/D AVI는1483frames/158,757,150B, R10/E는3001frames/220,063,312B로 같은8fps/1922×1140/첫·마지막decode를 확인했다. 메뉴·정지 감시도 포함하므로 전체 gameplay 길이·실제 drop수나 압축기 인과 비교로 확대하지 않는다.

## Acceptance Impact And Next Work

H의 R16정상/R17실패 뒤 부분 버프 복구에 필요한 [사람 확인](../../features/summoner-run.md)은 정상·계속 진행 답변으로 해소됐다. R18의 정확한 실게임 재승인도 받아 I-loot로 실행했지만, 단상 도착 후4관찰에서 생존/시체를 읽지 못하고 접근2회를 소진했다. 전용 Nova0과 그 전 navigation F4 12전송을 구분한다. 대기 방어 후 시체가 보이는 마지막 PNG는 있으나 정식 생사 판독으로 돌아가지 않았다. 두 포인터 mapping 오류로 실패/field retained·Act1 미마감이다. Esc 뒤 새 Computer Use 재개·확인/종료 허용을 받고 잔여 녹화와 GUI를 exit0로 닫았다. 사망/초상 미확인이나 자동 처치 성공으로 재라벨하지 않는다. 여섯 번째 묶음1정상/2실패, 누적6정상/12실패/연속0이다.

J는 한 번의 지도 숨김/복원과 현재300×180 내 몸통 후보를 우선하며, 대기 방어의 신선한 현재 시체 후보에서 입력 해제·worker 종료 후 정식 encounter를 한 번 재실행한다. 신호 자체는 죽음이 아니고 새 순서 있는 시체 쌍·정상 전리품 검사가 필요하다. 갱신된 버프 시한/새 pool·이전/새 보고서와 증거명을 구분하고 정상 일지/협곡 경로를 유지한다. 기존 right lower-band만 native .85+독립 blue/gold 각 .80을 요구하며 runtime29개 part/PNG는 증가하지 않았다. R18 source .858879/blue .816327/gold .827957과 기존14alive/105scene의 개발 점수 조사 뒤 실제 native 재생을 검사한다. GUI의 바닥 따라가기/위로 읽기/바닥 복귀 회귀는 통과했다. 최종 native corpus·release·EXE/실게임 범위는 run이 소유하며 아직 J 실게임을 주장하지 않는다.

R18의185고유 nav frames p50/p95(ms)는 capture26.243/46.104, near59.436/77.736, processed113.421/138.197, 새F2 backend135.816/152.986(n22/반복40제외), ready→backend11.993/24.320이다.4 loot 검사 클릭/접근0·최종OFF다. resource peak WS/private/기본힙은487,882,752/469,057,536/381,277,972B, GUI Drop100,737,024/93,720,576/13,565,655B이며 종료0을 확인했다. AVI3,127frames/174,288,840B·전frame decode PASS지만8fps header가 실제 cadence/drop 수를 보장하지 않으며 수동 개입 가능 시간도 포함한다. 다른 지도/단계의 인과 속도·메모리 개선율이나 일반적인 무누수를 뜻하지 않는다.

I 안전 보정은 실패 전송 metadata/무기불명확 상태의 재시전·정지방어입력 차단과 placecap뒤새chart번호 재사용 방지를 구현했다. 그래프 최대place96/reference12는 유지한다. source240 SHA `191a3c29eaced442dad300cac1fbeeb6a01f4f01829bfaa3a5203921a41d77b0`이며 신규graph재생은 기존번호1/기대2 FAIL→수정뒤 기존221연산 포함3검사PASS다. 당시 fmt/check/all-target Clippy·개발151/release151 PASS/각5ignored와 EXE3b4a9997/manifestb32ee580의660file/24,027,941B 배포·19system imports/필수4region/외부CWD dry/typedloader가 통과했다. 이후 같은 I 코드에 제한된 줍기 설정을 담은 I-loot로 R18을 실행했지만 이번 부분 버프 실패/차트 고갈 분기의 실게임 검증은 아니다. 상한슬롯재활용/판독병렬화는 미구현이며 전체hash/검증별범위는run이소유한다.

H checkpoint: 초기 굴곡의 짧은 현재 측면 탐침과 RegionObserver의 scan 내 BGR 통계 공유를 구현했다. 정확한R13회귀의 현재East 바닥/실패제외/stale/바닥제거, 세matching mode score-bit 일치·float NaN 거부와 기존 parity를 확인했다. fmt/check/all-target Clippy, 개발142 PASS/5ignored와 release154 PASS/5ignored가 통과했다. source240 SHA `ae6a674af1a0ed51ed02f4a734f006ecbd3308e274dc168e661cc52f5ec9f10b`, EXE SHA `5ebc9b9b03a5fd49e445d52f450921fd6985424c31f05352961f9adc552c9751`, manifest SHA `14233ffdfd626d4039aad1f1886c0cdad2877aed47bb5b63d1a469dbd01d201e`, bundle660files/24,023,845B다. 모든659 payload 경로/길이/hash·미등록0·19system imports·필수4region/기존supportcrop·개발fixture제외와 외부CWD dry/typedloader가 확인됐다. actual main EXE의 입력 없는 검사와 production backend resource-probe의 live GUI를 구분한다.

고정화면30샘플/3warmup/nearest-rank에서 H near p50/p95는17.2038/21.6604ms, 이전binary의 앞/뒤 측정은17.8504/23.5594·18.8459/23.4623ms였다. near+route aim은18.2736/21.3417 대18.6484/21.3079·18.9011/21.1239다. 같은source/floorhash·입력0/캡처0이며 일부중앙값의소폭감소만관찰했다. baseline binary hash/다른stage변동은run이소유하고 전체실시간50ms나큰개선율로확대하지않는다. 녹화 병행 H 실게임은 R16/R17까지 완료했으며 R18은 사람 확인 대기다.

G의 R13/R14는 두 실패다. R13은127고유frames의 outbound에서 현재 floor 역방향/비국소화로 `opposite_wing_aim_rejected`, R14는432frames의 outbound에서 현재 종점 접근1회 뒤 `terminal_reacquisition_required`였다. R14의 F10 중 control capture stopped/interrupted는 앞선 길찾기 정지와 구분한다. 두 감독 종료 뒤 기존 north return으로 각각28.21초/122.87초에 중앙→Act1→선택화면을 완료했고 새 방 성공에는 세지 않는다. 두 복구의 새G route_graph_reacquire marker는0이며 보정의 실게임 효과는 미확인이다. R15는 north_room_closed로 완료돼 다섯 번째 묶음1정상/2실패, R15 당시 누적5정상/10실패/연속1이었다. 세부 계측은 아래와 run에 기록했다.

R13/R14의p50/p95(ms)는각각capture27.105/43.924·25.804/45.338, near61.603/77.455·58.642/77.943, processed117.890/146.055·116.065/241.050, 새F2backend137.519/187.523(n25)·137.481/179.369(n45)다. stream은13/13저장+회전2생략·21/21저장+회전1생략/오류0이며정확한bookend가있다. GUI Drop기본힙은13,490,390B·13,459,528B였다. 서로다른지도/길이의인과비교나전체메모리무누수보장은아니다.

R13/R14 AVI의 RIFF metadata는574frames/45,458,926B·1144frames/86,644,784B, 모두8fps/1922×1140/XVID handler다. 기본/Anaconda Python만 본 중간 미검증 상태는 프로젝트 `.venv/Scripts/python.exe`의 OpenCV4.13.0으로 해소했다. 두 파일과 R15(2165frames/214,557,128B)의 첫/마지막 decode가 모두 PASS다. 전체 중간 frame·실제 wall-time drop 수는 검사하지 않았고 배포에 외부 decoder를 추가하지 않았다.

R15는 현재 종점→중앙WP→Act1→선택화면을 정상 마감했다.1402고유frames의 p50/p95(ms)는 capture24.939/43.124, near56.782/76.281, processed115.198/146.413, 새F2backend139.203/185.111(n267), ready→backend12.718/29.797이다. loot15검사 모두OFF/복원오류0/click·teleport0, stream54저장/회전7생략/오류0, GUI Drop기본힙13,664,392B다. 다른 지도/길이의 수치이며50ms 달성·일반적 무누수를 뜻하지 않는다. outbound 현재 종점7관측과 접근0회를 확인했으며 return navigation만 보고 나온 최초 ‘종점0’ 집계는 폐기했다.

새 개발 후보G의 입력 없는 checkpoint: 내부114개 개발 검사와 강화한 이벤트 순서/중단2개, fmt/all-target Clippy PASS, release integration14개(continuation2/field-runtime12) PASS다. 내부114개를 release에서 전부 다시 실행한 것은 아니다. source240 SHA `3dfc15aee4f0c4b9b693ce1b7abbfa6efb17a5c0af71d1a87147b1634476fe4a`, EXE SHA `4925ebb10392c0441a517f20ee17d0c6e8ba48c3077b4cc5dc5a08430bb30b64`, manifest SHA `3f8b224a23a56e1fa4407250a1b78a61d3bc7ab020bc25f2a8e3bc889dbb3152`다. 660files/24,021,802B와 모든659 payload 길이/hash·경로/미등록0·19system imports·필수4region·금지runtime 없음, 외부CWD dry/typed assets 검사를 통과했다. 이는 R13–R15 실제 복귀 보정의 성공 근거를 대신하지 않는다.

제한된 구현/배포 계약과 실제R09·R12·R15·R16 정상 마감은 확인했지만 처치 판독→정상 일지/협곡 마감·실제 가림 때의 라벨 복원·복귀 이력 소진·새 방 반복은 기능 수용을 막는 남은 작업이다. G의 return graph 새 frame 재관측은 입력 없는 회귀를 통과했으나 실제 해당 분기는 미실행이다. 여섯 번째3회 검토 뒤 J의 관련 검사/EXE를 검증하고, 현재 조작 인계·장면 조건이 확인된 범위에서 정상 포탈 경로를 실제 확인한다.50ms는 지향 목표이며 그 숫자만으로 수용/실패를 정하지 않는다. 기능·안전·자원 해제 근거를 먼저 유지한다.

필요한 회귀/결과를 각 owner로 승격한 뒤 알려진 writer 종료·경로 검사를 거쳐 구 후보A–G와H recordings·이전CPU기준의원시진단을 정리했다. 세부 범위와 제거 수는run이소유한다. 이 평가의 수치/검사 결과는관찰 당시의요약이며 삭제raw의무기한재생을약속하지않는다. 기본배포·깨끗한H fallback/현재I·source/config/필수assets는보존한다. 미해결진단은task의retained-diagnosis(6PNG+metadata/provenance;11files16,658,176B), 다음감독재개는task config overlay만유지하며 각목적이끝나면제거한다. 프로그램/정식검사는이임시경로에의존하지않는다.

I의 최종 읽기 전용 검사에서 관련 문서9개의 상대 링크156개 누락0, source240 SHA 재일치와 깨끗한 H/I 각660file의 전체 payload 해시·길이·정확한 inventory·containment·reparse0을 확인했다. source/tests/examples/scripts/config에는 정리한 구 배포 경로의 실행 의존 참조가 없다. 완료한 task scratch67files/8,881,430B도 제거했고, 일반 Cargo target/cache는 보존한다. 임시 root에는 위 재개용 config와 미해결 retained-diagnosis만 남는다.

J 최종 source240 SHA는 `d2fceb352869735d9d0ac88978b8a77f9f1ab4aa98d2420100e35268b8671a58`다. fmt/check/all-target Clippy와추가 debug corpus1/appearance1 PASS, static-CRT release library122+integration42=164 PASS/0fail/5ignored다. 변경 part 단독의 native 비검출120개/비정규크기 거부5개, 전체29part의 R18 current-platform/body 제거·무채색·생존·duplicate와 기존외형 parity, map/Stop·늦은 corpse 재진입/갱신buff·협곡 분기, 실제 egui wheel 스크롤 동작을 구분해 검증했다. EXE SHA `02a9f382af8a995d13d514c6fc0bdad8467c84fe28c9dab1f348628201cc18f2`, manifest SHA `230bf867f8456ff49c3c1d7d08262d958eb98210e69ccf48dca951b152bdc6f3`,660files/24,060,892B이며 전체 hash/inventory·필수4region·개발reference제외·19고유system/API DLL·외부CWD diagnose/dry·stage typedloader를 통과했다. 실제 J 실게임/포탈 마감과50ms는 미검증이다.

R18 원시와 종료 로그는 요약·필요 reference 승격·미해결 pointer metadata 한정 보존·writer 종료/소유/정확한 경로 검사 뒤 제거했다(35recording files/266,323,943B+8logs/86,869B). 최초 자동 검토 거부는 새 Helper 부재/exec0 근거로 해소한 뒤 같은 요청으로 정리했다. 현재 task 임시 root의 config11files/38,769B는 다음 감독 시험용, retained-diagnosis12files/16,884,893B는 R14/R17/R18 미해결 원인 분석용이며 각 시험 목적 종료/원인 해결·필요 회귀 승격 때 제거한다. 배포/소스/정식 검사는 임시 폴더에 의존하지 않는다.

2026-10-31 세션은 [Rust-only 소스 정리](../run/run-20261031-01-rust-source-retirement.md)로 전환해 마감한다. 새 게임 시험은 없으며 이 평가의 기능 FAIL/partial·R01–R18 집계·J 실게임/정상 포탈 마감 미검증은 그대로다. 임시 config overlay는 제거했고 최종 소스/검사/배포와 보존 진단의 최신 상태는 마감 run이 소유한다.
