# SPEC-0030: CPU 판단과 확인된 연속 입력

## Metadata
- Status: `approved`; Date: `2026-10-24`; Feature: [FEAT-0030](../feature/feat-0030-cpu-realtime-control.md); Run: [RUN-20261024-02](../run/run-20261024-02-cpu-realtime-control.md).
- Profile: `fullstack-product`; Evaluators: Contract, Functional; Plan: [REFACTOR-0004](../refactoring/refactor-0004-realtime-decision-latency.md).

## Authority And Baseline

50ms의 최신 의미는 FEAT-0030의 지향 목표다. 기능·판독 품질·안전성을 유지하면서 실제 반응에 불필요한 계산/복사/대기/자원 점유를 줄이며50ms 초과만으로 수용을 거부하지 않는다. 화면의 관찰을 덜 하거나 threshold/안전 검사를 완화해 숫자만 줄이지 않는다. 입력/화면 읽기 비용과 판단 자체의 비용, 다음 새 화면을 기다리는 시간, 작업 queue/합류 대기, 실제 시전·게임 효과를 구분한다. 주기·ROI·알고리즘 변경은 같은 목적의 품질/안전 검증과 명시한 의미 변화 계약을 계속 요구한다.

감독 복구도 기존 운영 계약을 따른다. TP 주문서로 임의 귀환하거나 수동 마을 이동으로 다음 시험을 준비하지 않는다. 빈 끝/중단 경로는 기존 `--return-to-hub-from north`의 현재 판독→중앙WP→Act1→저장 종료를 사용한다. 서모너 지점은 현재 시체/일지·협곡 경유 계약을 따른다. Act2 포탈 도착→마을WP의 별도 검증된 코드가 마련되기 전에는 이를 대체 경로로 쓰지 않는다. 2026-10-24 사용자가 재확인했으며 이미 수행한 수동 TP 복구는 위반/개입으로 RUN에 남기고 정상 수용에 합산하지 않는다.

감독 return-only resource-probe는 기존 `SummonerOptions`의 return_from=north/resume_arcane/확인한 labels-off/continuous/finish_room=true와 native.execute를 그대로 조합한다. interval60초/최대4구간 뒤에도 불명·기한 초과면 기존 pause/stop으로 보류한다. GUI 액션 전송 전에는 입력을 보내지 않고 별도 room cycles를 시작하지 않는다. CLI의 return 플래그만으로는 finish_room이 켜지지 않으므로, 중앙 인식만 한 검사를 Act1 마감으로 부르지 않는다. return-only는 성능/정상 새 방 성공 수에서 제외하고 실제 중앙/Act1/선택 화면을 기록한다.

후속 사용자 지시로 안전한 구조 개선·읽기→판별 단축을 우선하고 기존 타이밍/threshold/플레이 적응은 후속으로 구분한다. 기존 결과 보존을 의도한 Area/SIMD batch의 동등성 검사는 유지하되, cadence/phase/ROI 의미 변경 자체를 Python 결과 불일치만으로 거부하지 않는다. 안전 검사와 의미 변화의 명시·원래 source·판별 성능 측정은 생략하지 않는다.

최신 사용자 지시가 CPU 계획 실행/조준 대기 개선·감독 게임을 승인했다. 기존 최신 EXE92ee/production237개7d56과 EVAL-0032의30/3 수치를 비교 기준으로 보존한다. 실게임 전 scene/focus/HUD를 새로 확인하되 이미 받은 복구 확인은 반복하지 않는다. 기존 FEAT-0029는 freeze된 이전 결과 owner이며 이번 실행 loop는 FEAT-0030 하나다.

## Initial Implementation Contract

실제 시간이 흐르는 pointer backend 회귀에서0ms 설정도 첫 keydown을 service까지 미루는 결함을 재현했다. apply_inner가 포인터 완료 시각 aimed_at을 저장한 뒤 이동 전 now로 press_if_ready를 호출해 now−aimed_at<0이기 때문이다. 준비 직후 현재 monotonic clock을 새로 읽어 eligibility를 계산한다. 원래 observation/decision-ready 출처는 바꾸지 않고 양수 settling/반복 간격·시한/epoch/포인터 재확인·중단 해제를 유지한다. 시간이12ms 흐르는 fake 이동에서0ms 즉시 down,50ms 설정의 실제 완료 후 대기, 이동 중 시한 초과·취소 회귀를 검사한다.

- 계측을 먼저 넣고 실제 input backend 반환 시각(`backend_returned_at`)과 요청 시각을 구분한다. 이 반환은 native method 완료로, 게임 효과의 시각이 아니다. frame sequence/capture 시작/decision ready를 작게 복사해 held lease가 소유하고 actual keydown/mouse 기록에 붙인다. delay/pending/repeat에도 원래 frame의 source를 유지한다. 새 frame의 lease 갱신은 그 근거를 갱신하고 release는 source를 비운다. 외부 이벤트·JSON 기록 잠금을 OS 호출/대기 중 유지하지 않는다.
- capture 시작/완료, HUD/panel/ground/threats, plan 제출/합류, motion과 최종 directive를 같은 시계로 기록한다. 기존 `acquired_at` alias는 capture 전이라는 뜻을 명시하고 과거 결과와 혼합하지 않는다. 계측만 추가한 baseline은 입력 의미/시한을 바꾸지 않는다.
- 연속 native 입력은 이미 move_pointer에서 실제 좌표±3px 일치를 확인(최대80ms/중단 가능)하고 keydown 전 pointer/focus/epoch/만료를 다시 검사한다. 그 경로의 추가 고정50ms gate만 없앤다. 같은 key hold의 방향 변경에는 keyup/down을 끼워 넣지 않고 새 key/전투 전환은 기존 상대 key 해제 후 수행한다. configured repeat/FCR 의미·다른 단발 approach/hover의 대기를 전역0으로 바꾸지 않는다. 모든 포인터/확인 지연 뒤 stop/expired/epoch 변경이면 late keydown/late move를 금지하는 기존 검사를 유지한다.
- CPU 준비 공유는 같은 source 픽셀·ROI·encoding·scale·FFT tile 조건에서만 사용한다. panel의 전체 resize 대신 필요한 출력 ROI를 원래 전체 target 좌표/동일 Area 가중치·누적·반올림으로 구할 수 있다. 기존TownVision의범위/shape validation을 유지하고 원래 crop→filter→template 순서·threshold/variant 최고점·동점을 검사한다. peak-only가 가능한 소비자와 suppression을 쓰는 소비자를 구분하며 source/독립 회귀로 결과를 확인한다.
- latest-frame/fast-slow 연결은 frame/room/geometry/이동·input 세대와 unknown/late 결과 계약을 코드 변경 전에 이 spec에 구체화한다. 기존 common runtime의8fps/최근5장/decision source tagging을 그대로 연결하지 않는다. 안전 해제/감시/I/O는 포화 compute 뒤로 보내지 않는다. capture slot/실행 중 consumer·codec·증거의 실제 byte/개수와 마지막 참조를 포함해 상한/해제를 검증한다.
- 배포 검사는 EXE와 함께 남은 assets/regions runtime137파일을 포함해 필수 자료를 독립 새 폴더/CWD에서만 읽어 수행한다. production의 CWD 경로 선택과 개발 root/사용자 명시 경로를 구분하고 source fallback 없이 오류를 드러낸다. public 실행 패키지는 EXE+assets+config+고지를 보존한다.

## Verification And Exit

### CPU fast/slow 실행 계약

E 실제 capture p5030.760/p9551.028ms가 남아 CPU GDI 경로의 중복 bitmap→BGRA 복사를 줄이는 batch를 진행한다. 호출 한 번 안의 Surface가32bit top-down CreateDIBSection과 DC/선택 복원/비트 메모리를 소유한다. 별도 GetDIBits BGRA Vec 대신 GDI 완료를 GdiFlush로 확인한 DIB byte를 BGR 소유 버퍼로 복사한다. 길이/네이티브 크기/isize 경계를 확인하고 원시 byte는 먼저 초기화하며 slice/handle은 Surface 밖이나 다른 thread로 넘기지 않는다. 새 unsafe Send/Sync나 전역 캐시를 만들지 않는다. GDI 실패/부분 초기화/close 오류는 기존 typed 오류·fallback 범위로 전달하고 owned fixture 색상·GDI handle 균형/실제 close 및 live 화면/capture 속도를 검사한다. 메모리 소유·GDI 완료 동기화는 [Microsoft CreateDIBSection 계약](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-createdibsection)을 따른다. WGC/GPU/hook은 추가하지 않는다.

HUD와 패널의 순수 판독은 같은 불변 현재 Arc에서 병렬 시작할 수 있다. NativeContinuousVision의 기존 fast observation pool에 패널 task를 최대1개 제출하고 HUD 뒤 prepare에서 실제 join한다. PanelObservers의 동일 Arc Weak memo만 준비 결과로 재사용하며 다른 frame의 결과를 섞지 않는다. camera authorization·현재 HUD/발/패널·focus 검사는 모두 합류 뒤 기존대로 수행한다. 입력을 사전 승인하지 않고 HUD 오류/조기 종료도 pending panel의 실제 join/오류 회수를 거친다. 기본/mock 소비자는 begin_frame=no-op이다. 패널 실제 시작/완료와 HUD/최종 합류 시각을 구분하고 new frame mismatch/미소비 결과/오류·close를 검사한다.

checkpoint C의 near p5074.314ms/전체136.295ms와 새F4 입력160.626ms는50ms를 충족하지 않는다. current 단계 HUD/panel/ground/near를 직렬 합류하고 slow FFT·녹화가 같은8-worker compute를 쓰는 비용을 다음 CPU batch에서 줄인다. 먼저 compute 총 상한8을 유지한 foreground/background 분할을 검증한다. foreground는 최대6, background는 최대2이며12논리 코어 환경의 값이다. 적은 코어는 총합 안에서 나누고1코어는 같은 풀을 재사용한다. broad terminal/objective만 background를 사용하고 현재 HUD/ground/hazard/hover/path/입력과 녹화의 기존 작업 의미는 유지한다. nested scope는 진입한 pool 안에 남기고 family의 마지막 외부 lease가 두 pool의 실제 OS worker를 join한다. 작업마다 풀/OS thread를 만들지 않는다. startup 부분 실패·panic 재사용·background 포화 중 foreground 수행·두 pool의 마지막 참조 해제를 검사하고, 새 지문/패키지로 결과를 따로 측정한다. 기존 C는 비교 checkpoint로 freeze하며 데이터 차이와 실제 CPU 경쟁의 인과는 측정 없이 확정하지 않는다.

실게임 checkpoint B에서 source capture p5045.664ms와 완료→소비 요청15.483ms, 근처 합류27.526ms가 나타나 계속60fps를 생성하는 정책은 채택하지 않는다. 다음 batch는 producer1/대기1 소유를 유지하되 소비 요청에만 새 캡처를 시작하고 요청이 없으면 대기한다(시작 시 사전1장만 허용, 상한60fps). 슬롯에 남은 오래된 프레임은 재사용하지 않는다. 포커스를 요구하는 연속 제어에서 capture_backend=auto와 named window인 경우에만 screen 경로를 선택하며, 명시 window 설정·preview/녹화 경로는 유지한다. actual backend를 source metadata에 기록한다. 캡처·입력의 기존 target/client/geometry/focus/freshness 검사는 모두 유지한다. wide terminal/objective 관찰 host는1개/queue2로 제한해 두 broad FFT가 fast compute를 동시에 포화시키지 않게 한다. slow source/unknown/positive 재확인·오류/실제 join 계약은 그대로다. 새로운 동시성/오래된 슬롯/요청 없는 대기/중단·close 실패를 검사하고 녹화OFF/ON actual source→near/입력을 다시 측정한다.

현재 근처 위험 판독과 hover 판독도 합류점을 나눈다. 같은 현재 frame에서 두 작업을 병렬 시작하되 current HUD/panel/ground/near-hazard가 끝나면 native 제어는 hover 합류 전에 신선함·focus·Stop·deadline을 확인하고 가까운 위협에 한해 이동 해제→전투를 요청할 수 있다. hover 결과는 이동 승인 전에 반드시 같은 frame으로 합류하며 미완료를 clear로 쓰지 않는다. pending hover는 한 개이고 오류/early exit도 실제 pool join 뒤 회수한다. native가 이 선점을 명시 지원할 때만 적용하며 기존 기본/mock 소비자의 순서는 유지한다. near/hover 완료 시각과 실제 backend source를 따로 기록한다.

후속 steering에 따른 다음 CPU phase는 중앙 출발을 현재 판독으로 확인했거나 기존 승인된 outbound resume proof가 있고, 복귀/획득 목표 접근 단계가 아닌 동안 local field 판단을 우선한다. 이 phase에서는 이미 떠난 중앙WP 전체 재검색을 하지 않는다. 첫 mapping은 현재 frame으로 수행하고 이후 ORB/terrain/graph 위치 갱신은 source capture 시각 기준 최소100ms 간격으로 제한한다. 관측하지 않은 motion은 displacement=None/`mapping_deferred`로 남기며 0 이동이나 현재 world pose로 바꾸지 않는다. 그 사이에는 graph의 오래된 features를 다시 소비/국소화하지 않고 `position_current=false`로 보고하며, 이동은 현재 path/floor/ground/HUD/근처 위협으로만 local aim을 고른다. 오래된 graph port/좌표를 현재 aim으로 투영하지 않는다. 목표 양성의 현재 확인·terminal 획득/복귀/도착 단계에서는 현재 전체 mapping/landmark 판독으로 돌아간다. 새 phase로 경로 선택/실게임 성공률이 달라질 수 있으며 튜닝은 후속; current source·250ms 만료/Stop/focus/geometry·pointer 확인/해제는 완화하지 않는다. 이 phase는 앞선 checkpoint의 전체 회귀 완료 뒤 적용하고 새 수치와 안전 회귀를 별도로 검사한다.

첫 outbound 프레임은 기존처럼 현재 terminal/objective 전체 판독을 합류한다. 이후 미획득 이동에서는 각 slow recognizer당 실행 중 작업 한 개만 허용하고 완료 결과를 비차단 poll한다. 음성·미완료는 `goal unknown`이며 현재 HUD/panel/ground/근처 위협/경로/floor 검사를 대체하지 않는다. 이전 결과의 좌표나 음성 판정을 현재 frame의 tracker/이동 허가로 전달하지 않는다. 양성 hint는 원래 capture 시작 시각을 진단에 남기고 현재 이동을 해제한 후 같은 현재 frame의 전체 recognizer로 재확인한다. 현재 확인의 freshness/geometry/stop/focus 조건은 기존과 동일하다. terminal 참조를 획득한 접근/재획득 단계는 매 프레임 현재 전체 확인으로 돌아간다. 작업 오류는 unknown으로 삼키지 않고 종료/해제 경로에 전달한다. 느린 작업이 길어져도 무한 제출·FIFO backlog를 만들지 않는다. interval 종료는 실제 pool join과 미소비 결과의 오류 회수 뒤 완료한다.

capture를 독립 owner로 옮길 때는 동일 ScreenCapture 한 개를 이동시켜 producer 한 개와 최신 대기 frame 한 개만 유지한다. 소비 중 Arc는 불변이며 source capture 시작/완료·sequence를 보존한다. 새 capture가 대기 slot을 대체하고 오래된 대기 frame을 즉시 놓는다. 원래 engine 요청 시각과 실제 source capture 시각을 분리하고 모든 freshness/입력 lease는 실제 source 시작을 사용한다. geometry drift/focus/Stop 실패는 기존 입력 guard를 우회하지 않는다. 종료 시 producer 취소→capture close→실제 join→대기 frame 해제 순서를 확인한다. 녹화는 별도 8fps의 기존 owner/byte 상한을 유지하며 제어 source의 저장 큐로 연결하지 않는다.

계측 baseline→입력/CPU 작은 batch→저장/fake/runtime 회귀→새 package→녹화OFF/ON 감독 실게임을 순차 수행하고 CPU 시간 검사를 컴파일/회귀와 겹치지 않는다. 블록된 포인터 작업·타임아웃 뒤 resume·epoch/cancel·partial startup/파일 실패·키 해제·실제 worker join을 포함한다. 같은 scene 판독과 실제 입력까지의 원래 frame source를 검사하고 p50/p95/p99/max/unknown/stale를 함께 기록한다. 최종 fmt/check/Clippy/필요한 전체 portable release/native 회귀와 자체 CWD asset loader/GUI/CLI/PNG/녹화·독립 decoder를 확인한다.50ms 초과는 미달 수치로 기록하되 그 자체를 실패로 삼지 않는다. 실제 기능/판독 품질·안전 실패는 별도로 수용되지 않은 상태로 남긴다. 원본 자료/표준 Cargo cache를 유지하고 task-owned 진단은 사실을 canonical RUN/EVAL에 남긴 뒤 제거한다.

## Documentation Contract

이번 목표 해석 갱신의 docs-content 계약은 최신 사용자 설명과 FEAT-0030을 authority로 삼는다. FEAT/SPEC/REFACTOR/RUN/EVAL의 현재 수용 기준을 정정하되 기존 측정값·실제 실패·승인·구현/미구현 경계는 보존한다. 소스/config/EXE/게임 입력은 바꾸지 않는다. Primary가 숫자 목표와 필수 안전/기능 조건의 구분, 역사적 verdict의 수정 이유 및 로컬 링크를 검토한다.

H에 관한 바로 다음 문단은 시도 당시의 계약이며 **성능 이득 미확인으로 폐기했다**. 현재 구현은 G다. H의 별도 landing 작업/새 준비 타입/해당 전용 테스트는 제거했다. G와 기존 계약은 유지한다. 채택 판단과 실제 검증은 EVAL-0033이 소유한다.

checkpoint G의 현재 near p5058.280ms 중 hazard는 발밑/조준점 두480px 영역을 모두 기다렸다. H는 같은 frame/sequence/context/options로 그 두 검사를 준비하되 발밑 결과와 현재 target panel만 먼저 받아 근접 전투를 허가할 수 있게 한다. 조준점 영역은 같은 host의 별도 pending 작업1개로 수행하고 finish_threats에서 현재 candidates에 기존 순서로 합류한 뒤에만 probe/이동 판단으로 진행한다. 두 영역이 같으면 중복 제출하지 않는다. template/ROI/argmax/limit/threshold와 원래 stamp는 유지한다. 발밑 음성만으로 이동·clear를 허가하지 않는다. frame 소유는 불변 Arc로 한정하고 미소비 landing 오류도 실제 join/입력 해제 경로로 회수한다. 기존 combined observer의 결과/sequence 동등성, landing 오류·중단·close 및 native 단계 시간/실게임을 검사한다.

checkpoint F의 실제412행 중 terminal 획득 뒤 current full objective 합류 p95가267.255ms이고 stale_hold185행이었다. 현재 확인도 broad host1/계산2로 보내는 분류 결함을 바로잡는다. SlowScan의 최초 현재 판독·force_current·양성 hint 재확인은 기존 foreground host/계산 pool을 사용하고, 비차단 미획득 탐색만 background host/계산 pool을 사용한다. 인식 알고리즘·현재 확인 횟수·source/양성/음성/unknown·미완료 drain/오류·250ms 만료는 유지한다. background가 막힌 동안 현재 확인이 그 host의 FIFO 뒤에서 기다리지 않는 검사와 native 결과/실게임 재측정을 수행한다. 단순히 worker를 더 늘리지 않는다.

동일 결과 CPU batch의 uint8 직접 correlation은16,384개 이하 블록에서0..255를 i16으로 확장한 SIMD 내적을 i32로 합산하고 u64로 누적할 수 있다. 블록 전체 최대 합16,384×255²<2³¹이고 기존 직접 경로의 전체4,000,000연산 한도는2⁵³보다 작아 정수 누적 재배치는 f64 결과를 바꾸지 않는다. f32 입력과 FFT 경로의 정밀도·선택 한도·정규화는 유지한다. 긴 행/마지막 lane/최대값 및 기존 독립 OpenCV oracle을 검사한다.

이 spec과 현재 구현/측정이 변경 근거다. RUN은 시도/실패/회수 상태, EVAL은 최종 전후 수치·조건·한계, 계획/feature/index는 현재 승인·진행 상태를 소유한다. 필요한 운영/공유 계약만 README/runtime 안내에 반영하고 역사/전체 게임 수용을 덮어쓰지 않는다. 임시 경로를 durable 참조로 만들지 않는다.
