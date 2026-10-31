# EVAL-0015: Rust 기반 빌드와 설치 경계

## Metadata
- Run: [RUN-20261022-01](../run/run-20261022-01-rust-native-build.md); latest Attempt: `2`.
- Feature: [FEAT-0016](../feature/feat-0016-rust-native-build.md); Spec: [SPEC-0015](../spec/spec-0015-rust-native-build.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `infra-devtool`; Evaluators: Contract, Functional; Date: `2026-10-22`.

## Contract
- Result: `PASS` for inspected source/config/document contracts.
- Evidence Coverage: `complete` for FEAT-0016; Acceptance impact: `not applicable` after Attempt2 linked command smoke. Future port behavior remains out of this evidence scope.
- Cargo uses one edition2024 package with normal lib/binary discovery. Compiler1.99.1/host x64-MSVC is pinned; lock exists; target is ignored; no external Rust dependencies yet.
- Source denies unsafe, runtime unwrap/expect lints are denied, release retains unwind cleanup. App dispatch has only help/version/input-free diagnosis and explicit usage/I/O errors. No game/capture/input code is called by this boundary.
- Existing tracked Python/main/config/assets/tests/requirements/pyproject files have no diff. New source/guide/plan changes remain bounded to Rust bootstrap and tracking.
- 초기20개 문서/138개 상대 링크 검사 뒤, 갱신한 project guide/architecture·evaluation을 포함해 최종23개 문서/180개 상대 링크와 UTF8/U+FFFD/공백/계획 섹션 순서 검사를 통과했다.
- This review does not establish GC/ownership safety of future FFI code, future feature parity, GUI behavior, or live-input safety.

## Functional
- Result: `PASS`; Evidence Coverage: `complete` for FEAT-0016; Acceptance impact: `not applicable`; current blocker 없음. 아래 Attempt1 실패 이력은 보존한다.
- Installed rustc/cargo1.99.1, rustup1.29.2, rustfmt/clippy. `cargo fmt --check`, `cargo check --all-targets --locked` and `cargo clippy --all-targets --locked -- -D warnings` passed on current source.
- `cargo build --release --locked` failed because `link.exe` was not found. Microsoft Build Tools17.14.42 official installer passed its package hash check but failed starting the administrator process with1602/0x80070642. No VS installer/SDK installation was confirmed.
- Attempt1에서는4개 Rust unit tests와 release EXE smoke가 링크 부재로 미수행이었다. 사용자 UAC 재요청 뒤 Attempt2의 공식 Build Tools17.14.42/SDK22621 설치가 성공했고 vswhere complete/launchable·재부팅 불필요와 실제 SDK import library를 확인했다.
- Attempt2 결과:4개 Rust unit tests/0실패, release 링크, `target/release/d2bot.exe`의 version/diagnose/help와 invalid argument exit2 통과. default 진단 소스는 capture/input/network를 호출하지 않는다. 이 EXE는 bootstrap이며 전체 GUI/automation은 아직 Python이다.
- Python baseline outside sandbox: **765 tests passed in105.468s**. Initial sandbox run had6 report replacement permission errors; the affected main module42 tests passed outside sandbox in5.513s before the complete765-test rerun. These environment-specific failures were not hidden or repaired by changing Python behavior.
- Actual baseline distributions: opencv-python4.13.0.92, numpy1.21.6, mss7.0.1, keyboard0.13.5, pydirectinput1.0.4, black23.3.0.

## Remaining Gate
FEAT-0016의 gate는 Attempt2에서 통과했다. 사용자 전체 실행 위임에 따라 다음 설정 계약으로 진행한다. 전체 포팅과 Python/OpenCV 제거·최종 전환 수용은 미완료다.

## Cleanup
직접 생성한 task scratch는 없다. 링크 실패의 원인은 위에 보존했고 Cargo의 이번 task-owned 부분 빌드 산출물은 정확한 workspace target 경로와 활성 compiler 부재 확인 후 정리했다(119 files/2.9MiB). Rust 도구는 요청한 개발 환경으로 유지한다. vendor-managed installer/cache/log는 임의 삭제하지 않는다. 설치 로그의 경로·용도·복구 후 정리 조건은 run에 기록했다.
