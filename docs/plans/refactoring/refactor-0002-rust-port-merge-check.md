# REFACTOR-0002: Rust 포팅과 Python 소스 정리 merge check

## Scope

- Track: [REFACTOR-0002](refactor-0002-rust-port.md); 실행과 세션 evidence map: [RUN-20261031-01](../run/run-20261031-01-rust-source-retirement.md).
- 비교 코드: `cde68bc2eb1285eddcd8a261cdfe411ed4924f76`, certainty `Confirmed`. Python 앱은 삭제 전 diff가 없었다. 현재 config/assets의 승인된 변경은 별도로 고정하며 옛 커밋의 입력과 혼동하지 않는다.
- 검토 대상: 루트 Cargo/src, 앱·공용 연산/정책·OS·actions/runs·GUI/CLI·직접 녹화·배포, Rust 회귀 테스트와 legacy 관찰 자료; 남은 Python 소스/테스트/일회성 도구/환경 제거.
- 변경 전 working tree는 Rust 전환 전체가 미커밋 상태였다. 사용자 요청 범위의 기존 변경을 포함해 검토하고 새 개발 checkpoint로 commit/push한다. 전체 게임 수용 판정과 병합 승인은 별개다.

## Strict Parity Audit Coverage

| old → new 논리 단위 | 확인한 계약·시험 연결 | 판정/범위 한계 |
| --- | --- | --- |
| `main.py`, `app.py` → `src/main.rs`, `app/{arguments,routes,execution}` | 플래그/기본값/분기 우선순위·dry/help/error·출력·취소/worker join. `rust-app-entry`, `rust-cli-tools`, arguments 단위 사례 | 지정 사례와 source 확인; Rust version/diagnose는 추가 기능 |
| `common/config.py` → `common/config.rs`와 하위 모듈 | 재귀 JSON 정렬/merge·삽입 순서·선택/공유 farm·null/기본값·큰 정수·오류 단계. `rust-config-parity` | invalid optional buff-gap 보정과 전투 반경은 승인된 delta; 기타 지정 사례 비교 |
| frame/capture/controller/realtime → `common/{frame,frame_geometry,capture,controller,realtime}`, `platform/windows` | 좌표/창변경·단일 입력 소유·Stop/포커스·만료·부분 입력 실패/해제·capture/worker 종료. `rust-frame-geometry`, `rust-windows-io`, `rust-input-*`, `rust-latest-capture`, `rust-workers` | 새 OS/DPI/게임 설정 전체의 동등성은 Unknown; 별도 native evidence와 구분 |
| detector/automap/color/scene_motion/생존·버프·필드 정책 → 대응 `common`, `vision_ops`, `field_preparation` | threshold 경계·픽셀/순서·ORB/affine·가짜 시계·불명확 관찰·포션/버프 요청과 확인 구분. `rust-vision-operations`, `rust-background-motion`, observer/policy/field/buff 검사 | 고정 corpus와 지정 수치/실패 비교; 모든 영상·난수/임의 구성은 Unknown |
| actions/gem/loot/town/lifecycle → `actions/*` | 후보 우선순위/조합 계획·클릭/대기 trace·출력 파일·부분 실패 뒤 해제/close·반복. `rust-action-*`, `rust-town-*` | 현재 named loot 정책은 delta. 정리 오류의 보존/합산과 상한 event buffer는 안전성 강화 |
| `runs/summoner/main_run.py`, cycle/gui session → `main_run/*`, `cycle`, `gui_session` | 입력→entry→buff→navigation/encounter→finish/report 흐름·반복1..5·취소 우선·방 유지·부분 report/이미지 실패. main/cycle/gui session 단위 비교와 `rust-main-output` | report 실패를 숨기지 않는 writer/cleanup 차이와 J recheck는 delta. 전체 실제 정상 마감은 Unknown |
| navigation/search/route_graph/steering/routes → 대응 Summoner 모듈·`definition` | 출구/순서·graph 상태·시한·current/stale 구분·계획된 동남서 metadata. `rust-navigation-*`, `rust-steering`, `rust-saved-route`, search/graph 단위 비교 | 단순 파일 대응이 전체 branch 감사를 뜻하지 않음. R14 재획득·긴 방 slot 상한·남쪽 이후 미검증 |
| encounter/appearance/life/journal/canyon → 대응 Summoner 모듈 | 현재 타깃과 corpse pair·loot gate·journal/portal·Canyon 순서와 실패. encounter/canyon/appearance/life 단위·통합 사례 | J 관련 source/corpus 회귀와 live 미실행을 구분. 클릭≠획득, corpse 후보≠처치 확정 |
| Tkinter/core/tools → `ui/*`, `core`, `tools/*` | 선택/버튼/반복/로그/close/preview·5개 command와 출력. UI/core/tool 비교, DPI/log wheel 회귀 | egui 외관·bounded log/실제 join은 차이; 모든 실제 GUI 상호작용/환경 동일성은 Unknown |
| OpenCV VideoWriter → `common/recording/*`, `actions/recording` | 승인된 직접 MPEG-4·AVI8fps/native 크기·atomic CreateNew·종료·오류·독립 decoder 결과 | 코덱 byte parity는 목표가 아님. retirement 뒤 새 AVI는 독립 판독한 기존 바이트의 identity와 일치해야 그 결과를 재사용; 새 bitstream은 새 독립 decode 검토 필요 |

모든 옛 모듈의 책임은 source inventory와 대응표로 확인했지만 **모든 관련 reader/writer·payload 필드·side effect·failure branch를 하나씩 완전 감사했다고 판단하지 않는다**. 따라서 strict audit complete = `No`, any unaudited relevant path = `Yes`, whole behavioral/external contract parity = **`Unknown`**, basis = `Partial audit + selected comparative tests + bounded native/live evidence`다. 테스트 개수나 Rust 파일 존재로 이 판정을 올리지 않는다.

## Intentional And Reviewed Deltas

사용자가 승인한 선택적 잘못된 버프 시간의 보정, JPEG 참고 화면 차이, egui 전환, 직접 Rust MPEG-4, EXE 기준 기본 경로/필수 자산 묶음, CPU 최신 프레임/ROI/compute/추가50ms 제거는 각 spec/evaluation의 조건을 따른다. 이후 전투 반경·부분 버프 sticky 실패·J의 지도/몸통 재확인·로그 스크롤·열쇠/이스트/오움만의 named loot는 SPEC-0031/current run 경계다. 중단 chord의 extra-held-key 제약은 사용자 요청대로 유지한다.

Rust는 Python의 무시된 image writer 반환이나 미완료 thread를 성공으로 숨기지 않고, 정리/보고서 오류를 보존하며 실제 종료까지 소유권을 유지한다. 유한 event/log 상한은 메모리를 제한하면서 생략을 보고한다. 이 안전성/자원 강화는 최초 전체 실행·충돌 방지 및 후속 안전 최우선 지시의 범위로 채택한 차이이며 동일한 오류 문자열/이벤트 개수를 약속하지 않는다.

빈 북쪽 개발 사이클의 `hub_arrived → north_room_closed`는 Python에도 있던 별도 개발 결과다. 이를 정상 서모너 처치·획득·협곡 마감 수용으로 세지 않는다. 정상 encounter는 `kill_confirmed && loot_complete` 뒤에만 journal/Canyon으로 간다. north return은 실패 복구 경계이며 처치 미확인의 새 성공 shortcut이 아니다.

## Validation Run And Assessment

- Working tree: 전환 전체를 검토한 개발 snapshot이며 commit 전에는 clean이 아니었다. 기존 변경을 되돌리거나 다른 branch의 내용을 덮어쓰지 않았다.
- 최종 source의 fmt/check/all-target Clippy `-D warnings`: 모두 exit0/경고0.
- Python 참조 코드/환경을 다시 제거한 확정 snapshot에서 static-CRT release all-target370 PASS/0fail/6ignored(4native·2performance),62 result group. 미완전 identity4개는 실제 고정 원본과 독립 재관찰해 값이 같음을 확인한 뒤 수정했다. 실패를 단순 fixture rename으로 숨기지 않았다.
- 실제 사용한473개 고정 사례만 남기고 manifest/생성 테이블/No-Python/손상 자료 등 release9개를 재검사했다. source/fixture·EXE/manifest SHA와 정확한 명령/예외는 실행 owner인 RUN-20261031-01의 receipt를 따른다.
- 실제 새 EXE660파일/24,060,889B,659payload 전부 일치, 필수4지역·고지161·system/API imports19. 외부 CWD/Windows-only PATH의 diagnose/dry exit0, typed assets3profiles/5paths·capture0/input0. 새 live 게임/GUI 외관/새 OS는 실행하지 않았다.

**Commit/push assessment: 승인된 Rust-only 개발 checkpoint를 공개했다.** 이는 PR 병합이나 완성된 서모너 런 수용이 아니다. `Behavioral parity = Unknown`, `External contract parity = intentional deltas + unaudited paths`, `confidence = partial audit + comparative regression + bounded prior native/live evidence`를 유지한다. 최초 source 삭제 제외 조건과 기본 전환 대기는2026-10-31 사용자 지시로 이번 범위에서 해소됐다. 남은 게임 목표를 미완료로 보존한 채 새 브랜치 `codex/rust-native-session-close`로 commit/push했으며 정확한 소스 commit·원격 동일성·공개 승인은 RUN-20261031-01의 receipt가 소유한다. `Safe to merge as fully completed migration/game acceptance = Not yet`; 새 실게임·전체 logic-unit/환경 감사는 남아 있다.

## Residual Risks And Next Action

R01–R18의6정상/12실패/연속0은 개발 종료 수이며 전체 처치/키/Canyon 수용이 아니다. J 실게임·종점 재획득·graph 장기 상한·아이템 획득 확정·북쪽10연속·남쪽 이후와50ms는 미완료다. 현재 고정 자료는 기존 관찰 사례만 보존하고 미래 신규 입력의 Python 실행 비교를 제공하지 않는다. 새 기대값과 변경 codec의 독립 판독은 별도 검토한다. 다음 게임 입력은 현재 조작 인계와 화면/안전 조건을 확인하고 현재 RUN/SPEC-0031에서 이어간다.
