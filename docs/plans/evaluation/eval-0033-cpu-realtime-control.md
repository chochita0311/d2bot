# EVAL-0033: CPU 제어 경로와 실제 입력 지연

## Metadata
- Result: `PASS WITH SUGGESTIONS` — 구현한 G의 제한된 CPU 개선·안전 회귀·배포 검사 범위. 전체 최적화/게임 수용 완료는 아님. Date: `2026-10-24`; Attempt: `1`; Evidence Coverage: `partial`.
- Feature: [FEAT-0030](../feature/feat-0030-cpu-realtime-control.md); Spec: [SPEC-0030](../spec/spec-0030-cpu-realtime-control.md); Run: [RUN-20261024-02](../run/run-20261024-02-cpu-realtime-control.md).
- Profile: `fullstack-product`; Evaluators: Contract, Functional. Primary owns architecture, evidence interpretation and final review.

## Boundary And Measurements

판정 해석 정정: 종전 `FAIL`은50ms와peak 목표 미달을 필수 합격선처럼 적용한 결과였다. 사용자는50ms가 기능·판독 품질·안전성을 유지하면서 효율을 최대한 높이기 위한 지향 목표이며 달성 불가능성도 허용한다고 명확히 했다. 이에 기존 검사로 검증한 G의 제한된 개선 범위를 위 결과로 정리한다. 새 실행/측정 없이 기준 해석만 정정한 것이며 기존 수치와 중앙 재획득/협곡 실패는 그대로다.50ms 미달만으로 실패라고 하지 않고, 실제 기능/안전 실패와 아직 제거 가능한 비용은 계속 추적한다. 전체 게임 기능/판독 품질의 동등성이나 최적 효율 달성은 입증하지 않았으며 FEAT-0030 전체 완료로 승격하지 않는다.

현재 채택 checkpoint는 G다. production239파일 집계 SHA256은 `4ae2e862adbc1374b626c8df27bd5841f9e0d191109314d774b863c93bfdb3ac`, 최종 EXE16,868,352B/SHA256 `eb820ef28f7717d02b5b3aa8460160fada22c55097d51ef7457f12b79dc800c9`이다. 최종 fmt/check/all-target Clippy와 static-CRT release331 PASS/0 fail/6 ignored(59 result groups,9분7.53초), 명시 native fixture/GDI balance/listener3 PASS와 read-only capture1 PASS, 새 링크를 통과했고 경고가 없었다. H는 미채택이며 H의 수치를 현재 성능으로 쓰지 않는다.

감독 G EXE c4b063c9와 새 링크의 차이는 실제24byte로, 파싱한 COFF timestamp·debug directory timestamp3개·RSDS GUID에만 있었다. `.text` SHA256 `cc1b717b2487e2bb1ad3bff66962991eab83449f3b0346fab9f31f5ce47abcfe`를 포함해 해당 메타데이터 외 파일 전체가 동일하다. 최종 EXE와 보존한 작은 원래 metadata 값으로 c4b063c9의 전체 SHA도 정확히 재구성했다. 실행 코드를 바꾸지 않은 새 링크임을 확인했으며 원본 EXE를 패치하지 않았다. 필드 해석은 [Microsoft PE format](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format)을 따랐고 재구성에 필요한 원래 metadata와 결과는 [작은 PE 검증 receipt](eval-0033-cpu-realtime-pe-receipt.json)가 소유한다.

현재 배포는 `dist/d2bot-windows-x64/`,659파일23,901,410B/manifest SHA256 `4432b4f3518289cdf911a7d403ee57050170972ba05ebf21a08816c21a913224`다. 필수 regions137파일1,780,641B를 포함한다.658개 payload의 크기/SHA와 미등록 파일0을 교체 전후 확인했고 실행중 writer는 없었다. 다른 CWD에서 default config/HUD/typed loader/dry run·region 누락 실패/원래SHA 복원·실제 배포 EXE GUI 실행/정상 exit0을 확인했다. 경로 교체 뒤 diagnose와 전체 typed loader를 다시 통과했다. profile3/config asset path5, smoke 중 입력0/capture0, PE imports19개는 Windows system DLL뿐이며 Python/OpenCV/FFmpeg/Xvid/외부 CRT가 없다.

기술 회귀 PASS가 반응 목표 PASS는 아니다. 현재 source→near58.280/72.101ms(p50/p95), 새F2 source→backend138.095/172.567ms로50ms 미달이며, 반복 전체가 끝나야 다음 화면을 읽는 경계가 남아 있다. 표시된 화면 변화→입력·게임 효과50ms도 입증하지 않았다. peak WS554,102,784B/private525,078,528B는 개선 완료라고 볼 수 없다. GUI drop 후 live heap13,441,977B와 WS113,623,040B는 구분한다. 큰 임시 계산 메모리와 독립 fast 판단 cadence가 다음 수정 대상이다. 중앙 재획득·협곡 마감·전체 새 방/처치/획득은 계속 미확인/실패 경계를 유지하며 별도 복구 성공으로 대체하지 않는다. FEAT-0030은 완료하지 않는다.

사용자는 안전한 구조 개선·실제 화면 읽기→판별 단축을 최우선으로 승인했다. 기존 타이밍/threshold/플레이 적응은 후속이며 완전 Python 동작 일치나 전체 방 마감을 구조 개선의 선행 gate로 삼지 않는다. WGC/GPU/게임 내부 접근과 외부 codec runtime은 추가하지 않는다. 단일 입력 소유·현재 source·unknown·250ms 만료·focus/geometry/Stop·overflow 검사·실제 join/해제는 유지한다.

비교 기준은 [EVAL-0032](eval-0032-rust-runtime-efficiency.md)의 EXE `92ee301f0fde68e4bc1dda880eb681c31bbc636022d033b5fa5232e90779d990`, production237파일 `7d56f7185fb5f2b1b1bc547aa8dd10d15f132cb2750704c33c8a8acd6cff9324`다. 계측-only baseline EXE `1e1653fb38ac876356ba7957c0f4b3fcf14d2fbdbbd000f73ef038f2d0ebe2b3`는 입력 의미를 유지했다. Windows11/12논리 코어, Rust1.99.1 release/thin-LTO/codegen1/static CRT를 사용한다. benchmark와 컴파일/회귀를 겹치지 않는다.

`capture_started_at`은 실제 source 읽기 시작, `capture_completed_at`은 읽기 완료다. 단계 차이는 합류/대기를 포함하며 독립 recognizer의 CPU 시간이 아니다. input의 `backend_returned_at`은 native method 반환으로 게임 효과 시각이 아니다. `observation.capture_started_at/decision_ready_at`에 명시 연결된 전송만 입력 지연으로 세고 가까운 frame으로 추정하지 않는다. `processed_at`은 마지막 입력보다 앞설 수 있고 근처 전투는 전체 plan보다 먼저 실행될 수 있다. 모든 percentile은 nearest-rank다.

## Implemented Changes

- panel 전체 resize 대신 같은 전체 좌표의 Area ROI 두 개와 matcher 통계를 공유한다. uint8 직접 correlation은 i16 SIMD 내적과16,384개 block/u64 합으로 계산한다. f32/FFT/threshold/최고점·동점은 유지한다. u32 SIMD 시도는 더 느려 폐기했다.
- 연속 입력의 추가50ms gate를0으로 바꿨다. actual pointer±3px 확인·최대80ms 중단 가능한 대기, 이후 focus/epoch/만료 검사는 유지한다. 같은 이동 hold에서 방향만 바꾸고 전투 전환은 상대 key 해제 후 수행한다. 단발 approach/hover 대기는 별개다.
- 미획득 outbound의 wide terminal/objective는 recognizer당 최대1개 pending이다. 이전 negative/pending은 unknown, positive는 이동 해제 후 현재 frame으로 재확인한다. terminal 접근/재획득은 현재 full scan이다. local phase는 최소100ms source 간격으로 mapping하고 그 사이는 displacement=None/position_current=false, 오래된 graph feature를 현재 aim에 투영하지 않는다.
- 현재 near hazard와 hover는 병렬 시작한다. guarded near combat은 hover보다 먼저 실행할 수 있고 이동은 current hover를 합류한다. 소비하지 않은 작업도 실제 join 뒤 회수한다.
- 캡처는 불변 consumer Arc와 producer1/대기1의 명시 소유다. checkpoint C는 요청 방식·초기1장만 사용하고 focused named AUTO만 SCREEN을 선택한다. 실제 backend metadata를 남기고 wide host는1worker/queue2다. 녹화는 별도8fps bounded owner를 유지한다.
- package marker가 있는 EXE는 sibling assets/default config를 선택한다. 명시 경로는 caller 기준이며 source-tree fallback은 없다. cleanup 보고 오류가 나도 stop/자원 해제를 생략하지 않는다.

## Same Stored Input

같은1922x1140 PNG,3warmup/30sample, 실제 같은 프로세스 recorder ON의 baseline→checkpoint B p50(ms)다. 고정 입력의 판독 결과/픽셀·floor hash는 일치했다. 이 도구의 live capture는 강제SCREEN이고 B 실게임은AUTO였으므로 별도로 해석한다.

| Stage | Baseline | B |
| --- | ---: | ---: |
| Panels | 19.3399 | 6.04 |
| Near hazards | 17.55 | 13.40 |
| Hover | 20.81 | 21.00 |
| Path | 18.59 | 18.88 |
| ORB pair | 19.37 | 16.09 |
| Stored packet | 40.8728 | 37.05 |
| Static encode | 2.16 | 2.12 |

ORB는 이번 batch에서 바꾸지 않아 표본 차이를 인과 개선으로 주장하지 않는다. stored packet은 normalize/gray+HUD+path+ORB pair의 도구 묶음으로 실제 main 전체가 아니다. 녹화는 게임 판단의 보조이며 encode 개선을 control 지연 개선으로 대체하지 않는다.

## Supervised Checkpoint B — Goal Not Met

Flash 새 방/자동 버프/녹화ON에서 텔레포트 방향 변경·Nova와 비보스 상자 플랫폼 도달, 살아 있는 캐릭터/용병을 관찰했다.646개 continuous 행의 capture→near p50/p95/p99/max는103.828/137.643/163.980/205.649ms, capture→processed168.647/270.140/296.417/366.660ms다. local589행 processed166.126/207.713/249.113/269.144ms, mapping_deferred136행/stale_hold50행, near≤50ms0/646이다.

actual stage p50은 capture45.664ms, capture완료→소비 요청15.483ms, ground→near27.526ms였다. 이 근거로 계속60fps 생성/두 broad host 정책은 채택하지 않고 C의 요청 캡처/SCREEN/단일 broad host를 구현했다. baseline은 중앙 인식 hold8행/F2·F4전송0으로 행동이 달라 실게임의 직접 인과 대조군이 아니다. baseline의 processed192.2059/207.3143/207.3143ms는 입력 반응 증거가 아니다.

| 명시 source가 연결된 B 실제 입력 | 건수 | Capture→return p50/p95/p99/max ms | Ready→return p50/p95/p99/max ms |
| --- | ---: | --- | --- |
| 새 F2 keydown | 316 | 197.5/236.5/253.2/261.5 | 22.7/54.4/60.6/76.8 |
| 반복 F2 keydown | 14 | 236.4/259.9/259.9/259.9 | 62.3/107.2/107.2/107.2 |
| 새 F4 keydown | 23 | 158.1/216.2/240.2/240.2 | 9.8/17.9/48.4/48.4 |
| 반복 F4 keydown | 3 | 181.1/204.4/204.4/204.4 | 44.8/46.1/46.1/46.1 |
| 연결된 move | 225 | 171.5/215.0/250.6/304.1 | 2.6/12.6/40.9/66.3 |

총sent1,517건 중 연결581/미연결936(move571/key-up339/press26)이며 미연결을 위 지연에 포함하지 않는다. 전체 sent keydown/up의 마지막 held set은 비었다. 보고는 별도로 `failed`, `acceptance_room=false`, `window geometry changed after capture`다. UI 전환/Stop과 오류의 인과는 확정하지 않았고 geometry guard를 완화하지 않았다. 전체 방/boss/key 성공은 입증하지 않는다.

## Memory And Recording

자체2초 자원 표본과 기본 heap은 OS/driver 전체 heap과 다르다. B224표본 peak working-set539,500,544B/private533,262,336B/default-heap allocated427,004,643B, committed462,831,616B/reserved494,166,016B다. GUI drop 뒤101,924,864B/93,483,008B/13,551,204B이며 heap committed76,406,784B/reserved142,848,000B다. baseline hold-only peak292,454,400B/private279,822,336B/allocated144,293,083B와는 workload가 달라 인과 메모리 개선을 주장하지 않는다. commit/reserve를 모두 live Rust 누수로 해석하지 않는다.

baseline recorder와 B recorder는 실제 stop/file-finalize/join 및 GUI exit0을 확인했다. 개발-only 독립 OpenCV4.13.0/FFmpeg decoder의 순차 전체 decode/seek0·중간·끝 검사는 모두 성공했고 stderr0B다. 이는 배포 runtime 의존성이 아니다.

| 녹화 | Bytes | Reported/decoded frames | FPS/size | Seek |
| --- | ---: | ---: | --- | --- |
| Baseline | 26,265,932 | 617/617 | 8/1922×1140 | 0/308/616 동일 |
| B | 149,411,888 | 2058/2058 | 8/1922×1140 | 0/1029/2057 동일 |

소유 경로 감사에서 GrayMatcher의 integral/energy는 `(W+1)*(H+1)*16`B, correlation cross/score는 출력 픽셀당 약12B이며 호출 중 함께 존재한다. Objective viewport1876×845의 stats만 약25.4MB다. FFT pattern/tile/transpose와 worker scratch도 호출 소유이고 scoped 작업 합류 뒤 해제한다. 전역 cache는 f32/f64 각8개 shape의 transform plan만 보관하며 frame/spectrum/scratch를 보관하지 않는다. terminal의 per-call cache는 좌표 목록이며 negative frame cache가 아니다. 안정화 tracker의 strong BGR 참조는 최대 이전1장(1922×1140에서6,583,320B), reset/relocalization/update로 놓는다. 이 정적 소유 근거는 peak427MB 전체를 특정 할당에 귀속시키거나 allocator 잔류를 입증하지 않는다.

## Checkpoint C — Same Platform OFF/ON/OFF

도달했던 같은 비보스 플랫폼에서 같은 GUI process로8초 interval을 OFF/ON/OFF 순서로 요청했다. 각 run은 현재 중앙 출발 근거가 없어 `outbound_central_reacquire_hold`만14/14/13행 기록했고 F2/F4 keydown은0이다. 모두 failed/acceptance_room=false, evidence complete, 마지막 held set은 비었다. ON은 paused guard geometry 오류도 기록했다. 따라서 아래 수치는 움직이는 게임이나 실제 이동 입력 속도 증거가 아니다. 첫 개발 helper의 output directory 중복 생성(OS183) 실패는 이 세 run에 포함하지 않는다.

| Trial | Frames | Capture→near p50/p95/p99/max ms | Capture→processed p50/p95/p99/max ms |
| --- | ---: | --- | --- |
| OFF 1 | 14 | 66.318/81.140/81.140/81.140 | 105.442/230.530/230.530/230.530 |
| ON | 14 | 65.599/74.285/74.285/74.285 | 101.386/229.642/229.642/229.642 |
| OFF 2 | 13 | 67.508/95.519/95.519/95.519 | 110.616/214.743/214.743/214.743 |
| Combined | 41 | 66.318/75.478/95.519/95.519 | 104.593/214.743/230.530/230.530 |

전체41행이 actual SCREEN이며 near≤50ms는0/41이다. adjacent p50/p95/max(ms)는 capture28.258/37.063/41.005, HUD4.438/5.742/6.029, panel7.961/10.122/10.336, ground3.577/4.934/5.354, near20.445/26.066/63.640, planning-submit32.784/41.365/53.549다. hover는 aim이 없어 합류0.001ms였고 motion은 모두 null이다. 첫 current full objective 합류를 포함한 objective p95125.971ms, max139.384ms다. 독립 stage percentile 합을 전체 percentile로 쓰지 않는다. 녹화 contention이 큰 증가를 보이지 않았지만 작은 정지 표본의 결과다.

119개2초 process-wide 표본 peak는 working-set267,108,352B/private241,012,736B/heap-allocated163,742,283B, heap committed171,487,232B/reserved183,201,792B였다. 개별 trial 시작 시각이 resource log에 없으므로 OFF/ON별 peak로 귀속하지 않는다. 마지막 timed234.247/236.270/238.276초에서 working-set118,013,952→118,022,144B/private98,721,792→98,729,984B/allocated19,001,922→19,004,154B, committed35,188,736B/reserved46,583,808B였다. GUI drop 뒤111,648,768B/92,114,944B/13,401,613B, committed32,034,816B/reserved43,368,448B, 실제 process exit0이다. B는 다른 moving workload라 peak 감소율을 인과 개선으로 주장하지 않는다.

C 녹화12,568,058B는8fps/1922×1140, reported/decoded289/289이고 seek0/144/288이 순차 픽셀과 같았다. 독립 oracle exit0/stderr0B다. 원래 config/assets를 바꾸지 않았고 게임 캐릭터는 살아 있는 상태로 Act1에 복귀·저장 종료했다. 이후 같은 C 빌드의 정상 새 방/이동·녹화 감독 검사를 시작했으며 그 결과는 별도다.

## Checkpoint C — Moving New Room With Recording

Production239파일 집계 SHA256은 `edc8331eae25ba8cdf30724aea9a04b722dfd5dded7bfbf7fdaf26258c16b807`, EXE는 `c9391a7a87db7002b42c25e5d17754b494aabe9b9ece8e231fcfcbc805f41af0`다. manifest659파일23,824,299B의 별도 package로 실행했다. 자동 버프는 보고상 `dispatched_unconfirmed`, entry는 `arcane_hub_observed`다. 텔레포트·Nova/서모너 단상 도달과 살아 있는 캐릭터/용병을 감독 관찰했으며203화면 모두 actual SCREEN이다.

| Source→stage | p50 | p95 | p99 | Max | ≤50ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Near | 74.314 | 119.557 | 134.513 | 139.916 | 2/203 |
| Hover | 88.988 | 122.587 | 136.585 | 147.684 | 2/203 |
| Processed | 136.295 | 178.248 | 289.400 | 311.455 | 0/203 |

local_field=true197행의 processed135.931/169.579/205.897/220.459ms다. local_field=false5행은 stale_hold였고 마지막 platform행은 이 분류 metadata가 없다. mapping_current=true202행, mapping_deferred0행이다. 즉 source 최소100ms mapping 간격은 이 실제 반복에서 계산을 생략시키지 못했다. actions는 teleport85/aim-settling65/Nova24/nearby-hover-probe16/stale5/loot-inspection4/turn-recheck3/platform-hold1이었다. `teleport_aim_settling`이라는 legacy label만으로 제거한50ms timer가 남았다고 해석하지 않는다.

| 명시 source가 연결된 C 입력 | 건수 | Capture→return p50/p95/p99/max ms | Ready→return p50/p95/p99/max ms |
| --- | ---: | --- | --- |
| 새 F2 | 107 | 171.668/205.847/212.586/227.015 | 27.090/58.546/60.443/60.656 |
| 반복 F2 | 12 | 233.914/259.864/259.864/259.864 | 89.507/133.027/133.027/133.027 |
| 새 F4 | 21 | 160.626/201.181/215.436/215.436 | 11.836/58.368/59.438/59.438 |
| 연결된 move | 81 | 142.481/171.659/187.338/187.338 | 2.533/14.393/16.836/16.836 |

반복 F4는0, 총sent627 중 연결221/미연결406(move263/key-up128/press15)이고 최종 held set은 비었다. B의새F4 p50158.1ms와 비교해 공격 입력이 빨라졌다고 주장하지 않는다. B/C는 같은 방·동일 몹 장면이 아니며 near/processed 감소도 정확한 인과 절감률로 표현하지 않는다.50ms는 미달이다.

navigation은 `summoner_platform_observed`/`journal_and_surrounding_runes`였다. encounter는 `kill_unconfirmed`, 한 관찰에서 `target_floor_unverified`라 추가 nova/approach0이고 처치 확인이나 key 획득은 없다. 최종 status=failed/acceptance_room=false, 오류 `window geometry changed after capture`, segments_completed0/phases0, evidence complete/field retained=true다. UI Stop 이후 오류 원인을 확정하지 않았으며 보호를 낮추지 않았다. 감독 플랫폼 도달과 전체 정상 방 마감은 다르다.

85개2초 process-wide 표본 peak working-set404,951,040B/private428,675,072B/default-heap allocated347,482,170B/committed361,263,104B/reserved368,222,208B다. 마지막 timed165.777–169.789초는 working-set119,578,624B/private99,762,176B/allocated19,564,384–19,567,120B, committed50,302,976B/reserved63,152,128B였다. GUI drop 뒤110,346,240B/87,744,512B/13,424,507B, committed47,149,056B/reserved59,965,440B이고 process exit0다. B와 실행 범위·장면이 달라 peak 감소의 원인별 귀속은 미확정이다.

녹화61,162,524B는 reported/decoded767/767,1922×1140/8fps, seek0/383/766이 순차 decode와 같았고 oracle exit0/stderr0B다. recorder 실제 stop·file finalize·join 후 GUI를 닫았다. 이후 Primary가 TP로 수동 복귀한 것은 기존 운영 계약 위반이며 정상 복구/방 성공이 아니다. [RUN의 복구 위반 기록](../run/run-20261024-02-cpu-realtime-control.md#감독-복구-위반과-재개-경계)이 사용자 정정/현재 상태를 소유한다. 사용자 입력 감지 후 추가 게임 입력을 중단했고 Act1 저장 종료를 완료했다고 주장하지 않는다.

## Validation And Remaining Work

checkpoint A 전체 portable322 PASS/6 ignored와 native4 PASS, fmt/check/Clippy가 통과했다. B targeted132 PASS/5 ignored 및 all-target Clippy가 통과했다. C는 fmt/check/all-target Clippy와 전체 static-CRT release324 PASS/0 fail/6 ignored(9분50.57초), 명시 native4 PASS를 통과했고 경고가 없었다. C의50ms/전체 방은 미달이다. 다음 compute 분할 batch와 최종 package는 별도로 검증하며 C의 PASS를 새 코드에 자동 이월하지 않는다.

`assets/regions` runtime137파일1,780,641B는 필수다. source344파일421,556,772B 중 제외된207개419,776,131B는 참고 자료다. 전체 이전474,641,839B 배포가23,702,450B로 줄었던 것은 reference281개 약451MB를 제외한 결과이며 실제 runtime 자산/필수 loading·exit reference는 남겼다. B 독립 CWD default config/HUD/typed loader/dry/region 누락 실패·복원 smoke는 통과했다. 개별 최소 crop 집합이라는 주장은 아니다.

## Checkpoints D/E — Priority And Current Preparation

D는 총8개 계산 예산을 foreground6/background2로 나누고 nested scope를 진입 pool에 남겼다. 외부 family lease가 두 pool의 실제 종료를 소유한다. blocked background 중 foreground 수행·panic 재사용·마지막 참조 해제 검사와 targeted120 PASS/5 ignored가 통과했다. 동일 고정 입력 C→D p50(ms)은 panel6.2537→6.2092, near12.5043→11.9021, path17.9379→18.2481, ORB15.268→15.3376, stored36.1352→35.869였다. 직접 observer 측정이므로 분할의 게임 효과를 입증하지 않는다.

E는 같은 불변 Arc의 panel task1개를 HUD와 겹치고 prepare에서 실제 join한다. frame mismatch/조기 HUD 실패/미소비 오류 회수 검사를 포함한 targeted117 PASS/5 ignored 및 check/Clippy/build가 통과했다. 고정30/3 native near의 serial→parallel p50/p95(ms)는 broad 병행 녹화OFF31.6828/36.6198→27.8306/31.8578, 녹화ON31.0893/35.006→28.3102/32.3498이다. 불변 frame 할당 후 시작이며 capture/input은 제외한다. 입력0/소비 참조 해제/gray·floor 지문 일치, 실제 recorder19,859.3821ms·9,782,446B/stop→join5.9409ms를 확인했다.

E 새 방 녹화ON의254 actual SCREEN 행은 source→near p50/p95/p99/max65.990/85.454/112.564/125.768ms(≤50ms5/254), processed120.125/162.242/332.439/406.219ms였다. capture30.760/51.028/79.460/91.666ms이며 HUD/panel overlap은254행 모두, p503.942ms다. 새F2 source→return175.532/217.396/225.624/225.624ms·ready→return33.912/49.796/56.972/56.972ms, 새F4는105.422/191.783/191.783/191.783ms·7.090/57.855/57.855/57.855ms였다. 방과 적 분포가 B/C와 달라 인과 절감률로 표현하지 않는다.

E 상태는 interrupted/errors=[control capture stopped], entry=arcane_hub_observed, segments0, encounter=kill_unconfirmed, acceptance=false다. 단상 도달은 전체 성공이 아니다. F10을 game foreground에서 먼저 보낸 이 경우 종전 geometry 오류가 없었지만 모든 원인을 확정하지 않는다. 총sent1327/명시 연결201/미연결1126, 최종 held empty, recorder 종료/GUI exit0을 확인했다.2198/2198frame8fps1922×1140·seek0/1099/2197 독립 decode가 통과했고 stderr0B였다. sampled peak WS543,035,392B/private522,530,816B/allocated435,638,014B, GUI drop 뒤112,873,472B/88,383,488B/13,443,962B다. C 대비 peak 개선을 입증하지 않았다.

E return-only는 기존 north→central→Act1→save/exit runtime으로 `north_room_closed`, errors=[], central_waypoint_and_player_settled, act1_then_character_select_observed를 기록하고 선택 화면/GUI exit0을 확인했다. 새 방·TP·수동 마을 보행은 없고 fresh 성공 수와 별개다.

## Checkpoint F — Actual Clock And DIB Ownership

pointer 확인 중12ms가 흐르면 이후 eligibility가 오래된 now로 계산돼0ms settling도 다음 service로 미뤄지는 결함을 수정했다. 새 회귀는 수정 전 실제0down/기대1로 실패(exit101), 수정 뒤 통과했다. 현재 clock을 다시 읽고 원래 observation ready는 보존한다. 양수50ms 대기·반복·focus/epoch/만료·포인터 확인은 유지한다. GDI는 호출 소유의 초기화된 top-down DIB를 GdiFlush/선택 해제 뒤 읽고 소유 BGR로 복사한다. 별도8,764,320B BGRA Vec/GetDIBits 복사를 제거했으며 raw view나 handle을 스레드 밖으로 넘기지 않는다. 전체 peak 감소량과 같은 의미는 아니다.

F targeted32 PASS/1 ignored, native4 PASS, Clippy/build와 독립 CWD default config/HUD/typed asset loader/dry/region 누락 실패·원래 SHA 복원 검사를 통과했다. source 자산 fallback은 없었다. 동일 target 정지 선택 화면의 이전 E EXE→F 30/3 캡처 p50/p95는16.889/32.8832→16.6445/17.6148ms다. 서로 다른 시각의 화면이므로 픽셀 동일한 live 대조군은 아니다. 고정 PNG gray/floor 지문은 같았다. F native parallel 준비의 near p50/p95는 broad 없음24.4123/28.3742ms, broad 있음30.0115/35.4452ms다. capture/input을 제외한다.

F 새 방 녹화ON412행은 모두 actual SCREEN이며 top-level phases는 비어 중복 합산하지 않는다. source→near59.350/72.711/90.226/122.382ms, hover69.417/86.760/103.514/136.907ms, processed139.953/382.199/406.226/445.084ms다. capture24.976/34.412/50.680/75.562ms, near 자체23.942/30.689/38.942/52.106ms였다. local true226/false186, mapping_current412/deferred0이다. terminal 획득 뒤 current full objective 합류 p95267.255ms, stale_hold185행이 생겨 처리 tail이 악화했다. 이 실패를 평균 향상으로 숨기지 않는다.

| F 명시 연결 입력 | 건수 | Source→return p50/p95/p99/max ms | Ready→return p50/p95/p99/max ms |
| --- | ---: | --- | --- |
| 새F2 | 48 | 138.317/177.225/182.421/182.421 | 11.265/26.563/28.873/28.873 |
| 반복F2 | 41 | 191.574/234.697/242.233/242.233 | 80.121/114.404/128.566/128.566 |
| 새F4 | 13 | 139.847/195.876/195.876/195.876 | 13.335/61.259/61.259/61.259 |
| 반복F4 | 7 | 176.024/198.624/198.624/198.624 | 59.667/83.593/83.593/83.593 |
| Move | 102 | 122.783/155.194/173.492/174.093 | 2.302/9.480/15.041/16.162 |

총sent307/명시 연결211/미연결96이며 최종 held set empty다. 새로운 keydown 지연은 줄었지만 F4 개선이나50ms를 입증하지 않는다. status=interrupted/errors=[observation submission interrupted], segments1/4·same-process continuation1, acceptance=false다. 늦은 판독은 안전 hold했으며 만료를 늘리지 않았다.104 timed process 표본의 peak WS432,427,008B/private411,152,384B/allocated316,824,614B/committed341,712,896B/reserved358,182,912B, GUI drop 뒤112,553,984B/94,203,904B/13,581,863B/57,118,720B/93,102,080B다. 다른 방 E와의 원인별 감소율은 미확정이다. 최종 idle3표본 allocated약19.62MB와 drop 뒤13.58MB는 live heap 감소 증거이며 private 전체를 누수로 해석하지 않는다.

F AVI는1289/1289frame, E/F 읽기 전용 AVI는 각각59/59frame, 모두8fps1922×1140이며 앞/중간/끝 seek가 순차 decode와 같고 stderr0B다. 독립 decoder는 개발-only다. F 복귀 역시 코드만으로 north_room_closed/errors=[]/central_waypoint_and_player_settled/act1_then_character_select_observed, 선택 화면과 GUI exit0을 확인했다.

이 단계의 원시 수치/보고는 당시 `recordings/benchmarks/20261024-cpu-realtime/`가 소유했다. E/F JSON은 trace 보존용이었으며 과거 모든 PNG/AVI의 무기한 보존 약속은 아니다. 당시 목표 미달 원인에 따라 G의 현재 확인을 foreground로 분류하고 F 검사를 새 코드에 이월하지 않았다. 이후 최종 검증 결과와 아래 보존 상태가 현재 참조 범위를 정한다.

## Checkpoint G — Current Confirmations In Foreground

현재 확인/알고리즘/만료를 유지하면서 최초·force-current·양성 재확인만 foreground host/계산으로 옮겼다. background host 차단 중 current 확인 수행 검사 포함 lib101 PASS/5 ignored, Clippy/build 및 independent-CWD loader/region 누락 거부·복원을 통과했다. EXE SHA256은 `c4b063c9afab74321eb95e8db1c6a7d4d93dddd6749a1350b72760714c655732`, package659파일23,901,099B다. 링크 중 실행돼 binary/컴파일 경쟁을 배제하지 못한 첫 도구 결과는 폐기하고 실제 build exit0 뒤 다시 측정했다.

고정 PNG30/3·recorder ON의 native parallel/broad near는33.0639/36.659/37.7812/37.7812ms, hover43.5915/48.3754/49.0384/49.0384ms다. capture/input 제외, 입력0/참조 해제/gray·floor 지문 동일이며 동시 녹화22,478.1888ms/11,390,290B/stop→join8.0764ms/running=false였다. ORB pair19.6204/21.3497ms와 stored41.081/44.8577ms가 E/F보다 빠르다고 주장하지 않는다.

녹화ON 새 방의 report에는 끝 도달 phase247행과 이후 top navigation147행이 서로 다른 capture sequence 범위로 존재한다. 중복 없이394행을 합산했으며 전부 actual SCREEN이다. phase는 wing_end_observed/current_terminal_landmarks_and_player_settled/nonboss_chest_platform, 이후는 field_paused/central_reacquisition_required다. 중앙WP의 실제 화면과 자동 북쪽 끝 도달을 보았지만 최종 마감 전에 F10으로 중단했으므로 status=interrupted/errors=[control capture stopped], acceptance=false, report-level segments0/4, continuation0이다. fresh 정상 마감이나 boss/key 성공으로 세지 않는다.

| G source→단계394행 | p50 | p95 | p99 | Max | ≤50ms |
| --- | ---: | ---: | ---: | ---: | --- |
| Near | 58.280 | 72.101 | 84.457 | 91.028 | 43/394 |
| Hover | 68.846 | 85.612 | 97.117 | 104.198 | 26/394 |
| Processed | 116.987 | 182.293 | 254.265 | 312.797 | 0/394 |

capture25.877/33.772/48.820/64.084ms, near 자체21.949/28.839/33.792/48.932ms, objective 합류0.001/36.374/103.711/121.597ms다. local true225/false169, mapping current392/deferred2, stale_hold5행이다. F의185 stale hold/current objective p95267.255ms보다 줄었지만 서로 다른 방이어서 정확한 인과 개선율은 아니다. phase/current nav의 processed p50/p95는115.910/225.961과118.419/147.464ms다. 전체50ms는 미달이다.

| G 명시 연결 입력 | 건수 | Source→return p50/p95/p99/max ms | Ready→return p50/p95/p99/max ms |
| --- | ---: | --- | --- |
| 새F2 | 66 | 138.095/172.567/258.680/258.680 | 12.038/26.609/30.436/30.436 |
| 반복F2 | 69 | 215.927/235.771/246.151/246.151 | 93.703/119.533/130.144/130.144 |
| 새F4 | 10 | 92.061/244.949/244.949/244.949 | 8.191/58.740/58.740/58.740 |
| 반복F4 | 14 | 177.785/223.361/223.361/223.361 | 53.721/156.958/156.958/156.958 |
| Move | 161 | 120.244/157.905/164.282/244.207 | 2.138/8.239/17.903/18.703 |

총sent1189/명시 연결320/미연결869, keydown159/up76/move924/press30이며 최종 held set empty다. source→return258.680ms 같은 값에는 오래된 원래 관찰을 보존한 입력 확인/반환 시간이 포함되며 게임 효과나 해당 순간의 새 입력 승인 시각으로 바꾸지 않는다.152 timed process 표본 peak WS554,102,784B/private525,078,528B/allocated428,263,729B/committed452,722,688B/reserved477,691,904B였다. GUI drop 뒤113,623,040B/90,386,432B/13,441,977B/57,643,008B/93,122,560B, 실제 exit0다. 전체 peak 메모리 목표는 개선됐다고 말할 수 없으며 현재 작업의 큰 임시 할당이 남은 최적화 대상이다.

G return-only는 errors=[]/north_room_closed/central_waypoint_and_player_settled/act1_then_character_select_observed를 기록했다. 새 선택 화면과 GUI exit0을 확인했고 TP/수동 마을 보행/새 방 생성은 없다. 원래 fresh run의 중앙 재획득 실패를 이 복구 성공으로 덮지 않는다. 이어 H는 발밑/먼 조준점 hazard 합류를 나누어 근처 판단을 우선하며 인식 ROI·최종 이동 검사를 유지하는 중이다.

## Rejected H — Earlier Ground-Only Hazard Join

H는 발밑/조준점 source·sequence/context를 한 번 준비하고 발밑과 target panel을 먼저 소비, landing을 hover 합류 전에 회수하는 후보였다. positive 두 ROI의 최종 결과/순서/stamp·동일 ROI 중복 제거·늦은 age·미소비 오류/실제 join/참조 해제 검사 포함 targeted112 PASS/5 ignored와 Clippy/build/portable loader가 통과했다. 하지만 같은 고정 PNG30/3/broad/recorder ON near p50/p9533.1326/35.4611ms는 G33.0639/36.659ms 대비 유의한 이득을 입증하지 못했다. 측정한 화질·gray/floor 지문은 같았다.

다른 방 녹화ON의876개 고유 SCREEN 행에서 source→ground-only near p50/p95/p99/max61.0/79.7/93.5/110.5ms(≤50ms41/876), hover73.2/94.8/111.2/141.1ms, processed113.9/143.0/160.5/230.0ms였다. H near는 G의 두 ROI 합류보다 적은 작업을 포함하므로 동등 workload 개선으로 보지 않는다. ground→near 자체22.1/28.0/46.8/54.7ms, 전투 선점29행 source→preflight83.1/115.4/123.2/123.2ms다. 새F2 155건 source→return138.3/178.9/236.2/238.4ms·ready13.5/27.7/30.5/31.2ms, 새F4 31건123.1/199.1/199.7/199.7ms·13.0/60.0/60.2/60.2ms였다. 실제50ms나 새 이동 입력 개선을 입증하지 못했다.

H report는 interrupted/errors=[continuous navigation interrupted or unfocused], segments2/4/continuation2, acceptance=false이며 최종 held set empty, evidence complete, recorder close/GUI exit0다.103 timed 표본 peak WS600,088,576B/private581,124,096B/allocated484,487,683B/committed510,926,848B/reserved529,104,896B, GUI drop 뒤103,268,352B/95,731,712B/13,545,726B/90,779,648B/175,996,928B다. 다른 장면 G와의 인과 메모리 증가로 단정하지 않지만 개선 근거도 없다. 복구는 code-only north_room_closed/errors=[]/central_waypoint_and_player_settled/act1_then_character_select_observed와 선택 화면/GUI exit0를 확인했다.

검증한 이득 없이 상태·타입·합류 복잡성이 늘어 H는 제거했다. 채택 코드는 G이며 H 전용 source/test를 남기지 않는다. H JSON은 폐기 판단 감사용이고 runtime/성능 성공 증거가 아니다. H 제거 뒤 production239파일 집계 SHA256은4ae2e862adbc1374b626c8df27bd5841f9e0d191109314d774b863c93bfdb3ac이며 위 Boundary의 최종 회귀/새 링크 검증을 통과했다.

## Final Retention And Remaining Route

최종 회귀·새 링크/GUI·패키지 검증은 위 Boundary의 수치대로 완료했다. G 영상1754/1754frame의0/877/1753을 실제로 열어 캐릭터 선택과 게임·HUD가 보이고 검은 game body가 아님을 표본 확인했다. 전체 순차 decode와 seek 일치는 별도 oracle 결과다. 화질의 전체 프레임 무결점이나 화면 효과 시각을 이3장으로 보증하지 않는다.

숫자/원래 source trace·유효 설정5 JSON·검사 로그는 `recordings/benchmarks/20261024-cpu-realtime/`, 아직 해결할 중앙 재획득의 실제 report/PNG/AVI는 `recordings/summoner/evidence/cpu-realtime-20261024/g-central-reacquisition/`에 보존했다. 설정 snapshot의 녹화 directory만 완료된 임시 경로에서 해당 영구 검증 owner로 바꿨고 원본 config는 그대로다. 원래 source 입력 PNG와 hash를 보존 안내에 연결했다. E/F/H media의 역사적 참조는 현재 보존 약속이 아니며 G의 선택한 완전한 증거와 구분한다.

2026-10-30 보존 갱신: 위 두 원시 owner는 후속 Rust 개발에서의 비교/중앙 진단 목적을 마친 뒤 정리했다. 이 문서의 요약 수치·검사 결과·한계는 유지하고, 재구성에 필요한1523B PE metadata만 위 receipt로 승격해 바이트/SHA 일치를 확인했다. 이전 PNG/AVI/설정 snapshot/전체 trace는 더 이상 현재 보존 대상으로 안내하지 않는다. 중앙의 미해결 부분은 [현재 Rust 개발 run](../run/run-20261024-03-rust-summoner-development.md)과 새로 보존한 공용 region 회귀가 소유하며, 새 실제 성공을 추가하지 않았다. 사용자 config와 기본 G 배포는 삭제하지 않았다.

검증 종료 후 활성 writer가 없음을 확인하고 중간 후보6개1,100,393,914B, 남은 G 후보120,229,687B, task scratch/검증된 이전 패키지 backup997파일897,120,867B를 제거했다. source/Python/config/assets와 표준 Cargo cache는 지우지 않았다. 최종 배포는659파일23,901,410B이며 진단 자료가 섞이지 않았다. active task scratch는 남지 않는다. 현재 기능은 안전 회귀/단독 실행을 통과한 CPU 개선 checkpoint이고 목표 전체 완료는 아니다.

다음 실행은 현재 전체 planner 합류 전에 다음 화면을 볼 수 있는 독립 fast 판단 cadence와 단일 입력 소유 계약, 큰 호출 소유 FFT/matcher 임시 메모리 축소로 돌아간다. 읽기→판독/실제 입력50ms와 표시 변화→효과는 별도로 검증한다. 현행250ms/focus/geometry/Stop/unknown/포인터 확인/실제 join을 완화하지 않는다. 중앙 재획득·협곡/전체 방 수용과 peak 개선의 남은 문제를 숨기지 않으며 WGC/GPU는 계속 보류한다.
