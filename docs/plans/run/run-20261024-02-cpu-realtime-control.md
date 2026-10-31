# RUN-20261024-02: CPU 중심 판단·입력 실행

## Metadata
- Status: `active`; Attempt: `1`; Feature: [FEAT-0030](../feature/feat-0030-cpu-realtime-control.md); Spec: [SPEC-0030](../spec/spec-0030-cpu-realtime-control.md); Date: `2026-10-24`.
- Profile: `fullstack-product`; Evaluators: Contract, Functional; Plan: [REFACTOR-0004](../refactoring/refactor-0004-realtime-decision-latency.md).

## Orchestrator Decision

최신 사용자 목표 정정(docs-content):50ms는 필수 합격선이 아니라 기능·판독 품질·안전성을 유지하며 효율을 최대한 높이기 위한 지향 목표다. 이번 문서 갱신의 Active Spec은 [SPEC-0030 Documentation Contract](../spec/spec-0030-cpu-realtime-control.md#documentation-contract)다. 종전의 수치 미달=실패 해석을 정정하고 측정/실패 이력은 보존했다. 코드/EXE/게임은 변경하지 않았다. 다음 작업은 확인된 불필요 비용을 줄이는 것이며50ms를 맞추기 위한 기능 축소가 아니다.

현재 채택 checkpoint는 G이며 H는 성능 이득 미확인으로 제거했다. 최종 source/EXE/패키지 지문과331 release+4 native PASS, 실제 독립 GUI/loader·누락 파일 검사는 [EVAL-0033](../evaluation/eval-0033-cpu-realtime-control.md)이 소유한다. 현재 패키지는 `dist/d2bot-windows-x64/`에 반영했다.50ms 초과만으로 실패/미완료를 판정하지 않는다. 다만 독립 판단 cadence와 큰 임시 계산 메모리 등 확인된 개선 과제 및 전체 게임 수용이 남아 feature를 완료하지 않았다. 다음 경계는 전체 planner 합류와 독립된 최신 fast 판단 cadence/단일 입력 소유, 큰 임시 계산 메모리의 축소다. WGC/GPU·만료 완화·새 codec 의존성은 도입하지 않는다. 마지막 실제 게임 상태는 코드 복귀 후 Act1 저장 종료/선택 화면이며 모든 bot GUI·녹화 process는 종료했다. 캐릭터 복구 확인을 다시 요구하지 않는다.

이 시도의 임시 파일과 중간 package는 검증 로그·필요한 재획득 증거를 각 영구 owner로 옮긴 뒤 정리했다. 과거 절의 scratch/후보 경로는 당시 실행 위치이며 현재 참조 대상이 아니다. 실제 현재 source·EXE·배포 및 보존 owner는 EVAL-0033을 따른다.

사용자가 CPU 구조 계획과50ms 대기의 안전한 개선, 녹화 병행 실게임·전후 비교를 승인했다. 이번 loop는 FEAT-0030이며 이전 FEAT-0029의92ee EXE/7d56 source/실제 결과를 baseline으로 freeze했다. 초기 원인 근거는 named read-only worker3개가 입력 출처/CPU 재사용/배포 경로에서 제공했고 Primary가 구조·안전·승인과 구현을 소유한다. 새 의존성/WGC/GPU/게임 내부 접근은 도입하지 않는다.

## Baseline And First Decisions

현재공용source/EXE지문은기준과일치하고 task-owned scratch에원본EXE/개발benchmark를보존했다. 입력은 같은 F2 hold의 방향 변경 시 keyup하지 않지만50ms가 새/반복 keydown을 늦춘다. Python의 이전 pointer 소비 방지 주석에서 왔으며 실제 게임에서50ms가 필수라는 검증은 없다. Native backend는 이미 실제 좌표 확인과 직전 focus/epoch/만료를 수행하므로 연속 입력의 중복 고정 대기 개선을 fake/실게임으로 확인한다. 먼저 계측만 넣어 기준 실행을 고정한다.

assets/regions는 필수이며 현재 배포에137파일/1,780,641B가 있다. 빠진207개는 reference 자료이고 runtime137개의 source/package SHA는 모두 같다. Production은 compile-time source fallback 없이 CWD root를 쓰므로 새 독립 디렉터리의 실행/원본 폴더 비의존도 검증한다. 현재 Python/config/assets와 이전 최종배포는변경하지않았다. 새 PASS/50ms 달성/실게임 결과는 미리 기록하지 않는다.

## Attempt 1 — 계측과 CPU 구조 batch

후속 사용자 steering: 안전성을 지키면서 구조 개선으로 실제 자원 읽기→판별을 먼저 최단화한다. 기존 타이밍/threshold/행동의 재적응은 후속이며 완전 원본 동작 일치를 선행 gate로 두지 않는다. 현재 batch와 안전 회귀를 마친 측정 기준 위에서 추가 phase/ROI/알고리즘 변경을 판단한다. 녹화·감독 입력 증거는 계속 수집하되 전체 방 성공을 구조 개선 채택의 선행 조건으로 삼지 않는다.

기존 동작에 source/단계/backend 반환 시각만 더한 portable EXE는 SHA256 `1e1653fb38ac876356ba7957c0f4b3fcf14d2fbdbbd000f73ef038f2d0ebe2b3`이며 resource-probe는 `6281966c8b7bc7ec15d35a8b1d4c1cadea6a979b0b03b43cd39734e0b071cf40`이다. 28개 입력/관찰 targeted release 회귀와 check가 통과했다. baseline은 기존50ms 설정, Flash Act1 저장 종료 상태에서 새 방·자동 버프·녹화ON을 사용했다. 8개 continuous 행 모두 `outbound_central_reacquire_hold`, F2/F4 dispatch0/0, 오류 `outbound_requires_central_waypoint`다. 전체 처리 p50/p95/max192.2059/207.3143/207.3143ms; 이 수치는 이동 입력 반응을 입증하지 않는다. 이후 paused 감시 보고에는 `window geometry changed after capture`도 남았다. UI 전환과의 인과는 확인하지 않았다.

baseline의 단계 p50(ms)은 acquired→capture완료29.5951, HUD3.5089, panels18.8536, ground2.0052, threats17.6771, plan-submit43.4289, objective-join61.8196, path-join0.0040, 나머지processed13.8399다. 병렬 합류의 시간 차이이며 각 recognizer의 독립 실행 시간으로 해석하지 않는다. 자체 68개 자원 표본 peak는 working-set292,454,400B/private279,822,336B/heap-allocated144,293,083B, GUI drop 뒤137,945,088B/125,165,568B/13,337,327B다. 2초 표본 peak라 순간 최대나 원인별 메모리를 보장하지 않는다. 녹화 마감/실제GUI종료exit0을 확인했고 Flash를 Act1으로 돌려 저장 종료했다.

같은 첫/마지막 frame은 개발용 Python oracle에서도 hub=None였다. 두 blue pair는 현재 존재하지만 첫 frame의 바닥 문양은 right0.8308 한 부위만0.8을 넘고 나머지0.1638–0.5958이다. threshold/두 부위 조건을 낮추지 않았다. 같은 scene의 추가 판독으로 개선되는지는 이후 게임에서 확인한다.

구현 batch는 (1) 전체 target 좌표의 같은 Area 가중치/누적/반올림으로 panel 두 ROI만 계산하고 GrayMatcher 통계 공유, (2) actual pointer 확인을 유지하고 연속입력의 추가50ms gate만0, (3) slow 관측기별 실행중1개/독립2-worker pool과 원래 source의 양성→현재 frame 재확인, terminal 획득 뒤 현재 전체 확인, (4) 60fps 상한 producer1/최신 대기1개의 capture owner와 actual source 시각, 250ms 소비중단 시 idle, 단발 grab은 요청 후 시작 frame을 요구, (5) package marker가 있는 EXE는 sibling assets/default config를 선택하고 명시 config는 caller 기준 유지다. source buffer를 수정/오래된 음성을 현재 clear로 relabel하지 않는다. cleanup 도중 보고 배열 오류가 나더라도 stop/cleanup을 건너뛰지 않게 했다. 구체 계약은 SPEC-0030에 구현 전에 기록했다.

첫 CPU targeted22개 회귀는 통과했다. 통합 라이브러리에서는 계측 필드 추가로 Python oracle 객체 key 수가 달라 2건 실패했으며, 테스트에서 새 계측만 검사·분리하고 행동 필드를 계속 비교하도록 수정했다. 그 수정 중 null 값의 mutable indexing이 객체를 만드는 시험 보정 오류도 바로잡았다. 재검사 라이브러리93 PASS/5 ignored, app1/input16/latest2/ROI2 PASS를 확인했다. portable-root 테스트의 경로 문자열 비교 실패는 Windows extended 경로를 canonicalize해 비교하도록 수정했으며 재검사가 필요하다. Clippy의 FrameStages 초기화 지적을 수정했다. 이후 source 변경을 포함한 최종 전체/portable/native와 게임 수용은 아직 미확정이다.

### Checkpoint A — 동등한 판독 경로 단축

직접 uint8 correlation의 u32 SIMD 시도는 panel p5015.5ms로 ROI-only9.9ms보다 느려 폐기했다. 최종 i16 lane 내적은 overflow를 막는16,384-sample block/u64 누적으로 바꿨다. checkpoint A portable EXE는 `93bf1029cbbaf275f582e97dde7cc4d85dca28522acaf6f10844f53fc1d7bc13`, package659파일23,811,287B다. fmt/check/Clippy와 전체 portable322 PASS/6 ignored, 명시 native4 PASS를 확인했다. portable 경로 회귀도 canonicalize 수정 뒤 통과했다.

### Checkpoint B — local phase와 현재 근처 위험 선점

미획득 outbound의 mapping은 source 기준 최소100ms 간격으로 제한하고 미관측 motion은 `mapping_deferred`/displacement=None/position_current=false로 보고한다. 이때 오래된 graph feature/port를 현재 aim으로 쓰지 않고 현재 floor/path만 선택한다. 현재 근처 위험은 hover 합류 전에 안전 검사를 거쳐 전투할 수 있고 이동은 hover를 합류한다. 테스트의 field_seconds=0.5는 기존1..60 검증에 거부되어1로 고쳤으며 유효 범위는 유지했다. targeted132 PASS/5 ignored와 all-target Clippy를 확인했다.

checkpoint B EXE `34be847a246ffc3e19d99e6963c386c864bad18395417926b78a20739addafbb`, manifest659파일23,822,551B로 독립 package를 만들었다. 관련 파일을 옮겨 부정 검사를 하는 동안 정확한 원래 SHA로 복원했다. 다른 CWD에서 default config/직접 HUD/전체 typed asset loader/dry run을 실행했고 region manifest 제거는 source fallback 없이 실패했다. 입력/캡처 없는 로더 smoke와 실제 게임 수용은 별개다.

같은1922x1140 저장 PNG30회/3warmup, 녹화ON의 p50(ms)은 baseline→B panel19.3399→6.04, near17.55→13.40, hover20.81→21.00, path18.59→18.88, ORB pair19.37→16.09, stored-packet40.8728→37.05, encode2.16→2.12였다. ORB는 이 batch에서 바꾸지 않아 인과 개선으로 주장하지 않는다. stored-packet은 normalize/gray/HUD/path/ORB pair 묶음이며 main 전체 시간이 아니다. 이 도구의 capture_screen은 강제SCREEN이고 당시 live는AUTO였다.

감독 새 방/자동 버프/녹화ON에서 Flash의 텔레포트 방향 변경·Nova와 비보스 상자 플랫폼 도달, 살아 있는 캐릭터와 용병을 관찰했다. 보고646개 continuous 행은 capture→near p50/p95/p99/max103.828/137.643/163.980/205.649ms, capture→processed168.647/270.140/296.417/366.660ms였다. local589행의 processed166.126/207.713/249.113/269.144ms, deferred136행, stale_hold50행이다. near≤50ms는0/646이며 목표 미달이다. actual 단계 p50은 capture45.664ms, capture완료→소비 요청15.483ms, ground→near27.526ms였다. 이 측정 때문에 계속60fps 캡처/동시 broad2개를 최종 구조로 채택하지 않는다.

실제 input metadata가 있는 F2 새keydown316건의 capture→backend-return p50/p95/p99/max197.5/236.5/253.2/261.5ms, F4 새keydown23건은158.1/216.2/240.2/240.2ms다. decision-ready→return은 각각22.7/54.4/60.6/76.8,9.8/17.9/48.4/48.4ms다. F2 반복14/F4반복3은 별도이며, 총sent1,517건 중 연결581건·미연결936건이다. 근처 frame에 연결한 초기 scout 추정값은 폐기했고 명시 observation만 사용했다. 전체 keydown/up 추적의 최종 held set은 비었다.

224개2초 resource 표본의 peak working-set539,500,544B/private533,262,336B/heap-allocated427,004,643B, native GUI drop 뒤101,924,864B/93,483,008B/13,551,204B다. baseline은 중앙 hold-only여서 peak의 직접 인과 비교는 할 수 없다. heap committed76,406,784B/reserved142,848,000B는 live allocation과 다르다. recorder 실제 stop/파일마감, GUI exit0을 확인했다. AVI149,411,888B는 독립 decode 대기다. 마지막 보고는 `window geometry changed after capture`로 failed/acceptance_room=false이며 전체 방·boss/key 성공은 입증하지 않는다. 플랫폼 도달과 해당 guard 오류는 별도 관찰이다.

### Checkpoint C — 요청 캡처와 CPU 경쟁 축소

B 근거를 SPEC에 먼저 기록한 뒤 캡처 producer1/대기1을 소비 요청 방식으로 바꿨다. 초기1장 외에는 요청 없는 캡처를 만들지 않고, focused continuous AUTO named 경로만 SCREEN을 선택하며 명시 WINDOW·녹화·preview 선택은 유지한다. actual backend를 각 frame stage에 기록하고 broad observer pool은1worker/queue2로 제한한다. 중단/지연 capture의 실제 close·join 재시도, 불변 consumer frame·원래 시각, 요청 없는 대기, explicit backend 검사를 추가했다. 현재 검증 중이며 성능·실게임 결과는 아직 없다.

C all-target check/Clippy와 targeted release가 통과하고 portable EXE를 링크했다. 첫 resume resource-probe는 개발 helper가 runtime 소유의 output directory를 먼저 만들어 OS183으로 거부됐고 native 입력 초기화에 도달하지 않았다. helper의 중복 directory 생성만 제거해 다시 빌드했다. production EXE/기존 RunFiles의 충돌 거부 계약은 유지했다. B와 baseline AVI는 개발-only 독립 decoder에서617/617·2058/2058frame,1922×1140/8fps와 앞·중간·끝 seek가 모두 일치했고 stderr0B다.

### 감독 복구 위반과 재개 경계

Primary가 B 이후 비보스 플랫폼 및 C 서모너 단상 검사 뒤 TP 키4를 수동 사용하고 Act2에서 수동 보행했다. 기존 `docs/features/summoner-run.md`의2026-10-18 TP 금지와 일지/협곡 또는 north return 복구 계약을 지키지 못한 실행 위반이다. B/C의 정상 방 마감이나 승인된 자동 복구 성공으로 세지 않는다. 두루마리 소비를 숨기지 않으며 원본 config를 변경하거나 임의 보급/구매로 보완하지 않았다. C의 첫 플랫폼 반복을 끝낸 뒤 Act1 복귀/저장 종료는 실제 확인했지만 그 수동 복구 역시 성공 수에서 제외한다.

최종 C 새 방 검사를 중단/녹화 마감·GUI exit0한 뒤 Act2 마을에서 사용자 입력이 감지되어 자동/감독 추가 입력을 중단했다. 사용자는 TP를 금지하고 north return→중앙→Act1 또는 검증된 Act2 포탈→WP 코드를 요구했다. Primary는 기존 Rust north return 진입점을 확인했고 이후 시험 복구에 이를 사용한다. 현재 Act2에 있는 사용자 조작 상태에서 Act1 저장 종료를 완료했다고 주장하지 않는다. 새 Act2 길찾기 기능을 즉석에서 추가한다는 설명은 철회하며 이번 CPU 범위에서는 기존 복귀 코드를 사용한다.

C 새 방203화면의 capture→near p50/p95/p99/max74.314/119.557/134.513/139.916ms, capture→processed136.295/178.248/289.400/311.455ms, near≤50ms2/203이다. F2 새107/반복12, F4 새21/반복0의 최종 held set은 비었다. 현재 서모너 단상 인식 뒤 encounter는 `kill_unconfirmed`/`target_floor_unverified`, 최종 보고는 geometry changed failed이고 전체 마감은 미확인이다. C comparison과 자원/영상 수치는 EVAL-0033에 기록하며 아직50ms는 미달이다. foreground/background compute 분리의 다음 계약을 SPEC에 기록했다. 최종 C 전체 회귀를 freeze해 끝낸 뒤 다음 batch를 구현한다.

### Checkpoint D/E — 계산 자원 분리와 준비 합류

C freeze는 전체324 PASS/6 ignored와 명시 native4 PASS, fmt/check/Clippy를 통과했다. D는 총8개 안에서 foreground6/background2 계산 family를 공유하고 nested scope가 진입 pool에 남도록 바꿨다. 두 pool의 마지막 참조 해제/실제 join, background를 막은 동안 foreground 수행, panic 뒤 재사용을 검사했다. D targeted120 PASS/5 ignored와 Clippy/build를 통과했다. 동일 고정 PNG30/3에서 C→D p50(ms)은 panels6.2537→6.2092, near12.5043→11.9021, path17.9379→18.2481, ORB15.268→15.3376, packet36.1352→35.869였다. 여기서는 직접 관찰기 호출이므로 background 경쟁 분리의 실게임 효과를 입증하지 않는다.

E는 같은 현재 불변 Arc의 panel task1개를 기존 fast host에 먼저 제출해 HUD와 겹치고 prepare에서 실제 join한다. 카메라 authorization/발/현재 HUD/focus·입력 검사는 기존 합류 뒤다. frame mismatch는 오류이며 HUD 조기 실패도 pending 작업과 오류를 회수한다. panel 실제 시작/관측 완료와 HUD/합류 시각을 분리했다. E targeted117 PASS/5 ignored, Clippy/check/build가 통과했다. 개발 benchmark는 serial/parallel 준비·broad 경쟁 유무를 같은 native pipeline으로 측정하는 옵션을 추가했다. 이 옵션은 capture/입력 시간을 포함하지 않으며 inputs0/소비 frame 참조 해제를 검사한다.

사용자가2026-10-24 Act1 저장 종료 완료로 인계했다. 새 현재 선택 화면을 확인하고 검증을 재개한다. 회복 확인은 반복하지 않고 TP/수동 마을 보행을 사용하지 않는다. 새 버전의 감독 게임/종료·배포와50ms는 아직 미확정이다.

E의 실제 native 준비/near/hover만 고정 화면30회/3warmup으로 비교했다. 할당 후 시작이며 capture/input은 포함하지 않는다. broad 관찰이 병행된 상태에서 serial→parallel near p50/p95(ms)는 녹화OFF31.6828/36.6198→27.8306/31.8578, 녹화ON31.0893/35.006→28.3102/32.3498이다. 같은 foreground/background 분할 위에서 panel 준비 방식만 비교한 결과이며 D 분리 자체의 전후 인과는 아니다. 녹화ON의 실제 writer는19,859.3821ms 동안9,782,446B를 쓰고 stop→실제join5.9409ms/running=false였다. 입력0, 소비 frame의 최종 참조 해제를 확인했다. gray/floor 지문은 기존과 같았다. 이 CPU 수치만으로 capture→input50ms를 달성했다고 부르지 않는다.

E 새 방 녹화ON은254개의 actual SCREEN 행에서 source→near p50/p95/p99/max65.990/85.454/112.564/125.768ms, processed120.125/162.242/332.439/406.219ms, near≤50ms5/254였다. panel이 HUD와 겹치는 것을254행 모두에서 확인했고 overlap p503.942ms다. 새F2 source→backend p50175.532ms/ready→backend33.912ms, 새F4는105.422/7.090ms였다. 처치 확인은 없으며 단상 인식 뒤 F10으로 중단해 status=interrupted/errors=[control capture stopped]다. GUI Stop으로 포커스를 옮기기 전에 F10을 보낸 이 경우에는 종전 geometry 오류를 기록하지 않았다. 이1회로 종전 오류의 모든 원인을 확정하지 않는다. 실제 recorder stop/join·GUI exit0, final held set empty와2198/2198frame8fps/1922×1140 독립 decode/seek를 확인했다.

동일 E runtime을 사용한 return-only probe는 return_from=north/resume_arcane/확인한 labels-off/finish_room=true로 시작해 중앙→Act1→저장 종료를 완료했다. 최종 report=north_room_closed/errors=[], navigation=central_waypoint_and_player_settled, room_finish=act1_then_character_select_observed다. 선택 화면을 새로 감독 확인하고 GUI exit0을 확인했다. 이 복구에는 TP·수동 마을 보행·새 방 생성을 쓰지 않았고 정상 새 방 성공 수에서는 제외한다.

### Checkpoint F — 지연된0ms keydown 재현과 GDI byte 소유

E에서 새 aim의 keydown이 여전히 늦는 근거를 조사해 실제 pointer 완료 뒤 aimed_at보다 앞선 원래 now로 eligibility를 계산하는 결함을 찾았다.0ms도 음수 차이<0에 걸려 service까지 미뤄졌다.12ms 흐르는 fake pointer의 새 회귀는 수정 전0down/기대1로 실제 실패(exit101), 현재 clock을 새로 읽는 수정 뒤 통과했다. 양수50ms settling은 실제 pointer 완료 뒤 계속 기다리며 observation 원래 ready 시각은 유지한다. 종전의 ‘첫 down은20ms를 기다리지 않는다’는 설명은 이 경로에 대해 정정했다.

GDI capture는 별도 GetDIBits BGRA Vec 대신 호출 소유의32bit top-down DIB section을 초기화하고, GdiFlush/선택 해제 뒤 읽어 BGR 소유 buffer로 복사하도록 바꿨다. Surface가 raw view/DC/bitmap을 한 호출/한 thread 안에서 소유하고 기존 close/부분실패 경계를 유지한다. SPEC에 앞서 소유/길이/isize/동기화 계약을 기록했다. 현재 Clippy 통과, targeted/native fixture/새 링크 검증 중이며 성능·메모리 개선은 아직 주장하지 않는다.

F는 targeted32 PASS/1 ignored, 명시 native4 PASS, Clippy/build와 portable loader/누락 region 거부·복원을 통과했다. 정지 창30/3 캡처 E→F p50/p95는16.889/32.8832→16.6445/17.6148ms이며 native 준비/판독의 픽셀·floor 지문은 같다. 새 방 녹화ON412화면에서 near p5059.350ms, 새F2 source→return138.317ms/ready→return11.265ms를 기록했지만 전체 처리 p95382.199ms/stale_hold185행으로 목표 접근이 악화했다. 현재 full 확인도 broad host/계산2에 줄 세운 것이 다음 병목이다. 입력 만료를 늘리지 않았다. 녹화1289/1289frame 독립 decode/seek·종료, 최종 held empty와 GUI exit0을 확인했다. 복구는 코드만으로 north→중앙→Act1→저장 종료를 완료했고 errors=[]/선택 화면/GUI exit0이다. 자세한 입력·메모리·실패 수치는 EVAL-0033이 소유한다.

### Checkpoint G — 입력 경로가 기다리는 현재 확인의 우선순위

SPEC에 우선순위 계약을 기록한 뒤 SlowScan의 초기/force-current/양성 재확인은 기존 foreground host에서 수행하고 비차단 탐색만 broad host와 background compute를 사용하도록 분류했다. 현재 full 확인/회수/인식/unknown/freshness는 유지했다. blocked broad host 뒤에서 current 확인이 기다리지 않는 새 검사 포함 lib101 PASS/5 ignored와 Clippy가 통과했다. 최초 benchmark 요청이 링크 완료와 겹쳐 이전 binary/컴파일 경쟁을 배제하지 못했으므로 그 결과는 폐기했다. 실제 build 완료(exit0) 뒤 별도 출력으로 다시 측정하며 컴파일 경쟁 표본을 전후 결과에 쓰지 않는다. 최종 독립 패키지/감독 게임/전체 회귀는 아직 검증 중이다.

G는 package659파일23,901,099B/EXE c4b063c9로 native/stored/녹화 병행 새 방을 검사했다. 고유394행은 phase247+이후navigation147이며 capture sequence가 겹치지 않아 둘 다 포함한다. near p5058.280/p9572.101ms, processed116.987/182.293ms, stale5행이고 nonboss 끝 도달 뒤 중앙 재획득에서 pause했다. fresh 정상 마감은 아니며 F10 중단/recorder close/held empty/GUI exit0과 별도 code-only Act1 저장 종료를 확인했다. peak 메모리는 개선으로 주장하지 않는다. G AVI1754/1754frame/8fps/1922×1140·seek0/877/1753 독립 decode와 stderr0B를 확인했다. 상세 통계는 EVAL-0033에 있다.

### Checkpoint H — 발밑 근접 검사와 먼 착지 검사 분리

근처 판독이 발밑/먼 조준점 두 ROI를 모두 기다리는 경로를 확인했다. 같은 source/sequence/context를 한 번 준비하고 발밑 결과·target panel만 먼저 소비해 기존 guarded 근접 전투로 연결한다. 먼 착지 ROI는 pending1개로 유지하고 hover 합류에서 기존 candidates 순서로 합친 뒤 이동/probe를 판단한다. 각 job은 실제 시작 clock으로 age를 검사해 대기 중 늙은 자료가 현재 음성으로 바뀌지 않는다. 동일 ROI는 중복하지 않는다. 새 positive 두 ROI의 결과/순서/stamp·같은 aim·late age 검사와 abandoned landing 오류/actual join/참조 해제 검사가 debug에서 통과했다. Clippy도 통과했다. release 회귀/링크 뒤 새 near 시각의 좁아진 의미와 전체 이동 입력을 별도로 측정한다. 아직50ms/전체 게임 수용은 미확정이다.

H targeted112 PASS/5 ignored, Clippy/build 및 portable loader 검사는 통과했다. 그러나 고정 PNG/broad/녹화ON near p5033.1326ms로 G33.0639ms와 실질 차이가 없었다. 다른 방876개 SCREEN 표본에서도 ground-only near p5061.0ms로 빨라졌다는 근거가 없고 최종 이동 입력도 새F2 p50138.3ms로 같았다. ROI 합류 범위가 좁아진 지표 자체를 성능 달성으로 세지 않았다. 유지 비용에 비해 검증한 이득이 없어 H 구조/전용 테스트를 제거하고 G로 되돌렸다. 원래 source/ROI/합류 동작을 복원한 production239파일 집계는4ae2e862adbc1374b626c8df27bd5841f9e0d191109314d774b863c93bfdb3ac이다. 복원 뒤 전체 회귀/네이티브/새 링크를 별도로 검증한다.

H도 녹화 마감·held empty·GUI exit0, code-only north return→중앙→Act1→저장 종료(errors=[])와 새 선택 화면/실제GUI exit0을 확인했다. 마지막 사용자 게임 상태는 Act1 저장 종료다. 추가 게임/녹화 process는 없다. 원래 config/assets/Python은 유지한다. 현재50ms와 전체 마감/중앙 재획득·peak 메모리는 미완료이며 최종 수치는 EVAL-0033과 배포 검사 후 정리한다.
