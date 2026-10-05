# 마을 방 반복

Flash 온라인 캐릭터 선택에서 방을 만들고 Act 1 웨이포인트→Act 2 마을→Act 1→저장 종료를 진행한다. 현재 승인 범위와 실험 결과는 [FEAT-0002](../plans/feature/feat-0002-supervised-town-room-loop.md) 및 [실행 기록](../plans/run/run-20261004-02-supervised-town-room-loop.md)이 소유한다. 전체 서모너 전투·생존·전리품 처리는 별도 개발 단계다.

## 실행

게임 창에서 Flash를 선택하고 GUI의 `Live town loop (1–3 runs)`를 켠 뒤 Repeat Count를 1–3으로 지정하고 `Start Room Lifecycle`을 누른다. `Difficulty`가 적용된다. 이 옵션이 꺼진 기존 방 생성/종료 모드는 마을 방문을 하지 않으며 빈 반복 횟수를 허용한다.

같은 세션을 CLI에서도 실행할 수 있다. 프로젝트 루트에서 실행한다. 이 명령은 실제 게임 입력을 전송한다.

```powershell
.\.venv\Scripts\python.exe main.py --town-loop 1 --difficulty hell
```

CLI는 정확한 `Diablo II: Resurrected` 창과 window 캡처를 사용한다. GUI에서는 해당 게임 창을 선택한다. F10, Stop Action, CLI Ctrl+C로 중단한다. 완료하면 캐릭터 선택 화면에 남는다. 다른 초기 상태에서 무조건 Play/이동을 시도하지 않는다.

## 관찰과 이동

`TownLoopPlanner`는 입력을 전송하지 않고 최신 화면에서 다음 동작을 결정한다. `RunLifecycleSession`이 입력을 실행하고 다음 화면의 메뉴·지역·캐릭터 선택으로 결과를 확인한다. 빠른 로딩은 로딩 프레임을 반드시 보는 대신 도착 화면으로 판정한다.

Act 1은 실제 웨이포인트를 우선 찾는다. 보이지 않으면 시작 화면의 텐트 단서를 이용해 제한 탐색 순서를 선택한다. 각 이동은 왼쪽 버튼을 누른 채 관찰하며 목표가 나타나면 해제하고 다음 화면에서 다시 위치를 잡는다. 방향은 시간 상한이 있는 구간별로 정하며, 홀드 중 커서를 계속 조정하는 연속 조향은 구현하지 않았다. 탐색 세 번, 발판 접근 세 번, NPC 대화 복구 세 번을 상한으로 한다. 구현된 NPC 복구 경로는 취소 항목 확인→홀드 해제→Esc→재관찰이다. 실제 자동 이동에서 대화가 열리고 복구 후 완주하는 통합 검증은 [실행 기록의 남은 항목](../plans/run/run-20261004-02-supervised-town-room-loop.md#human-review)에 남아 있다.

Act 2 복귀는 도착 지역과 HUD, 발판 주변의 고정 지형 단서, 근처의 푸른 웨이포인트 효과를 함께 확인하고 최신 위치의 발판 안쪽을 클릭한다. 메뉴가 열리지 않으면 최신 화면에서 발판을 다시 찾아 총 세 번까지만 접근한다. Act 2 맵이 고정이어도 카메라와 도착 위치는 고정 좌표로 가정하지 않는다.

창 크기·위치·포커스 변화, 예상 밖 커서 이동, 인식 시간 초과는 추가 입력을 중단한다. WP 패널의 열림/닫힘에 따라 게임이 커서를 반 패널 너비만큼 옮기는 현상은 예상 변위와 긍정 화면 증거가 함께 맞을 때만 인정한다. 표시가 늦으면 최대 0.45초 입력을 보류하며 재관찰한다. 모든 종료 경로에서 세션이 누른 마우스를 해제한다.

## 데이터와 캘리브레이션

- 운용 템플릿 37개와 manifest: ignored `assets/private/town-loop/`. `town-loop-template-<상태/변형>-ko-<crop 크기>` 이름을 사용한다.
- 선별한 보정·재생 프레임 21개: ignored `assets/private/town-loop/frames/`. `town-loop-frame-<상태/배치>-ko-<캡처 크기>` 이름을 사용한다. 선택/난이도/마을/메뉴·NPC와 세 Act 1 웨이포인트 배치를 포함한다.
- 개발 중 자동 시도별 PNG, `events.jsonl`, `result.json`, `timings.json`: ignored `recordings/summoner/evidence/<timestamp>-town-loop/`.

이벤트의 PNG는 동작을 결정한 **입력 전 화면**이며 파일명의 phase는 그 동작 이후에 확인할 상태다. 다음 이벤트 또는 done 화면이 실제 도착/완료 증거다. timings에는 캡처·인식·입력·관찰→입력 시간 및 관찰 주기가 기록된다. 실패·중단 자료도 해당 개발 세션에서 검토할 동안 유지한다.

세션 종료 정리는 [개발자료 정리 기준](../project/developer-guide.md#evidence-handling)이 소유한다. `2026-10-05` 운용 이미지의 바이트를 보존하여 이름을 정리하고 manifest의 `file`/`variants` 및 `source`/`source_frame`/`variant_sources`를 유지할 assets로 연결했다. 원시 시도 폴더는 영구 입력 자료가 아니며, 실제 정리 결과와 역사적 경로의 의미는 [실행 기록](../plans/run/run-20261004-02-supervised-town-room-loop.md#session-close)에서 확인한다. private 자산은 Git에서 제외되므로 새 체크아웃에는 별도 보정 은행이 필요하다.

기준 창 이미지 1267×753으로 정규화한다. 실제 window 캡처는 1922×1140이며 native 관찰 도구의 이미지와 글자/테두리 샘플링이 달라 별도 검토 변형을 사용했다. 창 종횡비가 3% 이상 다르거나 800×450보다 작으면 추측하지 않고 멈춘다. 언어·그래픽 모드·UI 스케일 변경은 재검토가 필요하다.

```powershell
# 입력 없이 프로젝트 캡처 한 장 저장
.\.venv\Scripts\python.exe -m diablo2.actions.town_loop_vision --capture-frame recordings/summoner/evidence/<session-id>/scene.png
# 육안으로 장면을 확인한 뒤 기존 템플릿의 변형 추가
.\.venv\Scripts\python.exe -m diablo2.actions.town_loop_vision --calibrate-frame <frame-path> --scene waypoint1
```

지원 장면은 character, difficulty, act1, act2, waypoint1, waypoint2, exit, world_waypoint1이다. `--calibrate-from`은 최초 은행을 재생성하며 기존 manifest의 추가 단서/변형을 덮어쓰므로 운용 은행에 재실행하지 않는다. 새 단서의 crop/search/threshold/click_offset은 manifest와 원본 프레임에 함께 남긴다. 실행 중 자동 학습은 하지 않는다.

`--town-destination arcane`은 별도 도착 템플릿과 복귀 지점이 있어야 시작할 수 있다. 초기 마을 검증과 구분한다. 현재 실제 검증 범위 및 미검증 항목은 실행 기록을 확인한다.
