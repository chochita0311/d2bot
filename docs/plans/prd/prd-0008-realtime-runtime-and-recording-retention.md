# PRD-0008: 실시간 판단과 기록 분리·보존 상한

## Metadata

- ID: `prd-0008`
- Status: `draft`
- Owner role: `human`
- Created / Updated: `2026-10-05`

## Request Summary

텔레포트처럼 화면이 빠르게 바뀌는 런에서 저장·인코딩·로그·정리가 최신 관찰과 생존/이동 제어를 지연시키지 않도록 한다. 향후 100–200회 반복에서도 대기 데이터·메모리·디스크 사용량이 무한히 증가하지 않는 공통 기록 수명주기를 정의한다. 현재 마을 구현은 유지하며 이번 요청은 후속 설계 조건을 기록하는 범위다.

## Source Set

- Human request: 2026-10-05 실시간 동작과 저장 역할의 스레드 분리, F5 참고, 장시간 반복의 overflow 방지·retention; 앞선 세션 종료 시 개발자료 정리 지시.
- Local reference: F5의 `docs/architecture/data-pipeline.md`, `docs/policies/latency-and-integrity.md`, `internal/application/ingest/queue.go`, `internal/application/journal/writer.go`, `internal/application/stream/pipeline.go`, `scripts/inspect-operational-log-retention`을 읽기 전용으로 확인했다. 고정 크기 admission, 단일 writer, batch, 포화·종료 처리의 원칙을 참고한다. F5의 금융 durability barrier나 기간 수치를 이 프로젝트로 복제하지 않는다. 마지막 script는 삭제 후보 조사이며 실제 삭제기가 아니다.
- Current implementation: [마을 세션](../../../diablo2/actions/run_lifecycle.py), [실시간 worker](../../../diablo2/common/realtime.py), [비동기 로그](../../../diablo2/common/async_log.py), [녹화](../../../diablo2/actions/recording.py).
- Owners: [아키텍처](../../project/architecture.md#future-real-time-observation-and-recording), [개발자료 정리](../../project/developer-guide.md#evidence-handling), [설정 현황](../../../config/system/system.md), [실험 계획](../../features/summoner-experiment-plan.md).

## Product Intent

게임 제어는 저장 속도가 아니라 현재 화면·안전 상태에 따라 움직여야 한다. 운영자는 무엇이 저장됐고 무엇이 누락됐는지 알 수 있어야 하며, 오랜 실행이 메모리나 디스크를 소진시키지 않아야 한다. 기록 품질 저하와 화면 관찰/생존 제어 실패를 구분한다.

## Confirmed Scope

- 캡처·인식·판단/입력과 기록·인코딩·쓰기·retention의 실행 책임을 분리한다. 실시간 입력 경로는 저장 완료나 파일 정리를 기다리지 않는다. 비상 입력도 writer 종료/flush 뒤로 밀리지 않는다.
- 화면 소비는 최신 유효 프레임을 사용하고 오래된 미처리 프레임을 쌓지 않는다. 상태 변화 전 인식이 늦게 끝나더라도 새 위치의 행동 근거로 채택하지 않는다. 방/창/이동 세대와 frame sequence·나이를 함께 판단한다.
- 캡처 이미지의 소유권과 유효 기간을 정의한다. 저장자가 늦게 처리하는 동안 같은 버퍼를 수정/재활용하여 다른 화면을 기록하거나 판단하지 않는다.
- 모든 큐와 보유 데이터에 개수·byte 상한을 둔다. 전체 timing 목록·GUI 이벤트·실패 목록·종료 대기 데이터도 포함한다. 새 프레임/이벤트마다 무제한 worker를 만들지 않는다.
- 큐가 꽉 차면 비실시간 경로에서 오래된 대체 가능 기록을 생략/병합하고 누락 수를 집계한다. 호출자에서 동기 파일 쓰기로 돌아가거나 큐를 무한 확장하지 않는다. 방 결과·위험·중단의 최소 기록은 별도 제한 용량과 누락 표시를 갖춘다. bounded/nonblocking 상태에서 무조건 무손실 저장을 보장한다고 주장하지 않는다.
- 진단 저장 실패는 별도 상태로 보고한다. 실제 필드에서는 검증된 생존/탈출 처리를 우선하며, 자료가 필요한 다음 실험 묶음이나 새 방의 확대를 안전 경계에서 차단한다. 기록 오류 때문에 게임이 정지된 것처럼 취급하지 않는다.
- 반복 수와 무관하게 로그 segment 크기/개수, 기록 총 byte/파일·세션 수/나이, 디스크 여유 공간을 제한한다. 활성 파일 자체도 끝없이 커지지 않게 구간을 나눈다. 성공·실패 자료 모두 전체 상한에 포함한다.
- 정리 작업은 종료된 task 소유 자료만 삭제하고 활성 writer/reader 자료·프로그램의 `assets/`를 보호한다. 삭제 비용이 제어 경로를 막지 않는다. 비정상 종료의 고아 자료도 소유·활성 여부 확인 후 같은 상한으로 관리한다.
- 정상 운영의 자동 retention과 개발 세션 종료 정리를 구분한다. 개발 시 필요한 인식 자산은 이름을 정해 `assets/`로 선별하고 소비자 참조 확인 후 나머지 개발자료를 정리한다.
- 서모너 외 다른 런도 같은 공통 기록 정책을 사용할 수 있어야 한다. 저장 없음/이벤트 중심/표본 프레임·제한 클립 등 기록 수준을 구분하고 원본 전체 저장을 무제한 기본값으로 두지 않는다.

## Excluded Scope

- 현재 마을 동작의 즉시 리팩터, Python→Go 전환, F5 코드/금융 데이터 수정이나 실행.
- 100–200회의 실제 게임 실행 승인, 생존 검증 없이 텔레포트·전투 속도 최적화.
- 무제한 원본 녹화/실패 영상 예외 보존, 외부 업로드, 별도 DB/분산 시스템의 선도입.

## Uncertainty

| 항목 | 실행 경계 승인 전 정할 값 |
| --- | --- |
| 반응 시간 | 텔레포트/HUD 위험/입력별 frame age 및 capture→action의 p95/p99·최악값 상한 |
| 실행 분리 | 기존 worker 재사용 범위, 인코딩 부하 실측 후 thread/process 선택, 입력·상태 소유자 |
| 메모리 | 큐별 개수·byte와 최대 단일 프레임/클립, 예약된 중요 기록 용량 |
| 기록 수준 | 기본 저장 수준, 샘플링/위험 전후 구간, 누락 시 평가/다음 방 차단 조건 |
| 디스크 | segment 크기, 총 용량·세션 수·기간, 여유 공간 하한, 정리 주기/목표 여유분 |
| 종료 | 입력 해제와 capture 중단 순서, writer drain의 제한 시간·초과 시 discard, 재시작 복구 |

숫자는 마을 측정이나 F5의 기본값을 그대로 가져오지 않고 승인된 부하에서 정한다. 세션 종료 정리의 사용자 지시는 이미 확정이며 이 값들의 결정과 분리한다.

## Constraints

현재 북쪽 runtime은 capture/fast/slow/decision worker와 최신 프레임·최근 5개 버퍼를 갖는다. 로그는 유한 큐/rotation을 갖지만 중요 로그 포화 시 호출자 동기 쓰기 fallback이 있다. 마을 evidence 저장은 동기식이며 timing 목록을 종료까지 누적한다. 이 부분들의 존재가 본 PRD의 분리·상한 계약 충족을 의미하지 않는다.

저장 분리만으로 CPU 경쟁·lock·메모리 복사·스케줄링 지연이 없어지지 않는다. [생존](prd-0004-summoner-survival-and-buffs.md), [탐색](prd-0005-arcane-navigation-and-recovery.md), [완전 런](prd-0006-summoner-combat-and-loot.md)의 안전/관찰 전제를 유지한다. 캡처·생존 제어 실패와 진단 누락을 같은 정책으로 처리하지 않는다.

## Acceptance Envelope

| ID | 충족해야 하는 결과 | 검증 범위 |
| --- | --- | --- |
| RT-01 | 저장 off/on/느린 writer에서 승인한 실시간 지연 상한 유지 | 같은 입력 자료·환경의 p50/p95/p99/max, frame age, 입력 선점 비교 |
| RT-02 | 입력은 오래된 프레임/세대의 늦은 인식을 사용하지 않음 | 이동 전후·순서 역전·늦은 소비의 입력 없는 재생 |
| RT-03 | 큐·메모리가 상한 안에 머물고 누락을 숨기지 않음 | 소비 중단/포화/최대 프레임·byte 초과, GUI 느린 소비 |
| REC-01 | 100/200회 상당의 기록 workload에서도 총 byte·파일 수·메모리 상한 유지 | 게임 입력 없는 부하/retention 경계 검증; 모든 실패 보존 예외도 포함 |
| REC-02 | 디스크 부족·쓰기 실패·정리 실패가 위험 입력을 늦추지 않음 | 가짜 writer/파일시스템 경계와 승인된 안전 감독 검증 |
| REC-03 | 종료는 제한 시간 안에 입력을 해제하고 기록 drain/discard 상태를 보고 | 포화·writer 실패·중단·재시작, 활성/공유 자산 삭제 0 |
| REC-04 | 세션 종료 정리 뒤 필수 인식 자산으로 프로그램이 작동 | manifest/참조 확인, 제거된 개발 경로 의존 0 |

실제 게임에서 100–200회를 돌리는 것은 위 부하 검증이나 본 초안의 자동 승인 결과가 아니다.

## Candidate Features

- Foundation 후보: 최신 프레임·결과 세대·입력 단일 소유권·큐 상한과 포화 계약.
- Product 후보: 저장자가 느려도 관찰/생존 판단을 유지하는 공통 비동기 기록 경로.
- Foundation 후보: task/활성 파일 소유권과 메모리·디스크 보존 상한 계약.
- Product 후보: segment rotation·용량/기간 정리·종료 복구와 상태 표시.

후보는 미승인이다. 실행 spec·코드·게임 부하 테스트는 별도 경계 승인 뒤 진행한다.

## Continuity Notes

- `2026-10-05`: 사용자 실시간 경로 분리·100/200회 retention 요구와 F5의 bounded writer 참고를 반영. 마을 기능 변경·자료 삭제·실제 게임 실행은 수행하지 않았다.
