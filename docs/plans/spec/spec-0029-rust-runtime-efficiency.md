# SPEC-0029: Rust 효율 개선 실행 계약

## Metadata
- ID: `spec-0029`; Status: `approved`; Feature: [FEAT-0029](../feature/feat-0029-rust-runtime-efficiency.md); Run: [RUN-20261024-01](../run/run-20261024-01-rust-runtime-efficiency.md); Date: `2026-10-24`.
- Profile: `fullstack-product`; Evaluators: Contract, Functional.

## Authority And Baseline

최신 사용자 지시와 [효율 계획](../refactoring/refactor-0003-rust-efficiency.md)이 승인 범위/기준을 소유한다. 이전 EXE/234개 source 지문과 Python 비교 기준을 유지한다. Primary가 요구사항·구조·수명·통합·최종 판단을 담당하고 named read-only worker는 한정 증거만 제공한다.

## Implementation Contract

- 먼저 ORB/codec의 private 단계 시간과 자체 Windows 프로세스 작업집합/전용 commit/default heap allocated/committed를 측정한다. unsafe OS 읽기는 `platform/windows` 안에서만 허용하고 남의 프로세스/게임 메모리를 읽지 않는다. 진단 기능은 heap 축출/강제 정리를 하지 않는다.
- ORB FAST는 최종 선택 가능한31px border/mask 영역 및 NMS에 필요한 한 픽셀 이웃의 점수만 계산할 수 있다. 원래 mask 밖 이웃의 억제 효과·엄격한 비교·동점/점수·후속 Harris/각도/descriptor/level 순서를 보존한다. 행 단위 영역/중복 연산/분기/루프 자료 배치는 safe Rust로 개선한다. 새 프레임을 읽지 않고 이전 결과를 재사용하는 변경은 하지 않는다.
- FAST 후보의 정수 비교는 순수 Rust wide1.7.1의 safe SIMD API로16픽셀씩 묶을 수 있다. Windows x64 기본 SSE2/portable scalar fallback 범위이며 host 전용 CPU flag/fast-math/unsafe 앱 코드는 추가하지 않는다. 엄격한 차이>20, 원형 연속9개, score·NMS·mask/border와 나머지 tail을 scalar oracle/실제 source ORB와 비교한다. 이 의존성은 계산 구현만 제공하며 배포 때 native DLL을 요구하지 않는다.
- blur는 같은 f32 tap/누적 순서를4픽셀 lane으로 묶고 명시적 ties-even 반올림을 유지한다. nearest는 정수 픽셀 복사,0.5배 축소는 완전2x2의 정수 평균과 부분 경계의 기존 ties-even을 분리할 수 있다. 기존 gray/color/tiny/odd source 비교를 보존한다.
- ORB 피라미드는 이전 단계에서 다음 단계를 생성하는 기존 의존 순서를 유지하며, 완성된 단계의 특징 계산을 다음 단계 생성과 겹쳐 실행할 수 있다. 불변 gray/mask Arc만 해당 작업과 다음 생성 단계가 보유하고 더 이상 필요 없는 이전 영상은 해제한다. 결과 slot은8개 고정이며 단계/내부 순서를 합칠 때 보존하고 모든 job 종료 뒤 반환한다. 정상 관측/오류의 source parity와 실제 시간/peak를 확인한다.
- 피라미드의 Q8 보간은 동일 정수식/경계/반올림을4개 u32 SIMD lane으로 계산할 수 있다. 임의 크기·홀수 tail·mask와 source ORB parity를 보존한다. 공유 계산 풀8개와 맞춰 큰 template의 FFT/정규화 행 밴드도 최대8개로 나누며, FFT scratch는 대기 job이 아니라 실제 실행 중인 job 안에서 생성·해제한다. 출력별 합산 순서는 유지하고 성능과 GUI 자원을 다시 확인한다.
- Codec의 YUV/예측/DCT/양자화/복원/비트 작성과 버퍼는 측정 후 개선한다. byte 동일성은 요구하지 않지만 native 크기/무음8fps/GOP·seek/독립 decode·기존 PSNR/MSE/정지·pan·컷 조건을 보존한다. 변환 정밀도·예측/빠른 경로 변경은 해당 수치 경계와 품질 결과를 명시한다. 이전 버퍼는 더 이상 독자가 없을 때 재사용하고 실제 종료에 해제한다.
- YUV는 유효 행/2x2 묶음과 경계 복제를 분리해 동일 정수 결과를 유지한다. DCT/IDCT는 대칭·짝/홀 항으로 분해해 축당 곱셈을64개에서22개로 줄일 수 있다. f64를 유지하되 덧셈 순서 변경으로 압축 byte가 달라질 수 있으며 직접 행렬 대비 절대 오차<1e-9와 기존 독립 decoder 품질 조건을 함께 검증한다. 이 허용은 비전 판정/임계값 변경을 뜻하지 않는다.
- 패킷 버퍼는 최악 크기 전체를 미리 점유하지 않고 최대64KiB로 시작해 필요할 때만 fallible 확장한다. 기존 packet 상한 안에서만 작성하며 확장 실패는 encoder 실패로 전달하고 frame을 쓰거나 정상 완료로 숨기지 않는다. 활성 인코더는 관측한 packet 용량을 재사용하고 종료 시 버퍼를 해제한다.
- motion search는 후보별 visible/padded 경계를 한 번 확인한 행 슬라이스로 같은64개 SAD 화소를 비교하고 이미 평가한 동일 벡터만 중복 제거할 수 있다. 후보 순서·strict-less 동점·penalty·검색 범위는 보존한다. 정수/반 화소 예측은 내부 block 행 복사와 기존 경계 clamp fallback을 나누며 모든[-16,15] 벡터/홀수 크기/끝 블록을 scalar 결과와 비교한다.
- 같은 입력의 중복 P부호화와 매 프레임 버퍼 할당을 줄인다. 직전 원본 YUV와 정확히 같은 프레임/MB만 기존 복원 참조를 그대로 표준 zero-MV skip으로 유지하며 변화량 허용치를 추가하지 않는다. GOP16/keyframe·시간축·장면 전환은 유지한다. 필요한 이전/현재 YUV와 복원 scratch는 활성 인코더가 한 장씩 소유·재사용하고 종료 시 모두 해제한다. 이동 벡터 예측과 최종 syntax/DC 순서는 직렬로 유지하고, 독립 MB 변환/복원만 최대8개 borrowed 행 밴드로 나눈다. 모든 job이 끝나기 전 원본/참조/출력 버퍼를 재사용하지 않고 중복 프레임/움직임/컷·seek·PSNR/MSE와 실제 기록 시간/peak/Stop을 재검증한다. 이 변경은 같은 정지 화면만 빠르게 만든 것을 모든 동영상의 속도 향상으로 보고할 근거가 아니다.
- 관찰 scratch/worker와 증거는 실제 소비/쓰기 수명으로 한정한다. 프레임 history의 조용한 삭제, 빈 working set 강제 축출, 임의 allocator/C dependency 도입은 하지 않는다. 중단 요청과 worker/핸들/held input의 실제 종료를 구분한다.
- GUI+동일 판독80회의 commit 증가와 캡처 단독의 안정 결과를 근거로, 순수 Rust Rayon1.12의 scoped 계산을 사용해 호출마다 OS 스레드를 만들던5곳을 공유 계산 풀로 옮긴다. worker는 available parallelism 이내 최대8개이고 nested 작업도 같은 풀을 쓰며 결과 순서/누적/오류를 보존한다. 첫4개 제한은12논리 코어의 중첩 template 판독에서 지연 증가가 보여8개와 비교한다. 실행 세션/관찰기 수명 동안 lease를 유지하며 마지막 소유자가 끝나면 pool 종료와 실제 OS worker join을 수행한다. 전역 registry는 Weak만 보유하고 frame/model/job을 보유하지 않는다. 생성 실패·panic·nested scope·borrowed frame 해제·동시 공유·마지막 join을 검증한다. 사용자 지시가 허용한 같은 목적의 구조 변경이며 외부 C/runtime codec을 추가하지 않는다.
- 첫 실게임2회 뒤에도 private241→393MB/기본 heap19.6→20.8MB가 남았고, 추가 조사에서 hazard crop/RegionObserver template/loot scan3곳이 OS thread를 반복 생성했다. 이3곳도 같은 scoped 계산 풀로 옮기며 원래 crop/template/candidate 결과·오류 관찰 순서와 strict predicate를 유지한다. RegionObserver는 모델 수명 동안 lease를 보유한다. 원래 worker panic 메시지/생성 I/O 오류를 보존하고 모든 borrowed job을 join한 뒤 결과를 읽는다. 일회성 app I/O/핫키/감시 session 소유자는 compute pool로 옮기지 않는다. 영향을 받는 source hazard/region/loot와 native/runtime 회귀, 같은 저장 입력 시간·실게임 반복 메모리를 다시 검증한다.
- 배포는 코드에서 읽는 manifest/config/template/공통 참조의 closure만 포함할 수 있다. 참고/평가/과거 기록을 runtime 의존성으로 추측하지 않고 실제 소비자를 추적한다. 원본 assets는 지우지 않는다. 누락된 필수 참조는 packaging을 실패시키며 기존 모드/CLI/GUI를 제거하지 않는다. 고지/출처는 축소 대상이 아니다.

## Verification And Exit

같은 입력30회/3 warmup·p50/p95·pixel/decision 지문, source ORB/전체 관찰 비교, MPEG-4 독립 decoder/품질/색상/움직임/실패/마감, 수명·취소·반복 자원 검사를 수행한다. 최종 fmt/check/Clippy/release/영향받는 전체 회귀와 native 자원 검사 후 별도 신규 패키지에서 GUI/CLI/PNG/녹화/actual stop/실게임을 실행한다. CPU·메모리·크기·실제 deadline 거부율과 느린 항목도 보고한다. 전체 kill/key·10회/27-of-30·새 OS와 사람 외관 수용을 자동 PASS로 채우지 않는다. 원본 Python/config/assets와 표준 Cargo cache는 보존하고 task-owned scratch는 마감 시 정리한다.

## Documentation Contract

이번 feature/spec·현재 코드/최종 측정이 변경의 근거다. RUN은 실행 이력, EVAL은 최종 수치/조건/한계, feature/refactor/index는 현재 상태를 소유한다. README/runtime/architecture/developer guide와 assets/config 안내는 각각 시작·운영·구조·개발·자료/필드 계약만 갱신한다. 이전 EVAL의 역사와 미확인 전체 게임/사람 수용을 보존하고 임시 파일에 durable 참조를 만들지 않는다. Primary가 링크/한글/수치·조건·현행 상태의 일치와 관련 소비자의 stale 가정을 확인한다.
