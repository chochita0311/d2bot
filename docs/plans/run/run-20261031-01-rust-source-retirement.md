# RUN-20261031-01: Rust 전환 세션 마감과 Python 소스 정리

## Boundary

2026-10-31 사용자가 이번 세션의 전환·초기 시험·최적화 기록을 정리하고, 남은 Python 코드를 모두 제거한 뒤 리뷰·commit·push와 다음 세션 프롬프트를 요청했다. REFACTOR-0002의 추가 소스 정리 범위다. 이 작업에서는 새 게임 입력이나 게임 시험을 진행하지 않는다. 기존 Esc/조작 인계 대기를 새로운 실게임 승인으로 해석하지 않는다.

기준 Python 코드는 `cde68bc2eb1285eddcd8a261cdfe411ed4924f76`다. 삭제 전 `git diff`로 앱 소스가 기준과 같음을 확인하고 파일별 SHA를 보존한다. 비교 실행의 입력은 이후 승인된 현재 config/assets이며, 원래 커밋의 모든 자산과 같다는 뜻이 아니다. 실제 parity 판단은 [merge check](../refactoring/refactor-0002-rust-port-merge-check.md)가 소유한다.

## Session Evidence Map

| 범위 | 이번 세션에서 구현·확인한 내용 | 증거와 한계의 owner |
| --- | --- | --- |
| 설치·구조 | MSVC Rust 도구·SDK, 루트 Cargo 단일 패키지, `src/`의 기존 도메인 책임, GUI/CLI/도구 연결 | [포팅 계획](../refactoring/refactor-0002-rust-port.md), [빌드 평가](../evaluation/eval-0015-rust-native-build.md), [runtime 안내](../../features/rust-runtime.md) |
| 설정 | 순서·병합·기본값·선택·큰 정수·실패 비교. 선택적 버프 gap의 NaN/무한대/음수/overflow/Duration 초과는 메모리에서 제거하고 기존 기본값과 경고로 계속 진행; 필수 안전 정책 오류는 명시적으로 실패 | [설정 평가](../evaluation/eval-0016-rust-config-contract.md). 개인 파일에서 NaN이 발견된 것이 아니라 오류 입력 비교에서 확인한 결함 |
| 화면·판독·입력 | 소유한 BGR/좌표 변환·직접 이미지 연산·ORB/정책·Windows 표준 캡처/입력·해제/Stop/실패 테스트 | EVAL-[0017](../evaluation/eval-0017-rust-frame-geometry.md)/[0018](../evaluation/eval-0018-rust-vision-operations.md)/[0019](../evaluation/eval-0019-rust-character-policy.md)/[0020](../evaluation/eval-0020-rust-windows-io.md)/[0021](../evaluation/eval-0021-rust-background-motion.md). 승인된 JPEG 참고 화면 차이는 별도 delta이며 PNG/실캡처 판정의 포괄 허용 오차가 아님 |
| action·앱 | 조합·줍기·방 lifecycle, Summoner 정책/흐름, egui 제어판·preview·5개 도구, 취소/실제 join/정리 오류 보고 | EVAL-[0022](../evaluation/eval-0022-rust-actions-lifecycle.md)/[0027](../evaluation/eval-0027-rust-summoner-runtime.md)/[0029](../evaluation/eval-0029-rust-desktop-control.md)/[0030](../evaluation/eval-0030-rust-cli-replay.md). 전체 실제 게임 동등성은 미입증 |
| 녹화 | FFmpeg/Xvid C 없이 직접 Rust MPEG-4와 AVI/OpenDML, 무음·명목8fps·native 크기, 독립 디코딩·seek·품질·크기·마감·오류 비교 | [EVAL-0028](../evaluation/eval-0028-rust-mpeg4-recording.md). 모든 MPEG-4 profile 인증이나 wall-clock8fps 보장이 아님 |
| 배포 | 정적 CRT EXE와 config/assets/고지 폴더, 실제 loader·외부 CWD 실행·DLL 검사 | [EVAL-0031](../evaluation/eval-0031-rust-windows-distribution.md). 새 OS 전체 인증·사람의 외관 수용과 구분 |
| 효율 | ORB 자료 재사용·단계 병렬화, 제한된 공유 compute worker, 인코더 버퍼 재사용/중복 블록 skip, frame/worker 수명 검사 | [EVAL-0032](../evaluation/eval-0032-rust-runtime-efficiency.md). Python이 native OpenCV를 호출하므로 Rust 언어 전환만으로 모든 연산이 더 빨라지지는 않았음 |
| 실시간 CPU | 요청 기반 최신 프레임1개, 공유 ROI, foreground/background 예산, 현재 준비의 중첩, 지속 조준의 추가50ms 제거. 포인터 확인·Stop/포커스/geometry·만료·키 해제는 유지 | [EVAL-0033](../evaluation/eval-0033-cpu-realtime-control.md). WGC/GPU 보류,50ms 미달, 메모리 예약/working set과 살아 있는 heap을 구분 |
| 최신 서모너 개발 | 공격 반경 축소, 북쪽3회 단위 검토, 그래프/종점 보완, 부분 버프 실패 보호, J의 지도 가림 시체 재확인·방어 후 encounter 재진입·로그 스크롤 | [현재 RUN](run-20261024-03-rust-summoner-development.md), [EVAL-0034](../evaluation/eval-0034-rust-summoner-navigation-refinement.md). R01–R18:6정상/12실패/연속0; 회복은 새 방 성공에서 제외. J 실게임은 미실행 |

배포가 약475MB에서 약24MB가 된 주원인은 개발용 전체 화면 reference 제외다. 당시923파일/474,641,839B에서 필수 공용 assets와 실제 사용 reference만 포함하도록 바꿨다. 필수 `assets/regions`는 포함하며, 소스의 reference corpus는 보존한다. 두 마을 reference는 실제 runtime template여서 예외로 포함한다. 폴더별 수치·등록 loader·새 설정의 한계는 [EVAL-0032의 자산 재감사](../evaluation/eval-0032-rust-runtime-efficiency.md)가 소유한다. 임의 새 모드의 자산까지 자동으로 공급한다는 보장은 아니다.

## Retirement And Review

- 하위 에이전트가 docs-structuring/docs-shaping 관점의 현재 안내·역사 기록·링크/의미 보존과 refactor-plan 관점의 source/실패/출력 대응을 읽기 전용으로 검토했다. primary가 소유권·수정·최종 판단을 담당한다.
- 삭제될 Python 내부의 유일한 Markdown은 [북쪽 경로 역사](../../features/summoner-north-route-history.md)로 옮겼다. 현재 소스 안내는 Rust로, 역사적 Python 링크는 고정 Git SHA로 연결한다. 과거 run/spec의 명령과 시험 결과를 현재 실행 지침으로 바꾸지 않는다.
- 앱·5개 개발 명령·benchmark는 Rust 구현을 사용한다. 비교 테스트의 기대값은 삭제 전 Python/OpenCV로 관찰한 고정 자료를 사용하며 Rust로 다시 계산해 정답을 만드는 방식은 허용하지 않는다. 새 AVI의 실제 바이트도 입력 identity에 포함한다. 변경된 AVI는 옛 decode 결과를 재사용해 통과할 수 없다.
- 일회성 Python 생성기가 만들었던 Unicode11·그리기·MPEG-4 테이블은 Rust 데이터로 유지하고 출처/고지/고정 해시를 검증한다. 일반적인 새 Unicode 버전/새 upstream 테이블 재생성 도구를 제공했다고 주장하지 않는다.
- 전체 parity는 `Unknown/Provisional`이다. 현재 회귀 사례, source로 감사한 분기와 승인된 변경을 전체 OS/입력/실게임의 완전 동등성으로 확대하지 않는다.

## Verification And Artifact Receipt

- 앱 코드 기준은 `cde68bc2eb1285eddcd8a261cdfe411ed4924f76`; 삭제될39개 비교/생성/benchmark 도구도 `d11737aeb6ec885293b2f6c0e01e9f44aad0ba15`에 별도로 고정했다. 현재 checkout에는 포함하지 않는다. 최종 작업 브랜치는 `codex/rust-native-session-close`다.
- 삭제 전 Python 전체765검사(164.368초)는764통과/1실패였다. 실패는 이전 `rune` 기대값과 현재 승인된 named `ist rune`의 차이다. 최초6개 임시 report 교체 WinError5는 workspace TEMP/TMP로 재실행해 해소했다. 이를765전체 PASS로 기록하지 않는다.
- 고정 자료 수집기의 일반 문자열 경로 오인과, calibration이 입력 bank를 갱신하는 것을 외부 변경으로 오인한 검사를 수정했다. 이후 tools/영상/Windows/worker 등53개 선택 검사가 통과했다. 느린 debug 전체 수집은 terminal corpus 기대값 보존 뒤 중단했고 최종 전체 판정에는 사용하지 않는다.
- source/tool Python154개와 `requirements.txt`/`pyproject.toml`, 프로젝트 `.venv`·두 virtualenv 도구 owner를 제거했다. 디렉터리 owner는4,272파일/222,407,830B였고 별도 스크립트/manifest/캐시도 정리했다. 숨김/ignored까지 포함한 프로젝트 `.py/.pyw/.pyi/.pyc/.pyd` inventory는0개다. PC의 전역 Python 설치는 이 범위가 아니다.
- 제거 후 첫 Rust-only smoke21개 PASS. 추가 리뷰에서 빠져 있던 `--state` 내용 binding을 보완하고 직전 독립 관찰의 planner.data를 다음 비교 입력으로 복원했다. 두 번째 관찰의 expected JSON SHA `310d38286219ce4709110486366002ca8883ac378920184640c8ce203efc107d`는 바꾸지 않고 identity만 강화했으며 provenance가 from/to와 before-state 근거를 소유한다. 새 source로 관련12개 검사 PASS다. gzip 손상/잘림/뒤따른 데이터, inventory 누락/중복, config root 모호성, 파일 byte 변경도 검사한다.
- 최종 source의 `cargo fmt --all --check`, locked/offline all-target check, all-target Clippy `-D warnings`는 각각 exit0/경고0이다. 전체 static-CRT release suite와 실제 배포 receipt는 아래에 확정한다.
- 문서245개/로컬 링크1,359개 검사에서 실제 누락0, template의 치환 예시 경로24개를 별도로 구분했다. 현재 사용 안내에는 Python 실행 명령이 없고 과거 계획/실행 증거는 유지한다. 전체 코드 branch 감사 완료/모든 OS·화면 동등성을 의미하지 않는다.

최종 release/배포 receipt(`2026-10-31`):

- 원본 비교 환경을 제거한 뒤 첫 전체 release에서4개 사례가 missing fixture key로 실패했다. 원인은 ASCII `$`의 새 escape 규칙, 상대 임시 template 경로, report/sidecar와 JPEG 원본/decoded/shared template 의존성의 지문 누락이었다. 키를 임의로 바꾸거나 Rust 결과를 정답으로 만들지 않았다. 고정 Git 원본과 기록된5개 패키지 버전을 task 전용 범위에 잠시 복원했고 OpenCV build-info SHA도 처음과 같음을 확인했다. 네 사례를 실제 원본과 다시 비교해4개 모두 통과했으며 기존 parsed expected 값과도 모두 같았다. 이후 참조 source/packages/cache1,629파일/707,773,613B와 임시 비교 test/exe를 제거했다. `provenance.json`이 독립 재관찰과 버전·결과 지문을 소유한다. 이 중간 실패를 전체 PASS로 합치지 않는다.
- 참조 제거 뒤 **static-CRT release all-target:370통과/0실패/6ignored,62개 result group, exit0**. 명령은 `cargo test --release --all-targets --offline --locked --no-fail-fast --target x86_64-pc-windows-msvc --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]'`다. Rayon/입력/취소·정리/오류·ORB/영상/GUI/CLI/Summoner의 현재 기본 suite이며 실제 게임을 실행하지 않았다.
- 제외6개는 성능 계측2개(`efficiency_profile_codec_stages`, `efficiency_profile_orb_stages`)와 native4개(`native_fixture_window_capture`, `native_gdi_resources_are_balanced`, `native_listener_starts_and_unhooks_without_input`, `native_capture_smoke`)다. 앞선 native 실행 이력과 구분하며 이번 마감에서 이6개를 실행했다고 하지 않는다.
- 성공한 전체 실행의 실제 fixture 사용 receipt로 중간/폐기 관찰13개/7,978B를 제거했다. 현재473개/6,394,640B gzip 자료와 정확한 manifest가 남는다. 정리 후 release baseline 검사9개를 다시 통과했고 최종 fmt/check/all-target Clippy는 exit0/경고0이다. 중복한 검사 개수를370에 더하지 않는다.
- Source240 fingerprint는 `bad2b754f216de4e6d431d188098dbf0a138fbbce1b6bafdc24c0abf2679fae5`. 범위는 모든 `src/` 파일과 Cargo.toml/lock이며 Ordinal 정렬한 POSIX 상대경로+공백+raw-file SHA256+LF를 UTF-8로 해시했다. fixture manifest SHA는 `e44078f7901fd4bbc26054735dacbe4d3ccd55b46fca2ffb0d2e19c1ceeed1a7`, provenance SHA는 `b67deb5177593a52309e955e829a0df52e570ce72fdbbffecf9d06a7a8a97eac`다. raw source 지문은 작업 파일의 줄바꿈도 포함한다.
- 최종 폴더 `dist/d2bot-windows-x64-rust-only`:660파일/24,060,889B,659payload 모두 hash/길이 일치·누락/추가0. EXE17,003,008B SHA `c024a3afb4c173576ae8932607595e718db42794a2a4acdf5b59767f4024e967`; manifest SHA `bc553e4c26920b7a2b8dce411cabe6ad70127c0a2f26e9a2c172d00d23565618`. 공용 assets4,990,287B·필수4지역,161개 고지와19고유 system/API DLL import를 유지한다. Python/OpenCV/FFmpeg/Xvid/외부 CRT payload는 없고 test fixture도 배포에 넣지 않는다.
- 실제 배포 EXE를 빈 외부 CWD와 Windows-only PATH에서 `--diagnose`, `--summoner-run --continuous-north --north-cycles 1` 기본 dry로 각각 exit0 확인했다. packaged typed loader도3캐릭터/설정 자산 경로5개·입력0/capture0이다. dry는 북쪽 개발 계획이며 실제 방/처치/획득/Canyon 성공 증거가 아니다. GUI·녹화 병행 새 실게임은 이번 마감에 추가하지 않았다.

Git 공개와 scratch 최종 정리: 최종 검토 후 현재 개발 브랜치에서 수행한다. 원격 commit 동일성은 push 뒤 확인하며 전체 parity/실게임 수용으로 해석하지 않는다.

정리 owner: 새 작업의 `target/tmp/python-retirement-20261031/`는 결과를 승격하고 모든 writer/검사 종료·절대경로/소유·reparse 부재를 확인한 뒤 제거했다(1,551파일/255,599,879B, 제거 후 경로 없음). 이전 임시 config11파일/38,769B도 제거했다. `target/tmp/summoner-development-20261030-r09/retained-diagnosis/`만 R14 종점 재획득/R17 부분 버프/R18 포인터 오류 원인 분석을 위해12파일/16,884,893B로 한정 보존한다. 각 원인 해결 또는 필요한 회귀 자료를 정식 owner로 승격한 뒤 제거한다. 앱/배포/정식 검사는 이 임시 경로에 의존하지 않는다. 일반 Cargo 빌드/공유 캐시는 유지한다.

## Next Session Boundary

먼저 이 문서·merge check·EVAL-0034와 현재 실제 게임/조작 인계를 확인한다. 최신 후보의 첫 과제는 서모너 시체 빠른 판독→줍기 확인→일지/붉은 포탈→협곡 WP→Act1의 실제 마감이다. 처치/키 미확인을 중앙 복귀 성공으로 대체하지 않는다. 기존 빈 북쪽 개발 종료와 실패 시 north return→중앙WP→Act1 복구는 따로 기록한다. TP로 Act2 마을에 귀환하지 않는다.

일반 공격240×144, 보스/대기 방어300×180을 기준으로 경로와 판독을 다듬는다. 보석은 수집하지 않으며 정책은 이스트 이상이나 현재 이름별 지원은 열쇠/이스트/오움뿐이다. 나머지 룬은 미지원으로 건너뛰고 클릭을 획득 확정으로 세지 않는다. 북쪽 새 방3회마다 실패·길찾기·단계별 지연·메모리/해제·녹화 병행을 검토한다. 북쪽10연속 이후 원래 북→남 조합 등 순서를 따른다.50ms는 안전/기능을 유지하며 접근할 지향 목표이지 달성한 수치가 아니다.
