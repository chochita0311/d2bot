# RUN-20261024-01: Rust 성능·자원 원인과 개선

## Metadata
- Status: `active`; Attempt: `1`; Feature: [FEAT-0029](../feature/feat-0029-rust-runtime-efficiency.md); Active Spec: [SPEC-0029](../spec/spec-0029-rust-runtime-efficiency.md).
- Profile: `fullstack-product`; Evaluators: Contract, Functional; Date: `2026-10-24`.
- Evaluation: [EVAL-0032](../evaluation/eval-0032-rust-runtime-efficiency.md); Plan: [REFACTOR-0003](../refactoring/refactor-0003-rust-efficiency.md).

## Orchestrator Decision

최신 사용자 지시로 효율 개선을 활성화했다. 이전 포팅 실행 가능성은 이미 확인했지만 ORB/encoding/메모리·최소 자산 효율은 수용된 것으로 취급하지 않는다. Primary가 구조·승인·수명·통합과 실제 game eligibility를 결정한다. 세 named evidence worker는 ORB, codec, memory 소유 경계의 read-only 사실만 조사했고 변경/채택을 승인하지 않는다.

## Baseline And Initial Evidence

- 변경 전234개 Rust/manifests/toolchain의 집계 SHA256은 `0b6b93652f3b3d43df84f1857c8bde770de6ade28bd3378d29add2d13e97da59`; EXE는 `63df92fb6104116a0e793ecf9ab5f2dac200ce94bd0560b87e243c21b2180534`. 이전292+native4/Python765는 이 기준의 결과이며 이후 수정의 통과가 아니다.
- 설정의 invalid optional gap은 loader에서 override 제거+원래 값/JSON pointer 경고, runtime PauseSchedule에서 Duration 표현 가능성 재확인+기존 소비자 기본값을 사용한다. Summoner 기본0.4초다. 원본 JSON을 바꾸거나 사용자 입력 관리를 해결책으로 삼은 것이 아니다. 실제 개인 설정 경고는0이고 비교 fixture에서 결함을 발견했다.
- ORB는 FAST 점수를 전체 내부 영상에 만든 후31px border/mask를 NMS 결과에 적용한다. NMS에 필요한 mask 밖 이웃을 보존하는 연산 범위 축소를 조사한다. Codec는 각 프레임 YUV/reconstruction을 할당하고 f64 두 축 DCT/IDCT를 사용한다. 메모리는 live frame/report/worker와 allocator/driver 보유를 아직 구분하지 않았으므로 누수로 단정하지 않는다.
- 원본 assets는 regions421,556,772B/ui30,307,178B/character3,923,343B/items242,021B이며 private는 별도다. 이전 배포는 공용 assets 전체를 포함했다. runtime 참조 closure와 개발 참고 PNG를 확인한 뒤 패키징만 축소한다.

## Current Route

입력 없는 변경 전30/3 benchmark → 세부 계산/heap 계측 → 원인별 작은 수정과 parity/품질 검증 → 전체 회귀 → 신규 최소 패키지와 실게임/Stop/자원 재검증 → 결과·정리. 기존 게임 복구 확인은 수신됐고 마지막 저장 위치는 Act2 캐릭터 선택 화면이다. 현재 장면은 실제 재확인하며 같은 복구 질문을 반복하지 않는다.

## Checkpoint: First Computation And Asset Pass

- 자체 process/default heap 계측은 platform/windows에 격리했다. 입력 없는 terminal/objective80회 뒤 models/frame/worker를 drop/join하면 heap allocated100,706B/private3,252,224B로 내려간다. GUI 실제 view의 GPU 없는600회 log/layout도 context drop 후 allocated77,632B/private3,088,384B다. 이 두 독립 경로에서는 기존569MiB 잔류를 재현하지 못했다. 기본 heap의 allocated/committed와 전체 process private/working set은 다른 범위·측정 시점이며 누수로 단정하지 않는다. 실제 GUI/game 계측이 이어진다.
- 세부30/3 기준: FAST4.486/5.163ms, blur1.468/1.847ms, ORB level7.293/8.144ms, all levels13.746/15.276ms. Codec color5.113/7.228ms, global motion0.335/0.351ms,1000 DCT/quantize0.268/0.434ms,1000 IDCT0.087/0.088ms. Codec 복원은 정수 예측까지 f64로 만들고 픽셀별 round를 반복했다.
- FAST 계산은 유효31px mask/border와 NMS 한 픽셀 이웃 범위로 한정하며 descriptor 거리 계산을4개 u64로 묶었다. Codec 예측/원본 블록은 u8로 보존하고 무변환 복원은 행 복사, 실제 변환 복원은 기존 반올림을 경계 비교로 유지한다. Bits writer는 같은 bit order를 byte 단위로 작성한다. 절반값 인접/NaN/무한대·비트 경계/overflow·256개 descriptor 비트와 기존 source/영상 회귀가 첫 packet에서 통과했다.
- 첫 packet은 lib76 pass/5 ignored(기존 native3+계측2), source background5, observer4, independent MPEG-4 quality5 통과다. 추가 mask/border 작은 영상도 oracle과 비교했다. 이후 loader의 유한하지만 Duration 범위를 넘는 optional gap을 중앙 경계에서도 제거/경고하도록 일원화했고 최종 전체 회귀는 아직 필요하다.
- 같은 north-step-before PNG30/3에서 ORB pair43.768/47.086→40.332/48.322ms, encode/write62.108/68.357→29.094/37.838ms다. Python은13.239/13.877ms와4.842/8.786ms로 아직 더 빠르다.33-frame Rust static AVI는 전후802,700B로 같다. 부호화·frame/decision hash와 독립 품질을 최종에도 검증한다.
- 실제 loader 조사에서 runtime references는 loading-window/exit-menu 두 이미지다. 원본 shared assets456,034,116B 중 non-reference+이2개는4,966,144B다. runtime-bundle 정책으로 원본은 유지하고 default 패키지만 축소했다. 실제 모든 등록 loader·3개 캐릭터/config asset paths5개 초기화 smoke는 입력/캡처 없이 통과했다. 필수 journal을 잠시 제거한 음성 검사는 실패했고 원상복구했다. 첫 후보 package643파일/23,496,168B,154dependencies다. 이 후보는 추가 개선 전 checkpoint다.
- 실제 game 재관찰에서는 Flash가 선택된 Act1 배경의 캐릭터 선택 화면을 확인했다. 이전 Act2 마감 기록을 현재 상태로 재사용하지 않았다. Screen capture/Flash/Hell의 계측 GUI를 최소 패키지 CWD에서 열고 사람이 승인한 bounded Summoner 실행을 시작했다. 완료/입력/메모리 결과를 미리 기록하지 않는다.

## Checkpoint: Native Memory Isolation And Compute Ownership

- 첫 실제 room은 Arcane 도착10.178초, 연속256프레임, 구간1/4 완료 뒤85.008초 부근에 `continuous navigation interrupted or unfocused`로 끝났다. Stop과 focus 중 어느 것인지는 report가 구분하지 않는다.240개 sent 이벤트에 down/up57쌍(F2 각27/F4 각30), move105/press21이며 마지막 held는 비어 있다. 획득→처리 p50/p95=243.142/273.491ms, preflight=215.321/250.161ms다. 처치/버프 효과/완주 성공으로 세지 않았고 캐릭터는 Act2 선택 화면으로 저장 종료했다.
- 실제 GUI는 idle private 약132MB/default heap allocated18.17MB에서 실행 뒤 private1,512,263,680B/allocated19,440,324B가 남았다. GUI drop 후에도 private1,474,707,456B였다. 이는 살아 있는 Rust default heap 전체 크기와 다르며 특정 드라이버/allocator를 확정하지 않는다.
- 빈 OS thread2500개는 headless private+0.28MB, GUI+35.33MB 뒤 plateau로 전체1.5GB를 설명하지 못했다. 같은 terminal/objective 판독80회를 GUI에서 반복하면 private522,530,816B까지 증가하고 model/frame/drop/join 뒤511,582,208B가 남았다. 같은 GUI에서 full-size 화면 캡처80회는 drop/join 뒤118,407,168B로 돌아왔다. 캡처 단독과 실제 계산의 반복 스레드 경로를 분리한 결과다.
- 호출마다 생성하던 area/FFT/normalization/ORB/terminal OS thread5곳을 최대4개 공유 scoped 계산 worker로 바꿨다. Rayon1.12.0은 순수 Rust이고 nested 작업도 같은 풀을 사용한다. session/observer가 lease를 보유하고 registry는 Weak만 보유하며 마지막 lease에서 pool 종료+실제 OS join을 한다. 생성/실패 join은 registry lock 밖에서 수행한다. 동일 GUI 판독80회는 private136,138,752B, model/frame/drop/join 뒤123,482,112B로 돌아왔다. 이 결과는 첫 pool 구현이며 마지막 전체/실게임 검증은 이어진다.
- pool+정수 YUV 첫30/3: ORB35.052/37.224ms, encode25.758/33.911ms. static AVI SHA256은 변경 전·정수 codec·pool 모두 `a039f3007343fdef1f37eadef9c040c92a220035c14c1296f9b88249640723d6`다. 입력 BGR/gray/floor 지문과 HUD/motion의 최종 합성 비교는 마지막 후보에서도 확인한다.
- pool/Duration/YUV의 다음 packet은 lib81 pass/5 ignored, background5/config10/motion4/codec5로105 pass다. partial worker creation 실패의 실제 join, nested scope/panic 재사용과 frame Weak 해제가 통과했다. 이후 SIMD/DCT packet은 lib84 pass/5 ignored, background5/codec5로94 pass다. ring65536개/strict20/tail, f64 행렬 대비<1e-9, 독립 decoder의 기존 PSNR/MSE/seek/움직임 조건을 통과했다. 이 뒤 추가한 동시 acquire/startup unwind 소유권·nearest/half·blur vector 변경은 아직 최종 회귀가 필요하다.
- SIMD 단계30/3: FAST0.995/1.212ms, blur1.385/1.569ms, level3.720/4.009ms, all7.949/8.741ms. codec color2.407/2.783ms, global0.333/0.355ms, quantize1000=0.178/0.253ms, inverse1000=0.0415/0.0599ms. 현재 wide1.7.1/safe_arch1.2.0의 기본 x64 SSE2 safe API만 사용하고 host CPU flags/fast math/외부 DLL은 도입하지 않았다. 최종 EXE의 import/고지는 패키징에서 재확인한다.
- 검증 중 build 완료 전에 이전 진단 EXE를 실행해 Windows 파일 잠금으로 한 build가 실패했다. 해당 잘못 이름 붙인 `vision-gui-pooled` 실행은 pool 비교에서 제외했고, 종료 확인 뒤 별도 build 완료 후 `vision-gui-pooled-first`를 측정했다. Cargo output 외 목적별 진단은 task-owned scratch 한 곳에서만 보유하며 마지막 기록 후 정리한다.

## Checkpoint: Bounded Parallelism And Recording Reuse

- 추가 resize/blur·startup 수명 packet은 lib88 pass/5 ignored, background5/motion4/codec5/vision18로120 pass다. pool8 및 codec 버퍼 재사용/행 밴드의 다음 packet은 lib90 pass/5 ignored+codec5로95 pass다. 두 packet은 중복 검사를 포함하며 고유 검사 수로 합산하지 않는다.640×480 독립 decode와 직렬/8-worker의 bitstream·복원 픽셀 일치,40프레임 동안 source/reconstruction 각2개 버퍼만 재사용하는 검사를 포함한다.
- 정지 화면도 직전 원본이 아니라 손실 복원 참조와 다시 비교·변환해 residual 잡음을 반복 압축하던 비용을 확인했다. 직전 원본 YUV가 정확히 같은 프레임/MB만 표준 zero-MV skip으로 유지하며 움직임/컷/GOP16과 시간축을 보존했다. 독립 MB 변환만 borrowed 행 밴드로 병렬화하고 motion 예측/DC/syntax는 직렬 순서를 보존한다. packet은 최악 크기 전체를 할당하지 않고64KiB부터 fallible 확장한다.
- 첫4-worker 제한에서 일부 template가 느려져12논리 코어인 이 PC에서 최대8-worker를 비교한다. ORB는 이전 단계 의존성은 유지하면서 완성된 pyramid 단계의 추출을 다음 단계 생성과 겹친다. 첫8-worker/pipeline/codec 재사용30/3에서 ORB16.859/18.153ms, encode2.334/12.733ms, terminal95.589/102.472ms, objective75.626/79.948ms였다.33프레임 static은764,782B다. 모든 입력/gray/floor 지문과 HUD·motion 판독은 이전과 같으며 motion float는 Python 대비 작은 수치 차이가 있다.
- 이 checkpoint 뒤 Q8 pyramid 보간의4개 정수 lane과 FFT/정규화 최대8행 밴드, 실행 job 안의 FFT scratch 생성으로 남은 지연/peak를 개선 중이다. 해당 최종 변경은 check/Clippy와 전체 source/quality·실제 자원/게임 검사를 별도로 통과해야 한다. 앞선 static 녹화 수치를 움직이는 영상 전체에 확대하지 않는다.

## Checkpoint: Final Integration Verification

- 최종 production EXE는16,653,312B/SHA256 `d288a71bf8ab6df0f27c2bf886b67ee229348ae329610ca67c80c97fead3c09e`다. 기본 MSVC/x64/SSE2와 static CRT, Rust1.99.1/thin-LTO/codegen1이며 host CPU flag는 없다. PE imports19개는 Windows/system API이고 개발용 HeapSummary/K32GetProcessMemoryInfo는 production EXE에서 제거됐다.
- 필수 loader3캐릭터/config asset5 smoke와 고지161개를 포함한 새 최소 후보는659파일/23,685,554B다. 앞선 후보 이후 asset/config 안내만 갱신해 다시 패키징했다. Windows만 PATH에 둔 자체 CWD에서 diagnose/help/tool help0, 실제 GUI 열기/PNG 저장/정상 종료0, CLI DRY-RUN/RUNNING의 현재 게임 pixels 표시/Escape/실제 종료0을 확인했다. PNG는1922×1140/3,761,991B이며 offline survival-observe도0이다. 선택 화면이므로 HUD unverified가 정상이다.
- 잘못된 선택 대기시간4종류(NaN/Infinity/1e300/-1)를 task-owned 복사 config에 넣고 실제 새 EXE의 offline policy 관찰을 실행했다.4개 pointer/original 경고·parseable JSON stdout·exit0·입력0과 원본 fixture hash 불변을 확인했다. character/room 옵션 없는 순수 이미지 관찰은 config를 읽지 않으므로 그 첫 실행은 이 repair 검사 증거로 쓰지 않았다.
- 최종 fmt/check/all-target Clippy는 통과했다. 첫 portable 전체 검사 packet은183 pass/1 fail/5 ignored 뒤 field-runtime에서 멈췄다. 실패는1e300 경고가 runtime에 있어야 한다는 오래된 test expectation이었다. 중앙 loader에서 이미 제거/경고하므로 config의 경고 path/original/override 부재와 runtime의 중복 경고 없음으로 검사를 수정했다. 독립 PauseSchedule 경계의4개 fallback0.4초/취소 검사는 그대로 유지했다. Production은 바꾸지 않고 전체 packet을 다시 실행한다. 후속 target은 첫 packet에서 실행되지 않았음을 보존한다.
- 수정 후 전체 packet55개 result groups/310 pass/0 fail/6 ignored가296.92초에 통과했다. lib91/5 ignored, field-runtime12, config10, background5/motion4, MPEG-4독립 decode5, vision18과 나머지 observer/session/runtime을 포함한다. fmt/check/all-target Clippy도 다시 통과했다. ignored native fixture/GDI/listener3개1.21초+readonly capture1개0.53초가 별도 통과했다. 남은2개 ignored 성능 profiler는 다음 독립 시간 측정에 속하며 일반 pass 수에 합산하지 않는다. Production/source237개 지문은 EVAL-0032가 소유한다.

## Checkpoint: Remaining Live Thread Paths

- 첫5-site 공유 풀 후보의 실제 Flash/Hell 검사2회는 각각286/313 continuous rows였다. 처리 p50/p95는221.665/248.478ms와219.768/252.325ms, stale_hold12/286·23/313으로 변경 전243.142/273.491ms·93/256에서 줄었다. 각각약86초/111초 시점에 geometry guard가 실행을 끝냈으며 GUI 전면화/Stop 요청과 겹쳐 원인을 분리하지 않는다.1/4구간 완료, 전체 처치/key/방 성공으로 세지 않았다. 두 번 모두 살아 있는 용병/HP를 확인하고 Act2 선택 화면으로 저장 종료했다.
- 두 report의 sent 이벤트는367/413개다. 첫 F2/F4 down/up은74/74·20/20, 둘째는68/67·42/42다. 둘째 한 down은 이미 held인 F2의 중복 down이며 순서 재생 최종 held는 둘 다 비어 있고 마지막 F2는up이다. 둘째 마지막 navigation snapshot은 cleanup 전 held F2였지만 이후111.2931551의up이 있다. buff는7dispatch/unconfirmed/effect_verified=false다. 요청·실제 OS 전송과 게임 효과를 구분한다.
- stopped 첫 private240,873,472B/default heap19,582,112B, 둘째393,355,264B/20,813,623B, GUI drop383,856,640B/13,563,862B가 남았다. 두 회 전체 최대 private801,730,560B/default heap423,889,477B는 진단 작성까지 포함한 최대이며 정밀 active/stop 시각 정렬은 없다. 첫 수치 개선만으로 누수/잔류 해결을 선언하지 않았다.
- 추가 call-path 조사로 hazard crop1–2개 +각 RegionObserver template worker최대2개가 관측마다 std scope를 만들고 loot scan도최대3개 OS worker를 만드는 것을 찾았다. 이3곳도 공유 compute로 옮기고 RegionObserver가 모델 수명 동안 lease를 유지한다. 후보/crop/template 순서·오류·panic 메시지·수치/입력 정책은 유지한다. 마지막 source에서 app I/O/std 테스트 외의 per-call vision std scope가 없는지 확인했다. load·부분 실패·nested 처리의 회귀와 full packet/실제 반복을 다시 검증한다.
- 입력 없는 재현 example에 --hazards와 --repeat3회(새 outer worker/model/pool 수명)를 추가했다. 이 변경의 fmt/all-target Clippy와 release EXE/examples build는 통과했고 전체 static-CRT 회귀가 진행 중이다. 첫후보의 최소 package는 추가 코드 변경 전 checkpoint로 남기고 마지막에 새로 만들고 정리한다.
- 사용자는 실제 게임 제어 반응을 본체 목표, 녹화를 검증·개선용 보조로 재확인했다. 서브에이전트2개가 현재 capture/판독의 bounded read-only 경로를 조사했고 Primary가 WGC/DXGI/GPU 공식 자료를 검토해 [계획의 후속 구조 제안](../refactoring/refactor-0003-rust-efficiency.md#real-time-architecture-exploration--proposed-not-implemented)에 기록했다. 지금의 compute/resource 수정은 계속하고 WGC/GPU/cadence 새 구조를 이미 구현한 것처럼 기록하지 않는다.

## Checkpoint: Complete Pool, Final Distribution And Live Repeats

- 첫5개 경로 후보는 production237개 지문 `8cf50238e151b7deabe4a6b753dcfd0345140e866f2c9345793d493dbcc1dbc2`, EXE16,653,312B/`d288a71bf8ab6df0f27c2bf886b67ee229348ae329610ca67c80c97fead3c09e`였다. 당시 full packet310pass/0fail/6ignored/296.92초와659파일/23,685,554B 패키지는 그 checkpoint의 역사다.
- 추가3개 경로까지 공유한 production237개 source/manifests/toolchain 지문은 `7d56f7185fb5f2b1b1bc547aa8dd10d15f132cb2750704c33c8a8acd6cff9324`다. fmt/check/all-target Clippy-D warnings, static-CRT release all-target serialized55groups/310pass/0fail/6ignored/580.57초가 통과했다. native4개(3개1.20초+1개0.53초), 성능 profiler2개1.32초도 별도 통과했다. 중복 packet을 고유 테스트 수로 합산하지 않는다.
- 최종 동일 PNG/30sample/3warmup에서 ORB pair44.196→15.409ms, stored packet64.716→39.035ms, near hazard16.028→12.137ms다. 모든 Python/OpenCV 우위를 달성한 것은 아니며 추가 순차 OFF/ON 측정의 ORB18.990/19.650ms도 숨기지 않는다. 입력/gray/floor/HUD는 같고 Python motion은 기존 작은 float 허용 차이다. 최종 전체 표/품질/메모리와 해석 한계는 EVAL-0032가 소유한다.
- 입력 없는 GUI hazard80회×3회는 새 outer worker/model/pool마다 model/frame drop와 실제 worker join까지 전부 도달했다. post-join private120.68→124.44→125.04MB/heap16.63→16.67→16.67MB다. stderr0이며 per-call thread 생성은 제거됐지만 무한 반복/장시간의 누수 부재를 이 검사로 확정하지 않는다.
- 같은 실제 계측 GUI에서 Flash/Hell2개 방을 실행했다. 첫 report `gui-20261024-093736-96cd41c2f4`의137rows는 처리218.112/246.975ms, stale4/137이다. 현재 단상 시체 pair로 kill_confirmed를 보고했으나 Nova dispatch0/픽업0이고 canyon finish deadline으로failed/구간0이다. 둘째 `gui-20261024-094023-fa72fc0642`는 수동 녹화를 병행했고291rows/235.580/273.266ms/stale71/291, 구간1/4 뒤 terminal 재획득 미확인으로field_paused였다. 다른 random room이므로 녹화의 단독 효과를 입증하지 않는다. 두 번의 down/up51/51·70/70은 순서 재생 최종held=[]이고 effect_verified=false인 buff7dispatch만 보고했다. 살아 있는 캐릭터/용병을 확인하고 저장 종료, 마지막은 Act2 선택 화면이다.
- 같은 process 최초private72.38MB, 첫 종료 뒤95.59MB, 둘째 종료 후103.94–105.85MB, GUI drop91.80MB/heap13.60MB, process exit0/stderr0다. 전체 peak private553.67MB/heap443.51MB는 일시 할당도 포함한다.2초 표본을 개별 계산/저장 단계에 정확히 귀속하지 않는다. 이전5개 경로 후보의241→393MB 잔류와 구별한다.
- 실제 녹화 병행 결과를 분리하기 위해 developer example에 --record-during만 추가했다. production source/EXE 지문은 불변이다. 새 example의 fmt/Clippy-D warnings/release build/test harness(검사0개)와 실제 같은 process/pool OFF→ON30/3 검사를 통과했다. packet40.832→41.427ms, terminal90.514→94.633ms이며 결과 지문/HUD/motion 전체는 같다. ON은 선택 화면 녹화여서 움직이는 전투/전체 제어 pipeline과 같지 않다.
- 최종 독립 decoder는 live59/59·static33/33·실게임 수동940/940·동시 계측107/107을 모두1922×1140/8fps로 읽고 처음/중간/끝 seek의 순차 화소 지문 일치를 확인했으며 stderr0이다. 실게임 수동 파일76,989,990B에는 별도 PSNR/동일 XVID 비교가 없다. codec64프레임 정지/pan/cuts 품질/크기 표는 source가 같은 검사의 EVAL-0032에 유지한다.
- 최종 EXE16,670,208B/`92ee301f0fde68e4bc1dda880eb681c31bbc636022d033b5fa5232e90779d990`, 최소 package659파일/23,702,450B/assets291파일4,967,268B/고지161개다. candidate와 기존 package의 모든 payload hash/bytes/containment/중복/extra를 검증하고, 관련 process가 없음을 확인해 `dist/d2bot-windows-x64`로 교체했다. 교체 후658payload/전체659파일과 hash/bytes를 다시 검증했다.19 Windows DLL imports뿐이고 production에 HeapSummary/K32GetProcessMemoryInfo 이름도 없다. 자체 CWD/Windows만 PATH에서 help/diagnose·3캐릭터/5config 자산 loader가 통과했다.
- 실제 배포 EXE의 GUI/PNG1922×1140 저장과 종료0, CLI DRY-RUN/RUNNING 현재 선택 화면 표시/Escape 종료0, 개발 명령5개 help0, offline PNG 관찰0/입력0을 확인했다. 최종 optional NaN/Infinity/1e300/-1 fixture도4개 경고/JSON출력/exit0이며 실제 원본 파일을 바꾸는 방식이 아니다. 생성 영상/캡처는 배포에서 제거했다.
- 본체의 실시간 반응 목표는 미완료다. 전체 feature/refactor 수용은 in-loop/Provisional을 유지하며 WGC/GPU/latest-frame/판독 cadence 분리는 계획의 조사 제안이다. 현재 전체 회귀 PASS를 전체 gameplay·모든 OpenCV 속도 우위·새 OS/사람 수용으로 확대하지 않는다.
- 최종 관련 문서의 UTF-8/대체 문자·로컬 링크/앵커와 임시 경로에 대한 durable 링크 부재를 확인했다. git diff whitespace 검사와 fmt를 통과했고 tracked Python/runtime config JSON/공유 이미지·requirements/pyproject 변경은0이다.237개 production source 지문과658개 배포 payload를 재검증한 뒤 활성 task process·reparse 없음/절대 경로 포함을 확인해 task-owned scratch와 검증 완료한 이전 배포 backup을 제거했다(3,187파일/1,067,098,095B). 원본 자료·현재659파일 배포·보통 Cargo outputs/cache는 유지했다. 임시 진단의 필요 사실/수치는 이 RUN/EVAL에 남으며 후속 실행은 새 task-owned output을 사용한다.

## Planning Return: CPU First And Approximately50ms

후속 사용자 요청은 약50ms를 위한 구조 검토와475→23.7MB 배포 축소의 설명이다. WGC/GPU는 우선 보류하며 인용 추천 프롬프트를 곧바로 구현 승인으로 확대하지 않았다. 제어 시각/관찰 유효성으로 위험과 목표가 옮겨 [REFACTOR-0004](../refactoring/refactor-0004-realtime-decision-latency.md)에 실행 전 CPU 계획을 분리했다. runtime·config·assets·EXE 변경/새 게임 입력은 없다.

Primary가 source의 `acquired`가 캡처 전임을 확인해 이전17+220ms 설명을 EVAL-0032/이전 계획에서 명시 정정했다.50ms aim settle·20ms service, 이미 있는 early combat/ROI/cache/동시 실행, 연결되지 않은 common Fast/Slow runtime과 남은 full-map/FFT 반복을 구분했다. named read-only worker3개는 call path/계산/자산의 한정 근거를 제공했고 구조·실행 경계는 Primary가 판단했다. normal runtime 자산 loader3캐릭터/config5/capture0/input0와 자체 CWD diagnose를 다시 실행해 exit0, package658payload와 assets291개의 hash/bytes/포함을 읽기 감사했다. 개발 replay/calibration/임의 새 config의 corpus 가용성은 이 finite loader 검사의 범위 밖임을 EVAL에 남겼다. 전체 회귀310/native4는 앞선 구현의 검사이며 이번 문서 검토로 반복 실행한 수라고 부르지 않는다.

검토 문서6개의 strict UTF-8/로컬 링크·앵커131개, 계획10개 section/5개 step의6필드, 이전/현재 용량 합계와 whitespace 검사를 통과했다.237개 Rust source/manifests/toolchain 집계7d56과 배포 EXE92ee도 기존 검증 값과 같았다. 의미/조건/현재·제안 상태를 private source-unit/draft와 대조한 뒤 이번 review의 임시 사본·검사 도구를 제거했다. 현재 배포/원본 자료/Cargo cache는 유지했고 새50ms 달성·게임 입력·WGC/GPU 구현 PASS는 기록하지 않는다.
