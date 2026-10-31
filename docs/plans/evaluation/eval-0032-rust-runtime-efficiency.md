# EVAL-0032: Rust 효율 개선

## Metadata
- Result: `IN PROGRESS`; Date: `2026-10-24`; Attempt: `1`.
- Feature: [FEAT-0029](../feature/feat-0029-rust-runtime-efficiency.md); Spec: [SPEC-0029](../spec/spec-0029-rust-runtime-efficiency.md); Run: [RUN-20261024-01](../run/run-20261024-01-rust-runtime-efficiency.md).
- Profile: `fullstack-product`; Evaluators: Contract, Functional. Primary owns interpretation and final review.

## Scope

ORB·직접 MPEG-4·메모리 수명과 필요한 배포 자산의 원인을 측정·개선한다. 이전 [EVAL-0031](eval-0031-rust-windows-distribution.md)은 변경 전 구현/실행 증거다. 최신 변경의 회귀·속도·메모리·실게임 PASS를 미리 기록하지 않는다.

## Configuration Repair And Performance Causes

개인 JSON에서 NaN을 발견한 것이 아니라 비교 fixture의 잘못된 입력으로 Python `float()`가 NaN/Infinity/문자열·숫자 overflow를 허용하는 것을 확인했다. Rust는 선택 항목 `buff_action_pause_seconds`의 유한·비음수·`Duration::try_from_secs_f64` 표현 가능성을 중앙 loader에서 검사한다. 잘못된 override만 메모리에서 제거하고 원래 값/JSON pointer/수정 경고를 보존한다. 소비자는 기존 missing-token 기본값(Summoner0.4초)을 쓰며 runtime pause schedule도 변환을 다시 확인한다. 원본 JSON/정상 값은 유지한다. 필수 안전 정책 오류를 조용히 기본값으로 바꾸는 변경은 아니다.

새 EXE에 복사 config의 NaN/Infinity/1e300/-1을 입력한 실제 offline policy 관찰은4개 경고, JSON stdout,exit0/입력0이고 fixture SHA256은 불변이다. character/room 없는 순수 이미지 관찰은 config를 로드하지 않으므로 해당 첫 실행은 repair 증거에서 제외했다. 기존 factory 검사의1e300 runtime 경고 기대는 중앙 처리 이전 가정이어서 중앙 경고/원래 값/override 부재와 runtime 중복 경고 없음으로 바꿨다. 별도 PauseSchedule 검사는 runtime에 직접 들어온4종류의 기본값0.4초와 취소를 계속 검사한다.

Python은 ORB/녹화의 핵심 연산을 최적화된 native OpenCV/XVID에 위임한다. Rust만 사용한다는 사실은 동일한 알고리즘·자료 배치·SIMD·병렬화를 보장하지 않는다. 실제 병목은 ORB의 불필요한 FAST 영역/반복 scalar 계산·직렬 pyramid 준비, codec의 픽셀별 float 복원/큰 DCT·동일 손실 잔차 재부호화·반복 할당, GUI에서 매 호출마다 생성하는 계산 스레드였다.

최종 후보는 유효 mask/NMS 범위의 SIMD FAST, 순서를 보존한 blur/Q8 보간·단계 pipeline,4×u64 Hamming을 사용한다. Codec는 정수 YUV/예측, f64 대칭 DCT/IDCT, byte 단위 bit writing, 필요할 때만 확장하는 packet, 재사용 YUV/복원 버퍼와 정확히 같은 입력에만 적용되는 표준 P-skip을 사용한다. 독립 MB 변환만 병렬화하고 motion 예측/DC/syntax는 직렬이다. 계산은 최대8개 공유 scoped worker를 쓰며 마지막 lease가 실제 OS thread를 join한다. 판정 임계값·시한·핫키·화질/해상도/FPS를 낮추지 않았다.

## Memory Diagnosis

자체 프로세스의 Windows 작업집합/전용 commit과 기본 heap allocated/committed/reserved를 구분했다. 남의 프로세스/게임 메모리는 읽지 않았고 working-set 축출이나 강제 heap 정리를 사용하지 않았다. 기본 heap은 모든 native/driver heap을 포함하지 않으므로 private와 같지 않다.

| 동일 경로/시점 | Private bytes | 기본 heap allocated bytes |
| --- | ---: | ---: |
| Headless terminal/objective80회 후 model/frame/drop/join | 3,252,224 | 100,706 |
| GPU 없는 실제 GUI layout/log600회 후 context drop | 3,088,384 | 77,632 |
| 변경 전 실제 GUI/game85초 후 stopped | 1,512,263,680 | 19,440,324 |
| 변경 전 GUI 자체 drop 후, process exit 전 | 1,474,707,456 | 13,518,417 |
| GUI+판독80회 후 model/frame/drop/join | 511,582,208 | 약16.63MB |
| GUI+화면 캡처만80회 후 drop/join | 118,407,168 | 약16.59MB |
| 첫4-worker 공유 풀+GUI판독80회 후 drop/join | 123,482,112 | 약16.63MB |

빈 OS thread2500개 실험은 headless +0.28MB, GUI +35.33MB 후 plateau여서 실제1.5GB 전체를 설명하지 못했다. 같은 GUI/계산 재현에서 per-call threads를 공유 풀로 바꿔 약388MB의 잔류가 사라졌다. 이 증거는 계산 스레드 생성 경로와 native/private 보유의 관계를 지지하지만 특정 드라이버/allocator의 결함을 확정하지 않는다. 살아 있는 Rust 기본 heap 전체가1.5GB 누수였다는 해석도 지지하지 않는다. 마지막8-worker와 실게임의 최종 반복 수치는 아래 최종 검증에서 별도로 기록한다.

## Measurement Contract

Windows11/12논리 코어에서 Rust1.99.1 release/thin-LTO/codegen1/static CRT와 Python3.7.9/NumPy1.21.6/OpenCV4.13.0 기본12threads를 순차 실행한다. 컴파일/회귀 작업과 CPU 시간 측정을 겹치지 않는다. 동일1922×1140 `north-step-before.png`와 north-step before/after,3 warmup+30sample/nearest-rank p50/p95다. 과거 EVAL-0031의 다른 field PNG 수치를 이 표의 동일 입력 결과로 혼합하지 않는다.

세 BGR 입력은 순서대로 `5932cb5fe12e4d139cb5100b4fdec9069a7170c1257ec066e770fec01f267092`(앞2개), `4c26f2fa734ceb90e9ba5d214c06114fcec6606c5b3279680af071871759218c`다. gray는 `cab419554e728dcc56889b8c544b42afcd60c7b325c8ca5b473e914984343002`, floor는 `0b4207f0cf1820a0f982a89c200cfebebd621c0957ca04265e602b3670f65b28`다. HUD는 life985/985, mana1075/1075, closed belt/observed mercenary다. Motion은327inliers/332matches/background_translation; displacement와 linear error의 Python/Rust float는 작은 차이가 있으므로 byte 동일이라고 하지 않는다.

`stored_packet`은 normalize/gray+HUD+path+두 장 ORB의 개발용 조합이다. 실제 동시 navigation의 FPS나 게임 acknowledgement 시간이 아니다. 정지 화면 encode의 P-skip 효과와 이동/pan/cuts의 속도·품질·크기를 구분한다. live capture는 같은 backend/target의 다른 순간이므로 animated 픽셀 동일성을 주장하지 않는다.

## Latency Timestamp Correction

2026-10-24 추가 구조 검토에서 [continuous engine](../../../src/runs/summoner/continuous_navigation/engine.rs)의 `acquired=c.now()`가 held snapshot과 동기 capture보다 **앞선 시각**임을 확인했다. 아래 실게임218.112/235.580ms 등의 `processed_at-acquired_at`에는 capture가 포함된다. 이전 응답/계획의 “캡처17ms + 판독220ms” 해석과17+220의 절약 비율 예시는 정정한다. 숫자 자체는 원래 필드 차이로 유지하고 아래 행의 이름을 반복 캡처 요청→판독 완료로 바로잡았다. capture 완료→순수 연산 시간을 이 과거 report에서 별도 복원했다고 주장하지 않는다.

`processed_at`은 정상 이동의 마지막 입력보다 앞서며, 가까운 적은 전체 plan을 기다리기 전에 이미 combat을 요청할 수 있다. 따라서 이 행의 시간이 모든 공격의 첫 keydown 시간도 아니다. 다음 capture가 전체 step 뒤에 온다는 반복 지연과 실제 input backend의 전송 시각을 분리해야 한다. 새 목표 약50ms와 WGC/GPU 보류를 반영한 CPU 구조 검토는 [REFACTOR-0004](../refactoring/refactor-0004-realtime-decision-latency.md)가 소유한다. 이번 정정은 runtime/시한/핫키/EXE 변경이 아니다.

## Final Verification

첫5개 공유 풀 경로 checkpoint 뒤 실제2회에서 남은 hazard/region/loot3개 OS thread 생성 경로를 발견해 추가 수정했다. 아래 전체 회귀와 고정 입력 시간은 이8개 경로를 모두 공유한 최종 코드의 결과다.237개 production Rust source/manifests/toolchain 지문은 `7d56f7185fb5f2b1b1bc547aa8dd10d15f132cb2750704c33c8a8acd6cff9324`다(정렬한 `relative-path:lowercase-SHA256`+LF의 UTF-8 SHA256). 최종 EXE는16,670,208B/SHA256 `92ee301f0fde68e4bc1dda880eb681c31bbc636022d033b5fa5232e90779d990`다. 이전5개 경로 후보의 소스 지문/실게임은 RUN에 역사로 남기며 최종 반복 실게임 결과는 아래 별도로 기록했다.

- fmt/check/all-target Clippy `-D warnings` 모두 exit0. 보통 Cargo target/cache를 유지했고 Windows hard-link 환경의 경고를 피하기 위해 process-local `CARGO_INCREMENTAL=0`을 썼다.
- 동일 static-CRT release의 serialized all-target 전체 packet은55 result groups/310 pass/0 fail/6 ignored,580.57초다. lib91, config10, background5/motion4, MPEG-4독립 decode5, vision18, full observer/runtime/session/cancellation을 포함한다. 수치 회귀는 모든 uint8 HSV/Lab, ORB source keypoints/descriptors, tiny/odd/large Q8/blur, FFT score/동점과 현재 참조를 검사한다.
- ignored native4개는 별도 명시 실행해 전부 통과했다: owned fixture capture/GDI 균형/listener 시작·unhook(입력 없음)3개1.20초, readonly desktop capture1개0.53초다. 남은 ignored2개는 성능 profiler이며 일반 pass 수에 포함하지 않는다.
- 첫5개 경로 후보의 전체 packet은 stale1e300 runtime-warning 기대에서183 pass/1 fail/5 ignored 뒤 중단됐다. 위 phase별 assertion 수정 후 당시 전체를 다시 통과했으며 첫 실패나 미실행 후속 target을 숨기지 않는다. 그 수정은 test만 바꿔 당시 EXE는 같았다. 이후 추가3개 경로는 새 EXE/전체 packet으로 위처럼 다시 검증했다.

아래에 최종 동일 입력 시간·메모리·실게임 결과를 기록한다. 자동 회귀/한정 실행은 통과했지만 모든 단계에서 Rust의 Python 대비 우위, 실제 게임의 낮은 tail latency와 전체 정상 마감은 달성하지 못했으므로 효율 수용은 계속 IN PROGRESS다. 전체 처치/key·10연속 새 방·27/30·장시간/새 OS·사람 외관 수용을 이 한정 검사로 채우지 않는다.

## Portable Package

최소 후보는659파일/23,702,450B, 공용 assets291파일/4,967,268B와 normal dependencies161개의 고지를 포함한다. 변경 전923파일/474,641,839B에서 약95.0% 줄었으며 원본 assets/Python/config JSON은 지우지 않았다. 개발 references 전체 대신 non-reference 공유 자산+실제 loading/exit2개 reference+config-selected 자산을 포함한다. 수학적으로 최소인 개별 crop 집합이라는 주장은 아니다.

최종 `dist/d2bot-windows-x64/`로 교체했다. 이전 폴더와 후보의 manifest 전체를 먼저 검증하고 실행 process가 없음을 확인한 뒤 이동했으며, 교체 후658개 payload의 크기/SHA256·경로 포함·중복/추가 파일 없음과 총659파일/23,702,450B를 재검증했다. 자체 CWD에서 help/diagnose와 실제 loader3캐릭터/config 경로5개가 다시 통과했다. 최종 EXE의 invalid optional 설정4개도 경고/기본값·exit0/입력0으로 처리됐다. task가 만든 영상/캡처는 배포에 포함하지 않는다.

필수 실제 loader3캐릭터/config asset5가 초기화되고 필수 journal이 없으면 packaging이 실패한다. PE imports19개가 Windows/system API이며 Python/OpenCV/FFmpeg/Xvid/CRT DLL 의존성은 없다. 개발 계측용 HeapSummary/K32GetProcessMemoryInfo도 production imports에 없다. wide/safe_arch/Rayon/crossbeam exact-version 고지를 포함한다. Windows API/OpenGL 드라이버까지 Rust로 구현했다는 의미는 아니다.

자체 CWD/Windows만 PATH인 별도 process에서 help/diagnose와 개발 명령5개 help가 exit0, 실제 GUI/PNG 저장/종료0과 CLI의 DRY-RUN/RUNNING+현재 게임 pixels/Escape/종료0을 확인했다. 최종 PNG는1922×1140/3,859,393B이고 offline observation도0/inputs_sent0다. 선택 화면의 HUD unverified는 올바른 미확인 결과다. 이 장비의 isolated process 검증이며 새 Windows VM 인증이 아니다.

## Final Matched Timings

각 칸은 p50 / p95 ms다. 위 측정 계약의 최종 순차 비교이며 앞선 탐색 checkpoint를 대체한다.

| Stage | 이전 Rust | 최종 Rust | Python/OpenCV |
| --- | ---: | ---: | ---: |
| native_normalize_gray | 0.920 / 1.218 | 0.950 / 1.501 | 0.320 / 0.425 |
| preview_scale_0_7 | 4.141 / 4.477 | 4.275 / 4.873 | 0.499 / 1.096 |
| hud | 3.678 / 4.416 | 3.655 / 4.550 | 2.084 / 2.724 |
| path | 18.442 / 19.474 | 18.772 / 20.803 | 26.808 / 28.031 |
| orb_pair | 44.196 / 45.954 | 15.409 / 16.270 | 13.349 / 14.969 |
| panels | 19.148 / 21.862 | 19.062 / 20.871 | 4.424 / 5.031 |
| camera_ground | 3.261 / 3.649 | 2.487 / 2.923 | 7.052 / 7.920 |
| near_hazards | 16.028 / 23.532 | 12.137 / 13.744 | 14.325 / 16.605 |
| hover_route_aim | 20.528 / 22.933 | 20.498 / 21.831 | 10.136 / 11.793 |
| near_hazards_route_aim | 19.084 / 21.728 | 15.623 / 18.202 | 15.205 / 16.835 |
| terminal | 95.796 / 105.347 | 83.410 / 90.711 | 97.895 / 107.619 |
| objective | 74.925 / 80.492 | 58.627 / 62.821 | 71.412 / 81.518 |
| hub | 15.593 / 19.129 | 16.282 / 23.070 | 4.608 / 5.663 |
| legacy_hub | 14.216 / 15.060 | 14.736 / 15.788 | 3.858 / 4.612 |
| stored_packet | 64.716 / 67.490 | 39.035 / 44.076 | 43.318 / 50.028 |
| encode_write | 63.704 / 71.541 | 2.141 / 12.614 | 5.019 / 9.333 |
| live screen capture | 16.863 / 30.993 | 16.988 / 32.229 | 16.917 / 32.146 |

ORB는 약65.1%, stored packet은 약39.7% 줄었다. 경로·terminal·objective·반복 camera·aim 없는 근처 hazard 판독은 이 입력에서 Python보다 빠르지만 ORB, normalize/gray, preview, HUD, cold panel, aim 포함 hazard/hover/hub는 아직 느리다. Python은 해당 작업을 native OpenCV에 위임한다. 영상 변환과 자료 배치의 추가 최적화 여지가 남았으며 모든 항목의 우위를 달성했다고 하지 않는다. 앞선5개 경로 후보의 ORB19.046ms는 RUN의 checkpoint이며 최종8개 경로 후보는15.409ms다. 동일 알고리즘 단계의 시간 차이 전부를 마지막3개 수정의 단독 효과로 돌리지는 않는다.

최종 live capture target은 left1914/top586/1922×1140이다. 변경 전/후8초 녹화의 실제 stop+file finalize+capture close+worker join은54.511/3.779ms, request는0.0009/0.0061ms였고 running_after_finish=false다. 최종 실제 경과 시간은8,003.989ms, live 파일은4,032,606B다. 최종 파일도59/59프레임 독립 decode/seek/native 크기/8fps를 통과했다. nominal8fps/encoded7.375초이며 wall-clock 초당8장 보장은 아니다. 장면 순간이 달라 live 파일 크기를 압축률 동등 비교로 쓰지 않는다.

동일 static33프레임은 이전802,700B/최종764,782B/Python1,066,882B다. 최종 파일도33/33 독립 decode/seek/native 크기/8fps를 통과했다. 이33프레임의 별도 PSNR은 측정하지 않았고 아래64프레임 corpus가 품질 증거다. 최종 static file finish는0.971ms다. 성능 profiler2개도 별도 실행해 통과했으며 FAST1.025/1.635ms, ORB all5.784/7.482ms, codec raw P6.744/8.348ms였다. profiler의 raw P는 output/metadata 준비도 포함해 실제 재사용 writer 시간과 같지 않다.


## Recording Quality And Size

동일1922×1140 64프레임/8초,8fps, 정지·프레임당3px wrap pan·8장마다 cut을 비교했다. q4는 실제 배포 기본값이다. Rust/Python 입력 SHA256이 패턴별로 같고 모든 q3/q4/q6와 XVID clip은64/64 독립 decode/seek/native 크기/8fps를 통과했으며 decoder stderr는 비었다. 독립 decoder는 개발 환경의 OpenCV/FFmpeg이고 배포에 포함되지 않는다.

| Pattern | Rust q4 bytes / XVID bytes | 비율 | Rust / XVID PSNR dB | Rust write p50 / p95 ms | XVID write p50 / p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| static | 1,084,604 / 2,115,434 | 0.513 | 36.406 / 36.153 | 2.172 / 12.609 | 5.089 / 9.026 |
| pan | 1,961,892 / 2,741,316 | 0.716 | 36.457 / 36.353 | 9.264 / 12.235 | 5.795 / 9.111 |
| cuts | 2,145,386 / 2,728,226 | 0.786 | 35.934 / 36.509 | 2.251 / 16.245 | 5.164 / 11.179 |

q4의 PSNR은 이 corpus에서 source보다1dB 이상 낮지 않고 write p95는125ms 미만이다. 파일은 정지48.7%/pan28.4%/cuts21.4% 작지만 pan은 Python보다 느리며 key/cut의 tail도 남는다. 평균/중앙값만으로 모든 프레임의 deadline을 보장하지 않는다. 정확한 반복 YUV skip은 재복원 잡음을 재압축하지 않아 static p50을 크게 줄였고, 움직이는 MB에는 실제 motion/DCT 처리가 필요하다. 이 구현은 제한한 MPEG-4 인코더이며 모든 Xvid 기능의 재구현/모든 영상의 용량·화질·속도 우위를 주장하지 않는다.

## Final GUI Memory Reproduction

첫5개 경로의8-worker/FFT scratch/pyramid/codec 후보에서 GUI terminal/objective80회 뒤 model/frame drop+worker join은 private124,055,552B/heap16,661,113B, working set140,148,736B다. 이전 같은 재현의511,582,208B에서 약75.8% 줄었다.

추가3개 경로까지 바꾼 최종 GUI hazard80회×3cycle은 매 cycle 새 outer worker/model/pool을 만들고 모두 종료했다. 시작/80회/model-frame-drop/실제 worker-join 표식이 각각3개이며 stderr0이다. worker join 뒤 private120,676,352→124,436,480→125,042,688B, default heap16,634,399→16,670,431→16,671,039B다. iteration80 peak는 private131,416,064/133,234,688/132,845,568B, heap24,222,063/24,258,095/24,258,399B다. 강제 heap/working-set 정리는 하지 않았고 native 그림/GPU를 포함한 GUI의 기본 footprint가 남는다.80회만으로 무제한 반복이나 실제 게임 전체의 누수 부재를 증명하지 않는다.

## Final Supervised Game And Recording

동일 계측 GUI process/최소 자산/Flash/Hell/screen backend로 실제 방2개를 실행했다. 시작은 Act2 캐릭터 선택이며 기존 reset 경로가 Act1을 거쳐 Arcane에 들어갔다. 두 번째는 수동 GUI 녹화를 병행했다. 방/적/경로가 달라 아래 차이를 녹화의 단독 효과로 해석하지 않는다. `acquired_at`은 위 정정처럼 capture 직전, `processed_at`은 마지막 입력 전의 판독 시각이며 게임 acknowledgement나 사람의 반응 시간은 아니다.

| 측정 | 첫 방: 녹화 OFF | 둘째 방: 녹화 ON |
| --- | ---: | ---: |
| Arcane 도착: entry start 이후 초 | 25.385 | 24.446 |
| 연속 판독 행 | 137 | 291 |
| 반복 캡처 요청→판독 완료 p50 / p95 ms | 218.112 / 246.975 | 235.580 / 273.266 |
| 반복 캡처 요청→preflight 완료 p50 / p95 ms | 199.811 / 229.880 | 217.533 / 256.044 |
| 연속 캡처 요청 간격 p50 / p95 ms | 225.820 / 256.932 | 242.075 / 311.908 |
| 캡처 요청 간격 min / max ms | 199.351 / 887.276 | 182.404 / 1,111.442 |
| stale_hold | 4/137 (2.92%) | 71/291 (24.40%) |
| sent key down / up | 51 / 51 | 70 / 70 |

첫 report `gui-20261024-093736-96cd41c2f4`는63.205초에 journal+6개 rune 근거로 단상을 판독했다. encounter는67.593초에 현재 단상 시체2회 근거(`current_platform_corpse_pair`, first dead sequence20)로 `kill_confirmed=true`를 보고했고, Nova dispatch는0이다. 이는 보고서의 비전 판정이며 플레이어 직접 처치·키 획득을 독립 입증하지 않는다. loot는2회 eligible 없음으로 완료/픽업0이며, 이후 `canyon finish deadline reached`로 전체 status는failed/구간0이다. 별도 survival guard의 focus_handoff는98.578–122.110초이고 Stop interruption이라는 최종 오류 표식은 없다. 소스의 deadline guard는 focus/stop 검사 뒤 실제 clock>=deadline에서 이 오류를 낸다. 협곡 완료나 새 방 성공으로 채점하지 않는다.

둘째 report `gui-20261024-094023-fa72fc0642`는 구간1/4 뒤 `terminal_reacquisition_required`/field_paused, errors=[]다. 현재3개 parts로 nonboss chest platform을 처음 읽었으나 끝에서는 terminal이 다시 미확인이다. objective/encounter 판정은 없고8개 전투 후 loot scan은 각2회 eligible 없음/픽업0이다.107.016–111.850초 focus_handoff 뒤 Stop했고 전체 완주가 아니다.

두 sent-key 순서 재생에 중복 down/짝 없는 up이 없으며 최종 held set은 비었다. 마지막 해제는 첫 F2@62.6448993, 둘째 F4@96.0367542초다. 두 번 모두 buff7개 dispatch, weapon-set balanced, effect_verified=false다. Stop 클릭+UI refresh의227/230ms는 실제 worker join latency로 사용하지 않는다. 캐릭터 HP1562/1580·회복된 mana1543/1543과 살아 있는 용병을 확인한 뒤 둘 다 저장 종료했고 마지막은 Act2 선택 화면이다. GUI/action/recording 종료 후 process exit0, stderr0이다.

같은 process의 최초 idle private72,376,320B/default heap18,342,425B에서 첫 종료 뒤 표본178.349초는95,592,448B/19,323,001B다. 둘째 녹화/실행 종료 뒤 표본370–374초는103,944,192B/20,555,924B였고 최종406.430초는105,852,928B/20,530,470B다. GUI 자원 drop 뒤91,799,552B/13,601,358B로 내려왔다. 전체 측정 peak는237.814초의private553,672,704B/heap443,509,395B/working set569,200,640B다.2초 간격 표본은 각 frame/job과 정확히 정렬되지 않으므로 peak 전부를 증거 저장이나 특정 계산 한 단계 탓으로 돌리지 않는다. 이전5개 경로 후보의 두 번 종료241→393MB, GUI drop384MB와 구별한다. 잔류를 크게 줄였지만 장시간/무한 반복의 누수 부재나 극한 효율 달성을 선언하지 않는다.

녹화는 보조 소비자이며 본체 성공 지표를 대체하지 않는다. 이 실행의 수동 AVI는76,989,990B/940프레임,8fps/encoded117.5초다.940/940 독립 decode와 처음/중간/끝 seek가 순차 decode와 같고 stderr0이다. 이 파일의 별도 원본 대응 PSNR/동일 입력 XVID 용량 비교는 하지 않았다.

## Same-Process Recording Contention Check

개발 example에 명시적 `--record-during`을 추가해 실제 RecordingSession을 판독과 같은 process/공유 pool에서 실행했다. 배포 EXE/production source 지문은 변하지 않았다. example의 fmt, warnings-denied Clippy, portable release build/test harness(검사0개)와 실제 OFF/ON 실행은 통과했다. 앞서 별도 GUI process가 켜진 OFF 탐색 측정은 이 비교에 포함하지 않는다.

동일 저장 PNG/30sample/3warmup을 OFF 다음 ON으로 순차 실행했다. 게임 입력은0이고 BGR/gray/floor 지문, HUD 전체 객체와 motion 전체 객체는 정확히 같다. 녹화는 현재 선택 화면의 애니메이션을 읽었고 게임 제어 전체나 움직이는 전투를 재현하지 않는다.

| 판독 | OFF p50 / p95 ms | ON p50 / p95 ms |
| --- | ---: | ---: |
| HUD | 3.677 / 4.409 | 3.310 / 3.728 |
| path | 18.613 / 21.730 | 18.511 / 21.111 |
| ORB pair | 18.990 / 20.447 | 19.650 / 22.024 |
| panels | 19.050 / 20.217 | 19.508 / 23.077 |
| camera ground | 2.811 / 3.399 | 2.975 / 4.230 |
| near hazards | 18.444 / 25.298 | 17.048 / 20.917 |
| near hazards + aim | 19.137 / 21.972 | 20.146 / 23.516 |
| terminal | 90.514 / 95.812 | 94.633 / 99.128 |
| objective | 65.728 / 73.217 | 69.022 / 72.439 |
| stored packet | 40.832 / 45.964 | 41.427 / 44.901 |

녹화 병행 시간14,662.386ms, stop+file finish+capture close+실제 worker join8.136ms, running_after_finish=false다. 파일6,970,644B/107프레임은 독립 decode/세 위치 seek/native1922×1140/8fps를 통과했다. 이 조건의 packet 중앙값 증가는 약1.46%, terminal 약4.55%다. 일부 ON 수치가 더 빠르고 OFF ORB18.990ms가 앞선15.409ms와도 달라 host 스케줄/상태 변동이 있다. 단 한 순차 pair로 인과 효과나 모든 장면의 녹화 overhead 상한을 확정하지 않는다. 실제 둘째 방의24.40% stale을 녹화 하나 탓으로 돌리지 않고 monolithic 판독 대기와 장면별 비용을 후속 설계에서 다룬다.

## Remaining Direction

주 목표는 화면→판단→입력의 반응과 안전한 중단이며 영상은 검증/개선 자료다. 소비한 제어 자료는 회수하고 필요한 진단만 별도 제한 보관한다. capture를 포함한 약220–270ms 반복 판독은 capture 교체만으로 해결되지 않는다. [이전 구조 조사](../refactoring/refactor-0003-rust-efficiency.md#real-time-architecture-exploration--proposed-not-implemented)의 WGC/GPU 후보는 사용자 최신 지시로 보류하고, [CPU first/약50ms 검토](../refactoring/refactor-0004-realtime-decision-latency.md)를 우선한다. cadence/관찰 유효성과50ms aim settle 충돌은 검토 전 실행 승인으로 취급하지 않는다. 협곡 마감과 terminal 재획득 실패도 전체 플레이 수용에서 남는다.

최종 문서 UTF-8/로컬 링크·앵커와 fmt/whitespace 검사를 통과했다. 원본 tracked Python/config JSON/공유 이미지·requirements/pyproject는 불변이다. 작업 프로세스가 모두 종료한 뒤 production source237개와 배포 payload658개를 다시 확인했고, task-owned 임시 자료·이전 배포 backup 약1.07GB를 정리했다. 현재 배포와 보통 Cargo cache/output은 유지하며 이 문서는 임시 파일을 참조하지 않는다.

## Package Asset Audit And Coverage

2026-10-24 구조 검토에서 축소 이유를 다시 확인했다. 이전 packager는 `assets/private/`를 제외한 공용 assets를 참고/회귀용 `references/`까지 포함했다. 당시 공용 assets456,034,116B는 [이전 실행 기록](../run/run-20261024-01-rust-runtime-efficiency.md)의 원본 inventory이며, 이전 패키지 총923파일/474,641,839B와 manifest 검증은 그 기준에 속한다. 삭제한 이전 pack을 현재 디스크에서 다시 읽은 per-folder inventory라고 주장하지 않는다.

| 구분 | 이전 기준 / bytes | 현재 배포 / bytes |
| --- | ---: | ---: |
| assets/regions | 421,556,772 | 1,780,641 |
| assets/ui | 30,307,178 | 2,983,330 |
| assets/character | 3,923,343 | 65,449 |
| assets/items | 242,021 | 131,922 |
| assets 루트 안내/정책(이전은 합계에서 계산) | 4,802 | 5,926 |
| assets 합계 | 456,034,116 | 4,967,268 |
| EXE·config·고지·manifest 등 나머지 | 18,607,723 | 18,735,182 |
| 전체 패키지 | 474,641,839 | 23,702,450 |

당시 큰 부분은 전체 화면 PNG reference다. 현재 source와 package의 경로 비교에서 빠진 공용 reference는281파일/451,067,972B다. 그중 Arcane `regions/arcane-sanctuary/remastered-ko-1922x1140/references`는176파일/347,793,197B, Rogue Encampment references33,410,393B, Lut Gholein25,741,544B, UI survival16,454,807B, Canyon12,830,997B다. 이 값은 현재 source의 누락 reference 집계다. 공용 원본 이미지와 replay 자료는 source에 유지했다. private는 원래도 pack에서 제외했으므로 private 용량을 이번475→23.7MB의 원인으로 세지 않는다.

현재는 non-reference 공용 자산289파일/2,691,170B를 모두 유지하고 실제 RoomTemplates가 읽는 `ui/town-loop/remastered-ko-window/references/loading-window.png`695,399B와 `exit-menu-window.png`1,580,699B를 예외로 포함한다. 두 reference는 현재 code에서 전체 image template로 읽으므로 제거하면 해당 방 흐름이 깨진다. config-selected viewport JSON과 key/rune/gem 이미지도 유지했다. EXE는16,566,784→16,670,208B로 오히려 조금 늘었고161개 의존성 고지는 남아 있다. 수학적으로 가장 작은 crop 집합이라는 주장은 아니다.

이 재감사에서 현재658payload의 포함 경로/중복/bytes/SHA256·extra 없음과291assets의 source 바이트 일치를 다시 확인했다. 자체 CWD/Windows만 PATH의 diagnose exit0, 실제 loader3캐릭터/config asset5개 초기화 exit0/captures0/inputs0도 다시 통과했다. 초기화 목록은 Room/Town/Panel/HUD/Automap/Gem/Loot/Arcane path·hazard·hover·target·hub·terrain·terminal·objective·Summoner/Canyon/portal을 포함한다. 이전 packaged GUI/CLI/PNG/녹화와 제한 실게임 증거는 위에 유지한다. 따라서 현재 설정/등록 runtime 경로의 필요한 파일을 빼서 크기를 줄인 것은 아니다. 이 검사는 전체 게임 성공이나 미래 임의 설정/새 모드의 포괄 보증은 아니다.

가용성 검사의 한계도 구분한다. 현재 farm `templates=[]`인 legacy CLI는 실제 preview smoke는 있지만 asset-checker가 TemplateMatcher를 직접 초기화하지는 않는다. 사용자가 별도 TownVision calibration bank나 `arcane-navigation-replay`의 manifest/calibration/이미지를 지정하면 그 입력도 별도로 제공해야 한다. 기본 pack에는 개발 reference corpus가 없으므로 source checkout 또는 `-IncludeDevelopmentAssets`로 만든 개발 pack/외부 corpus가 필요하다. 임의 새 config/loader는 packing의 명시한 loader·reference 목록과 실제 경로 검증을 확장해야 한다. 이번 review에서 코드·현재 패키지를 변경하거나 새 실게임 성공을 주장하지 않았다.
