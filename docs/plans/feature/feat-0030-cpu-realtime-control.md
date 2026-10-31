# FEAT-0030: CPU 중심 최신 화면 판단·입력

## Metadata
- Status: `approved`; Type: `product`; Surface: `mixed`; Date: `2026-10-24`.
- Parent: [PRD-0008](../prd/prd-0008-realtime-runtime-and-recording-retention.md), [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `fullstack-product`; Evaluators: Contract, Functional.
- Plan: [REFACTOR-0004](../refactoring/refactor-0004-realtime-decision-latency.md); Spec: [SPEC-0030](../spec/spec-0030-cpu-realtime-control.md); Run: [RUN-20261024-02](../run/run-20261024-02-cpu-realtime-control.md).

## Goal And Authority

감독 시험의 복구는 기존 TP 금지를 유지한다. 빈 끝/중단은 기존 north return→중앙WP→Act1→종료, 서모너 지점은 현재 시체·일지/협곡 마감이다. 검증된 Act2 포탈→WP 코드 없이 TP/수동 보행으로 다음 시험을 준비하지 않는다. 이번 실행의 위반·수동 개입은 RUN/EVAL에 별도 기록한다.

사용자가 CPU 계획의 실제 구현,50ms 조준 대기의 원인 확인/안전 개선, 메모리 안전·자원 해제 우선, 녹화 병행 실게임과 전후 반응/메모리 비교를 명시 승인했다. 텔레포트 hold 중 마우스로 방향을 바꾸고 전투키와 전환하는 기능적 반응 요구이며 탐지 회피/은폐 목적이 아니다. WGC/GPU·외부 C/Python runtime은 보류/제외한다. Primary가 구조·의미·수명·통합·실게임 적격성을 소유하고 named worker는 한정 사실/검사만 제공한다.

## Scope And Acceptance

사용자의 최신 설명에 따라50ms는 기능·판독 품질·메모리/입력 안전성을 유지하면서 불필요한 계산·복사·대기·자원 점유를 최대한 줄이기 위한 지향 목표다. 달성 불가능할 가능성도 허용하며50ms 초과만으로 구현을 실패로 판정하거나50ms 도달만으로 최적화를 끝내지 않는다. 채택 기준은 동일 목적의 기능/판독 품질과 안전 경계, 실제 지연·CPU·메모리의 검증한 개선 및 유지 비용이다. 남는 비용은 현재 구조에서 제거 가능한 비용과 실제로 측정한 필수 처리/시스템 비용으로 구분하고, 아직 조사하지 않은 비용을 하드웨어 한계로 단정하지 않는다.

후속 사용자 지시는 안전성 확보 아래 구조 개선으로 실제 자원 읽기→판별을 최단화하는 것을 최우선으로 정했다. 최적화 뒤 기존 타이밍/threshold/실게임 동작의 재적응은 후속이며, 원래 행동의 완전 일치가 구조 개선을 막는 gate가 아니다. 최신 source·unknown 보존·유한 수명·입력 해제/overflow 경계는 계속 필수다. 저장 입력 parity는 결과 변화를 설명하는 증거로 사용하고 실제 속도·안전성과 별도로 보고한다. 기존 게임 마감/키 획득/전체 수용을 먼저 완성해야 구조 개선을 채택할 수 있다는 순서로 해석하지 않는다.

실제 input backend 반환과 원래 frame 시각을 연결한 계측, 동일 판독의 CPU 준비/ROI/비교 공유, 최신 프레임과 빠른/느린 판단의 실행 경계, 확인한 포인터에 대한 불필요한 고정 대기 개선, 독립 배포 자산 가용성을 구현·검증한다.50ms는 반응 목표이며 측정 전 보장하지 않는다. capture→실제 입력과 화면 변화→입력·게임 효과, 기존 hold/새 aim/전투/해제를 구분한다.

## Invariants And Intentional Deltas

고정50ms settling은 연속 입력의 실제 pointer 확인을 근거로 바꿀 수 있다. frame 선택/cadence/결과 적용의 변경은 명시 계약으로 검사한다. 기존250ms freshness·focus/geometry/중단·키 해제·숫자/overflow 경계는 완화하지 않는다. 늦은 negative/다른 세대의 결과를 clear로 쓰지 않으며 명확한 소유/유한 큐·byte 상한과 실제 종료를 유지한다. 녹화는 본체 입력을 writer 대기로 막지 않고 현재 화질/크기/8fps·독립 재생 기준을 유지한다. 원본 Python/config/assets를 지우거나 기존 game 수용 실적을 바꾸지 않는다.

## Entry And Exit

기준은 EVAL-0032의92ee EXE/7d56 production source다. 먼저 의미를 바꾸지 않는 계측 baseline을 고정하고 같은 입력/환경에서 전후를 비교한다. 모의/저장 자료의 실패·취소·포인터 지연·late work 재개 검사를 통과한 후 기존 승인된 감독 게임으로 검증한다. 마지막에는 EXE+config+필수 assets의 독립 폴더를 제공하고 소스/작업 CWD 의존, 누락 파일·worker·held key가 없음을 확인한다. public 배포는 로컬 실행 가능한 패키지 준비를 뜻하며 인터넷 업로드/공개 게시 승인은 아니다.

## Evidence And Exit Gate

동작별 p50/p95/p99/max·frame age·실제 전송/반복·거부율, 같은 저장 입력의 판독/threshold/tie/오류, 녹화OFF/ON/느린 writer, CPU/peak·종료 후 memory, actual joins/키 해제·독립 video decode와 최종 package hash/실제 로더를 기록한다. 한 동작의50ms나 hold-only 통계를 전체 성공으로 확대하지 않는다. 협곡/재획득의 실패는 계속 드러내고 근거에 따라 수리한다. 전체 회귀와 관련 native 검사 뒤 결과/미달 범위를 보고하며 임시 자료를 정리한다.
