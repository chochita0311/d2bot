# FEAT-0026: Python/OpenCV 없는 녹화·증거 저장

## Metadata
- ID: `feat-0026`; Status: `passed`; Type: `product`; Surface: `backend`.
- Execution Profile: `backend-product`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created/Updated: `2026-10-22`.

## Goal
필요한 AVI 저장·인덱스·마감과 MPEG-4 호환 압축을 Rust로 직접 구현한다. 기존 AVI/무음/8fps를 유지하며 기존 XVID 대비 크기·화질·저장 시간을 측정한다. PNG/JSON/JSONL 계약은 유지한다. MJPEG 실험은 용량 증가를 확인한 비교 자료다.

## Acceptance Contract
encoder/muxer 실제 feasibility, 독립 decoder 재생·size/color/fps/duration/마감/오류·파일 명세 비교; 최종 dependency audit.

## Scope Boundary
- In: native GUI 녹화,8fps encoder와 capture sampling 구분, 파일명/내용/마감/실패·현 cleanup.
- Out: 임의 MP4/PNG 대체, 미승인 외부 codec 채택, PRD0008 retention 선행 구현.

## Contract Surfaces
기존 Python의 해당 공개 동작/입출력과 [계획의 invariants](../refactoring/refactor-0002-rust-port.md#invariants)가 기준이다. Rust의 구현 타입·자원 소유권은 각 spec에서 명시하며, 모호한 값·fallback·운영 부작용을 추측하지 않는다.

## User-Visible Outcome
Python/OpenCV/FFmpeg/Xvid C 라이브러리 없이 녹화하고 기존 XVID 대비 용량·화질·속도를 확인한다. 호환 비트스트림과 압축 효율·수치 동일성을 구분한다.

## Entry And Exit
- Entry: 승인한 실제 호출/진단/테스트 경계에서 시작.
- Exit: 성공·보류·오류·취소를 구분하고 소유한 worker/handle/input을 정리한다.

## State Expectations
- 기본·정상·누락/불명·오류·취소 상태를 구분한다. 기존의 불명을 성공으로 바꾸지 않는다.
- 실제 게임 side effect는 해당 feature/spec의 명시한 경계 안에서만 검증한다.

## Dependencies
사용자가 MJPEG 비교 결과를 검토한 뒤 MPEG-4 압축기도 Rust로 직접 구현하는 방향을 명시했다. FEAT-0018/0021의 이미지·자원 경계를 재사용한다.

## Likely Affected Surfaces
src/actions/recording.rs, src/common/recording.rs, tests/.

## Pass Or Fail Checks
encoder/muxer 실제 feasibility, 독립 decoder 재생·size/color/fps/duration/마감/오류·파일 명세 비교; 최종 dependency audit.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0026](../spec/spec-0026-rust-mpeg4-recording.md); Run: [RUN-20261023-07](../run/run-20261023-07-rust-mpeg4-recording.md). 이전 MJPEG 실험은 [RUN06](../run/run-20261023-06-rust-avi-recording.md)이 소유한다.
- Execution profile: `backend-product`.
- Evaluation: [EVAL-0028](../evaluation/eval-0028-rust-mpeg4-recording.md), 저장/합성 화면·가짜 캡처 경계 PASS. 실제 게임 녹화·전체 앱 성능은 최종 배포 검증에서 수행하며 전체 포팅 완료와 구분한다.

## Continuity Notes
- `2026-10-23`: MJPEG80의6.6–10.3배 용량 증가를 보고했다. 사용자는 C Xvid 라이브러리 대신 MPEG-4 압축기도 Rust로 직접 구현하라고 선택했다. 원본 Xvid C/FFmpeg 코드를 runtime dependency로 채택하지 않는다.
- `2026-10-23`: 실제 source는 오디오 없는 AVI/XVID 저장만 사용하며 앱 내부 영상 재생·변환은 없다. 사용자는 FFmpeg 전체 대신 필요한 부분의 직접 구현을 요청했고, MJPEG 제안에 대해 우선 구현 후 실제 파일 크기를 알려 달라고 답했다. 이 한정된 변경을 실행하며 미측정 용량·속도나 최종 전체 포팅 성공을 주장하지 않는다.
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
