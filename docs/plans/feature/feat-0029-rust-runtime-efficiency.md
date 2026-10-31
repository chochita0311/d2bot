# FEAT-0029: Rust 계산·메모리·배포 자원 효율

## Metadata
- ID: `feat-0029`; Status: `in-loop`; Type: `product`; Surface: `mixed`; Date: `2026-10-24`.
- Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Profile: `fullstack-product`; Evaluators: Contract, Functional.

## Goal And Authority

사용자가2026-10-24 ORB/녹화/메모리의 원인 조사·개선·실게임 재검증을 지시했다. 안전성을 최우선으로 하고 동일 목적 안의 자료/설계 변경과 불필요한 메모리 회수, 공통 최소 배포 자산을 승인했다. Python 대비 모든 항목의 속도 향상은 목표이며 검증 전 보장하지 않는다.

## Scope And Acceptance

[REFACTOR-0003](../refactoring/refactor-0003-rust-efficiency.md)의 기준/범위를 따른다. 원인별 전후30/3 측정, ORB source/descriptor·codec 독립 품질/파일·오류·취소/실제 종료, 반복 메모리 수명과 실행 자산 closure, 최종 패키지/실게임을 확인한다. 입력/판정 시한·핫키·화질을 완화해 성능을 만들지 않는다. unsafe 계산/C 코덱·게임 내부 접근/원본 자산 삭제는 제외한다.

## Harness Trace

- Spec: [SPEC-0029](../spec/spec-0029-rust-runtime-efficiency.md).
- Run: [RUN-20261024-01](../run/run-20261024-01-rust-runtime-efficiency.md).
- Evaluation: [EVAL-0032](../evaluation/eval-0032-rust-runtime-efficiency.md).
- 이전 [FEAT-0028](feat-0028-rust-windows-distribution.md)의 실행 가능성과 사람 전환 수용은 별도 상태로 유지한다.

## Current Checkpoint

2026-10-24 동일 수치/품질을 유지한8개 반복 계산 경로의 공유 pool·ORB/codec/자산 개선을 배포 폴더에 반영했다. 전체 회귀·실제 GUI/CLI/녹화·반복 메모리·두 방 검사의 수치/실패는 EVAL-0032가 소유한다. 메모리 잔류와 일부 계산은 크게 줄었지만 실제 녹화 병행 방의 tail latency/terminal 재획득, 협곡 마감, 모든 단계의 Python 대비 우위는 미해결이다. 상태는 계속 in-loop이며 WGC/GPU/빠른 제어 분리의 후속 구조는 REFACTOR-0003의 조사 제안이다. 녹화는 본체의 실시간 판단/입력을 보조한다.
