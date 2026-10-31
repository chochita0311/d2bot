# PRD-0009: 기존 동작을 유지하는 Rust Windows 앱

## Metadata
- ID: `prd-0009`; Status: `approved`; Owner role: `human`; Created/Updated: `2026-10-22`.
- 승인 근거: 사용자가 Python/OpenCV 최종 제거를 확정한 뒤 “표준적이고 깨끗한 아키텍처”, 메모리 안전성/오류 처리 고려, feature 작성과 실행 파일까지의 작업 진행을 명시했다. 범위 안의 기술적 세부 결정/실행은 위임받았으며 새 사용자 동작·외부 의존성·호환성 변경은 이 승인에 포함하지 않는다.

## Request Summary
현재 설정·자산·시나리오·사용 흐름을 보존하면서 안전하고 유지 가능한 Rust Windows 앱으로 이전하고 실행 가능한 EXE를 검증한다.

## Source Set
- 사용자 요청: 이 대화의 포팅 목표/실행 지시, Rust 경험이 없으므로 관용적 구조·수명/자원/동시성 처리를 담당해 달라는 요구.
- [REFACTOR-0002](../refactoring/refactor-0002-rust-port.md), [정책](../../project/policy.md), [개발 지침](../../project/developer-guide.md), [현재 구조](../../project/architecture.md).
- 고정 소스: `cde68bc2eb1285eddcd8a261cdfe411ed4924f76`; 기존 Python/CV/설정·자산·테스트가 비교 기준. 현재 실게임 중단/복구는 기존 통합 run 소유.

## Product Intent
Rust 설치 지식이 없어도 제공한 명령과 Windows EXE로 실행할 수 있고, 소유권·오류·중단·자원 정리가 명확한 코드로 계속 개발한다.

## Confirmed Scope
- `2026-10-24` 후속 지시: 안전성 우선 뒤 ORB/직접 녹화/메모리의 원인과 효율을 개선하고 실게임까지 재검증한다. 같은 목적 안의 자료/설계 변경과 불필요한 메모리 회수·공통 최소 runtime assets를 승인했다. [FEAT-0029](../feature/feat-0029-rust-runtime-efficiency.md)가 추가 실행 경계이며 모든 항목의 속도 우위를 검증 전 보장하지 않는다.
- 최종 배포에서 Python/OpenCV 제거; 기존 Python은 개발 비교용 유지.
- 동일 저장소·루트 Cargo/src 단일 패키지와 기존 역할별 모듈을 기본으로 하되 실제 의존성·소유권에 필요한 경계만 추가.
- 기존 설정·자산·활성 Summoner·독립 action·GUI/CLI/도구·녹화·중지/실패 의미의 이전과 비교.
- 안전한 Rust를 기본으로 하고 OS 호출 unsafe 격리, 소유 프레임/공유 불변 frame, 명시적 취소/종료, held 입력 독립 해제, 제한한 diagnostic 보관을 검증.
- 공식 Windows Rust/MSVC 환경 준비, 고정 빌드·회귀·release EXE 검증과 문서.
- 사용자 후속 결정: 녹화 AVI/무음/8fps/native 크기를 유지하고 MPEG-4 압축기도 Rust로 직접 구현한다. Xvid C/FFmpeg runtime을 채택하지 않으며 기존 XVID 대비 실제 용량·화질·시간을 보고한다.
- 사용자 후속 결정: 잘못된 버프 대기시간은 원인을 확인하고 실행 전 보정해 계속 실행한다. NaN/무한대/오버플로·음수·변환 불가의 선택적 override는 제거하고 소비자의 기존 기본값을 사용하며 경고를 남긴다. 다른 필수 정책/관찰 값을 안전하다고 추측해 자동 보정하지 않는다.

## Excluded Scope
- 기존 Python 삭제, gameplay 알고리즘 개선·임계값 완화·새 포맷/외부 codec 무단 대체, 현재 실게임 복구 승인 변경.
- 메모리/패킷/주입/후킹/게임 우회; OpenCV 전체 범용 구현; 무조건 단일 EXE 내장·성능 향상 보장.

## Uncertainty
- ORB/RANSAC·직접 Rust MPEG-4와 GUI의 지정 비교/DPI 경계는 각 feature에서 통과했다. 최종 dependency audit·전체 회귀·배포 EXE의 실제 Windows 실행·제한한 실게임/성능 검사를 완료했으며 [배포 run](../run/run-20261023-09-rust-windows-distribution.md)과 [평가](../evaluation/eval-0031-rust-windows-distribution.md)가 증거를 소유한다. 실제 전체 게임 성공, 새 Windows 이미지·미검증 GPU/DPI와 최종 사람 전환 수용은 미확인이고 이 비교 통과와 구분한다.
- codec/GUI/실제 입력 경계의 불확실성은 해당 feature 전 해소한다. 독립한 환경·설정·순수 계약은 진행 가능하다.

## Constraints
- 기존 Python 기준과 파일/설정/자산을 보존한다. 동등성은 구현·실행 근거 전까지 `Unknown/Provisional`.
- Rust는 GC 없이 ownership/borrowing/Drop을 사용한다. 이를 논리·교착·외부 OS 실패의 부재 보장으로 설명하지 않는다.
- 사용자 위임 범위에서 feature/spec/run을 순차 진행하며, 실제 game 입력은 기존 사람 복구 전제와 새 화면 확인을 필요로 한다.

## Acceptance Envelope
유지 대상 전체 계약의 비교와 실제 Windows 실행이 끝나고 Python/OpenCV 없는 배포 EXE가 GUI/CLI·기록·중단을 수행한다. build-only, 빈 버튼 또는 일부 모듈 이전을 전체 포팅 완료로 기록하지 않는다. 자동 평가 결과와 최종 사람 수용은 구분한다.

## Candidate Features
- [FEAT-0016](../feature/feat-0016-rust-native-build.md): 재현 가능한 Windows Rust 빌드.
- [FEAT-0017](../feature/feat-0017-rust-config-contract.md): 기존 JSON 설정과 선택 계약.
- [FEAT-0018](../feature/feat-0018-rust-frame-geometry.md): 분석 프레임과 물리 좌표 계약.
- [FEAT-0019](../feature/feat-0019-rust-vision-operations.md): 프로젝트가 쓰는 영상 연산.
- [FEAT-0020](../feature/feat-0020-rust-background-motion.md): 기존 ORB 기반 배경 이동 추정.
- [FEAT-0021](../feature/feat-0021-rust-windows-io.md): Windows 캡처와 중단 가능한 입력.
- [FEAT-0022](../feature/feat-0022-rust-character-policy.md): 캐릭터 정책·관찰·필드 상태.
- [FEAT-0023](../feature/feat-0023-rust-actions-lifecycle.md): 방 lifecycle·조합·일반 줍기.
- [FEAT-0024](../feature/feat-0024-rust-summoner-runtime.md): 현재 Summoner 실행 흐름.
- [FEAT-0025](../feature/feat-0025-rust-desktop-control.md): 기존 조작 흐름의 Rust 제어판.
- [FEAT-0026](../feature/feat-0026-rust-recording-contract.md): Python/OpenCV 없는 녹화·증거 저장.
- [FEAT-0027](../feature/feat-0027-rust-cli-replay.md): CLI preview·관찰·재생 도구.
- [FEAT-0028](../feature/feat-0028-rust-windows-distribution.md): Python/OpenCV 없는 Windows 실행 배포.

## Continuity Notes
- `2026-10-24`: 지정 기능·자동 회귀·배포 EXE 실행 검사를 완료해 구체 실행파일을 제공한다. 기본 실행 전환의 사람 수용을 아직 받지 않아 PRD/FEAT-0028은 `in-loop`를 유지한다. 게임 알고리즘 개선·Python 삭제·원본 게임 수용 실적 변경은 하지 않았다.
- `2026-10-22`: 기존 planning-only 요청에서 전체 실행 위임으로 경계가 바뀌었다. FEAT-0016을 첫 실행 대상으로 선택하며 나머지는 계약을 구체화한 뒤 실행한다.
- `2026-10-22`: 사용자가 잘못된 시간값을 시작 오류로 거부하는 제안 대신 원인 제거/처리 경계의 보정과 실행 지속을 지시했다. 선택적 버프 대기시간의 기본값 복귀를 승인한 의미 변경으로 기록한다.
