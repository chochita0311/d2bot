# REFACTOR-0003: Rust 실행 효율과 자원 수명

## Purpose

사용자2026-10-24 지시에 따라 크래시 방지·메모리 안전성을 우선하고, ORB/직접 MPEG-4/메모리 잔류의 원인을 측정해 개선한다. 기존 포팅의 실행 가능성과 효율성 수용을 구분한다. 필요한 자산은 목적별로 공유하고 개발 참고 자료를 배포에 일괄 포함하지 않는 경계를 검증한다.

## Refactor Type

- Structural 중심의 성능·수명 개선. 동일 목적을 달성하는 자료/설계 변경은 이번 사용자 지시로 승인됐으며, 게임 판정·입력 정책 변경과 화질 축소는 포함하지 않는다.
- 이전 트랙: [REFACTOR-0002](refactor-0002-rust-port.md), [EVAL-0031](../evaluation/eval-0031-rust-windows-distribution.md). 이전 구현/배포 증거는 유지하고 이번 변경 전 성능으로 취급한다.
- Rust 기준: 변경 전234개 Rust/manifest/toolchain 파일의 SHA256 집계 `0b6b93652f3b3d43df84f1857c8bde770de6ade28bd3378d29add2d13e97da59`; EXE `63df92fb6104116a0e793ecf9ab5f2dac200ce94bd0560b87e243c21b2180534`. 기존 Python 기준은 `cde68bc2eb1285eddcd8a261cdfe411ed4924f76` 그대로다.
- 상태: `in-loop`; 전체 게임 parity는 계속 `Provisional`. 하위 [FEAT-0029](../feature/feat-0029-rust-runtime-efficiency.md)가 현재 실행 owner다.
- 현재 checkpoint:8개 반복 계산 경로/ORB/codec/자산 개선의 전체 회귀·배포·실게임2회와 녹화 병행 계측은 [EVAL-0032](../evaluation/eval-0032-rust-runtime-efficiency.md)에 기록했다. 일부 속도/메모리 개선은 입증됐으나 모든 Python 대비 우위·실제 낮은 tail latency·전체 게임 마감은 미달이다. 아래 구조 조사는 구현 승인을 대체하지 않는다.

## Scope

ORB 특징 추출·연속 프레임 소비, MPEG-4 변환/버퍼/비트 쓰기, 관찰 scratch·worker·증거 수명, 패키지의 실행 자산 의존성과 공통 자료 경계를 다룬다. 최적화 전후 같은 입력·설정·품질로 비교한다.

## Non-Goals

Python/OpenCV/FFmpeg/C 코덱 재도입, 새 gameplay 알고리즘·핫키·시한 완화, unsafe 수치 최적화, 화질/해상도/FPS 축소로 얻는 속도, 사용자 원본 자산 삭제, OS 메모리 숫자만 줄이는 working-set 강제 축출은 하지 않는다. 모든 단계가 Python보다 빠르다는 결과를 미리 보장하지 않는다.

## Invariants

입력/결과의 판정·오류·중단·geometry/freshness 경계를 유지한다. ORB의 특징점/정렬/descriptor와 기존 허용 오차, AVI/native 크기/무음8fps/GOP/독립 재생·품질 기준, 정지 시 파일 마감·키 해제·실제 worker 회수를 검증한다. 공유 frame은 불변이고 불필요한 strong 참조는 작업 종료에 정리한다. 필요한 고지·실행 자산은 배포에 남긴다.

## Planned Work

### Step 1 — 원인 계측

Goal: ORB/코덱 단계와 살아 있는 메모리·할당기 보유량을 구분한다.
Why this grouping: 언어 이름이나 작업집합만으로 병목·누수를 판단할 수 없다.
Guardrails: 입력 없는 저장 자료,30회/3 warmup, 독립 프로세스 기준. 자체 프로세스만 읽고 heap 강제 정리는 하지 않는다.
Targets: 개발 benchmark와 좁은 Windows 자원 계측, 기존 ORB/codec private stages, 자산 의존성.
Validation: 입력 지문·판정 일치, 단계 시간·heap allocated/committed·작업집합/전용 commit·할당/참조 수명.
Exit gate: 관찰과 원인 추론을 구분한 증거로 다음 수정 대상을 정한다.

### Step 2 — ORB/녹화 계산과 임시 버퍼

Goal: 불필요한 영역 검사·동일 계산·반복 할당/복사를 줄인다.
Why this grouping: 동일 수치/파일 결과와 비교할 수 있는 실제 병목이다.
Guardrails: FAST NMS 이웃·mask/border·동점과 ORB 순서를 보존한다. codec 변환 변경은 독립 decode/PSNR/용량/속도 검사를 통과해야 한다.
Targets: scene_motion/orb, recording/mpeg4와 직접 소비 경계.
Validation: 기존 Python oracle 특징/descriptor/동작, MPEG-4 정지/이동/컷/홀수/오류/마감 회귀와30/3 비교.
Exit gate: 실제 개선과 안전성/품질 증거가 있는 변경만 채택한다.

### Step 3 — 자원 수명과 필요한 배포 자산

Goal: 종료 후 불필요한 buffers/worker/강한 frame 소유를 정리하고 필요한 자산만 배포한다.
Why this grouping: 계산 속도와 별개로 메모리 잔류와 배포 크기를 지배한다.
Guardrails: 진단 출력 의미·고유 증거를 조용히 삭제하지 않는다. 실행 파일의 모든 유지 경로에서 참조하는 자산의 closure와 고지를 확인하며 개발 원본은 유지한다.
Targets: 세션/관찰 수명, pattern/manifest 읽기, packaging owner.
Validation: 반복 후 메모리 곡선/실제 thread 회수, 필요한 참조 전수 확인과 최소 패키지 GUI/CLI/게임 smoke.
Exit gate: 라이브 데이터와 보유 메모리, 코드·개발 자료·실행 자산의 경계가 측정/검증된다.

### Step 4 — 통합·실게임

Goal: 개선한 EXE의 전체 회귀와 새 화면 이동/전투/Stop을 확인한다.
Why this grouping: 독립 계산 속도가 실제 동시 관찰 성능을 보장하지 않는다.
Guardrails: 이전 사용자 복구 확인을 반복하지 않고 새 장면/HUD/focus를 확인한다. 시작 위치는 이전 검사 후 Act2 캐릭터 선택 화면이며 현재 관찰을 우선한다.
Targets: 최종 패키지, GUI/CLI/녹화/입력 세션, 결과 문서.
Validation: fmt/check/Clippy/필요한 전체 release·native 회귀,30/3 성능/자원, 제한한 실게임·키 해제·안전 저장 종료.
Exit gate: 실제 개선 수치와 남은 제약을 제공하고 임시 자료를 정리한다. 기존 전체 게임 수용 실적은 바꾸지 않는다.

## Validation Gates

각 변경 전후 같은 입력 지문과 source/기준 Rust 결과를 비교한다. 동일 목적을 유지하는 구조 변경도 오류·취소·부분 파일·actual join 검사를 거친다. 숫자/특징/파일 품질이 틀리면 속도 향상으로 대체하지 않는다. 최종 binary의 실제 imports·고지·최소 assets·복사 hash와 자체 CWD 실행을 확인한다.

## Intentional Deltas

내부 자료 구조·작업 배치·메모리 소유·반복 계산과 배포 자료 선택은 사용자 승인한 효율 개선 범위다. 필수 runtime 자산/공통 모델의 읽기와 결과는 유지한다. Codec의 동일 byte는 요구하지 않지만 독립 재생·기존 품질/용량 gate는 유지한다. 새 판정/입력 의미 변경은 승인에 포함하지 않는다.

## Risks / Open Questions

OpenCV/XVID는 Python 밖의 최적화된 native 구현이다. 안전한 Rust 역시 알고리즘/자료 배치가 비효율적이면 느릴 수 있다. 메모리는 live allocations와 allocator/driver/OS 보유를 측정해 구분한다. 필수 자산 closure가 입증되지 않은 파일은 제외하지 않는다. 장시간/새 OS/전체 게임 결과를 제한한 검사로 확대하지 않는다.

## Exit Goal

안전성·같은 목적/품질을 유지하면서 측정된 병목과 불필요한 자원 점유를 줄인 실행파일을 제공한다. 원인, 전후 수치, 메모리 수명, 최소 자산 규칙과 남은 느린 항목/실게임 한계를 기록한다. 후속 알고리즘 개선은 별도 경계로 둔다.

## Real-Time Architecture Exploration — Proposed, Not Implemented

2026-10-24 사용자가 현재 작업을 계속하면서 subagent 조사와 공식 자료로 더 빠른 화면 수신/판단 구조를 탐색하도록 요청했다. 메모리 안전·overflow 방지·실제 자원 해제가 우선이며, 이후 주 지표는 자연스러운 게임 판단/입력 반응이다. 녹화는 검증·개선 자료다. 제어용 프레임/중간 결과는 소비 후 회수하고, 필요한 진단 증거만 별도 제한 보관한다. 녹화 on/off·느린 writer의 안전성/품질도 유지하되 인코딩 중앙값 개선을 본체 반응 개선으로 대체하지 않는다. 공통 기록·상한 계약은 [PRD-0008](../prd/prd-0008-realtime-runtime-and-recording-retention.md)이 소유한다.

### Current Evidence

Primary가 질문/범위를 정한 named read-only worker2개가 capture/판독 call path를 조사했고, Primary가 OS 공식 문서와 대조했다. 현재 GDI 경로는 파일 저장을 거치지 않고 매 grab마다 DC/bitmap 생성→동기 BitBlt/PrintWindow→GetDIBits→CPU BGR 변환을 한다.1922×1140은32bit 임시8,764,320B+BGR6,573,240B이며 GDI bitmap은 별도다. followed-window/layout 경로는 한 grab에 window enumeration4회로 geometry를 앞뒤 확인한다. native 크기는 normalize 시 borrow하고 다른 크기는 full/client resize가 추가된다. 근거: [Windows capture](../../../src/platform/windows/capture.rs), [common capture](../../../src/common/capture.rs), [normalization](../../../src/common/vision_ops/resize.rs).

실제 loop는 한 프레임을 on-demand 캡처하고 HUD→camera/panel 준비→ground→근처 적/hover→path·terminal·objective→motion/terrain→freshness/이동 입력으로 진행한다. 가까운 적의 combat 요청은 이미 plan 전에 가능하지만 다음 capture는 전체 step 뒤에 온다.3개 observation worker/queue3개가 일부 판독을 겹치며 outbound terminal/objective와 motion 결과를 정상 이동 전에 기다린다.2026-10-24 추가 source 검토에서 `acquired`가 capture 직전이므로 약220ms의 report 구간에 capture가 이미 포함됨을 확인했다. 이전17+220ms/7.2% 계산 예시는 이 시각을 잘못 분리한 것이어서 정정한다. 이 시간은 순수 판독 비용·모든 공격의 keydown·게임 acknowledgement와 같지 않다. capture p50약17ms뿐 아니라 p95약32ms도 고려해야 한다. 근거: [engine](../../../src/runs/summoner/continuous_navigation/engine.rs), [native vision](../../../src/runs/summoner/continuous_navigation/native.rs), [측정 정정](../evaluation/eval-0032-rust-runtime-efficiency.md#latency-timestamp-correction).

같은 불변 Arc의 panel memo와 path에 전달한 ground 재사용은 이미 있다. hazard는 플레이어/aim 주변 crop, target panel도 crop이지만 terminal은 넓은 viewport와 여러 template를 검사한다. ORB는 full frame gray/0.5축소와 마스크를 쓴다. 기존 동시 실행·crop·memo가 없다고 가정하거나 모든 판독을 항상 full frame으로 설명하지 않는다.

### OS Frame Providers

| 후보 | 제공 범위/이점 | 검증할 한계와 상태 |
| --- | --- | --- |
| GDI surface/CPU buffer 재사용 | 기존 acquisition/색상 기준 유지, DC·bitmap·BGRA/BGR 반복 할당 감소 후보 | 최신 불변 frame의 독자가 끝나기 전에 재사용 금지. GPU readback/판독 전체 비용은 남음. 현재 per-call surface를 바꾸는 후속안 |
| Windows Graphics Capture(WGC) | HWND로 한 창을 선택하고 FrameArrived로 GPU surface/시각/ContentSize를 받는 공식 창 프레임 스트림. 우선 prototype 권고 | 실제 D2R/window mode/GPU에서 지연·검은 화면·가림/최소화·색/HDR·resize·장치 손실을 측정해야 함. Rust에서 Windows API로 호출 가능하나 WinRT binding/지원 OS·shader/dependency는 아직 선택하지 않음 |
| DXGI Desktop Duplication | monitor별 GPU desktop surface, dirty/move regions·frame/pointer metadata | 창 전용이 아니므로 다른 창/desktop 좌표·모니터 변경을 처리. 화면 이동/애니메이션 때 dirty가 넓을 수 있어 이점 미보장. access-lost 재생성·유한 wait/Stop·ReleaseFrame 필요. 비교 후보 |
| 게임이 협력해서 제공하는 shared texture/API | producer가 공유 resource handle을 공개한다면 복사 감소 가능 | D2R가 제공하는 구독 API는 이번 조사에서 확인되지 않았다. 임의 PID만으로 타 프로세스 texture를 구독할 수 있다고 하지 않음. 내부 메모리/Present hook/DLL injection은 기존 프로젝트 경계 밖이라 채택하지 않음 |

공식 근거: [WGC overview](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture), [CreateForWindow](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow), [CreateFreeThreaded](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool.createfreethreaded?view=winrt-26100), [Desktop Duplication](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api), [AcquireNextFrame](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutputduplication-acquirenextframe), [shared resource의 생성/handle 조건](https://learn.microsoft.com/en-us/windows/win32/api/d3d11_1/nf-d3d11_1-id3d11device1-opensharedresource1), [Microsoft Rust binding](https://microsoft.github.io/windows-docs-rs/doc/windows/Graphics/Capture/struct.Direct3D11CaptureFramePool.html). HWND 지정은Windows10 1903+, CreateFreeThreaded는1809+다. 지원 OS와 실제 이 PC 성능 검증은 서로 다른 조건이다.

### Proposed Processing Direction And Gates

1. OS frame stream을 재사용 GPU texture에서 받고, 최신 유효 프레임 하나를 우선 소비한다. queue 길이를 늘려 평균 FPS만 높이지 않는다. sequence/방/이동/geometry/QPC 시각을 보존하며 늦은 결과를 최신으로 재명명하지 않는다.
2. GPU에서 crop/gray/색상 mask 등 병렬 화소 연산을 하고 필요한 작은 영역/결과만 CPU로 전달하는 후보를 비교한다. WGC를 쓰면서 매번 전체 영상을 즉시 CPU Map하면 복사·동기화 stall이 남을 수 있다. [Microsoft resource-copy guidance](https://learn.microsoft.com/en-us/windows/uwp/graphics-concepts/copying-and-accessing-resource-data), [nonblocking Map](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_map_flag). GPU 사용 자체의 속도/byte parity는 보장하지 않는다.
3. 생존/패널/근처 적·현재 이동 근거를 우선하는 빠른 판단과, 넓은 terminal/objective/경로 재탐색을 분리하는 안을 설계한다. 비용 큰 판독의 실행 조건/주기를 바꾸려면 현재 positive/unknown/freshness·중단 규칙을 새 계약에 명시하고 같은 장면 재생/실게임으로 검증한다. 기존250ms를 늘려 오래된 결과를 허용하는 방법은 아니다.
4. 녹화는 별도 bounded consumer로 두고 제어의 입력/해제·capture/판단을 writer 대기로 막지 않는다. 새 구조에서는 raw frame 공유와 normalize 분기를 비교하지만 현재 수동 녹화가 이미 제어 capture를 공유한다고 주장하지 않는다. 포화·누락·인코딩 오류·마감 상태를 명시하고 중요 최소 기록과 대체 가능한 영상/중간 이벤트를 구분한다.
5. CPU/GPU surface·Map/Unmap·COM/texture·event/callback·worker에 명확한 소유/종료를 두고 크기·pitch·곱셈 overflow/장치 손실을 검사한다. 소비 중 재사용/해제 금지, 종료 후 실제 callback/worker 회수, 새 frame이 없거나 device/geometry가 바뀌면 입력 해제를 먼저 검증한다.

평가 기준은 같은 PC/장면에서 capture 시작→판독→SendInput 제출의 p50/p95/p99/max와 frame age·stale 거부율, 다음 게임 화면의 반응, peak/반복 후 메모리·GPU 자원, 녹화 off/on/느린 writer의 차이다. 기존 `processed_at`은 최종 입력 전이므로 게임 acknowledgement로 쓰지 않는다. 예를 들어 p95 50–80ms를 prototype 목표 후보로 논의할 수 있지만 아직 확정 SLA/측정 결과가 아니며, 정확한 상한·기능/dependency 선택 후 실행 계약을 만든다. 이 절은 조사/후속 설계이고 WGC/GPU/판독 cadence 구현 승인을 대신하지 않는다. 진행 중인 동일 동작의 compute/resource 수정·검증은 계속한다.

## Handoff to Next Track

사용자가 이후 약50ms의 구조 검토를 요청하며 WGC/GPU의 비용 우려로 우선 보류했다. 위 OS 후보 비교는 이전 조사로 유지하고 CPU 중심의 현재 설계/측정 경계·50ms aim settle 충돌·단계별 실행 전 gate는 [REFACTOR-0004](refactor-0004-realtime-decision-latency.md)로 넘긴다. 새 baseline은 EVAL-0032의92ee EXE/7d56 production source이며 이 트랙의 원래 개선 전 기준과 섞지 않는다. 캐시/계산 공유와 새 관찰 cadence·조준 대기의 의미 변경을 분리하고, 현재 in-loop/전체 parity Provisional은 그대로다. 이번 handoff는 새 executable spec/feature 승인이나 runtime 변경을 발생시키지 않는다.
