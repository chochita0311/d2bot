# Field Control

공통 [제어기](../../diablo2/common/field_control.py)는 캐릭터 설정의 키로 F2 이동 유지→F2 해제/F4 공격 유지→F4 해제/전리품 확인→F2 이동 재개를 제공하는 재사용 API다. 기본은 dry-run이며 실제 입력에는 backend와 검증한 관찰 생산자를 명시적으로 연결해야 한다. [runtime](../../diablo2/common/field_runtime.py)은 관찰 한 개만 보관하고 독립적으로 입력 시한을 감시한다. 새 제어기는 기존 북쪽 GUI 시험에 자동 활성화되어 있지 않다.

## Use And Ownership

`create_field_control(config, character_id, room_id, limits)`는 기본으로 입력을 보내지 않는 `DryFieldInput`을 사용한다. live 사용에는 명시적인 backend, 현재 게임 포커스 함수, 창 경계 함수, 관찰 생산자와 같은 시계의 확인된 생존 상태가 필요하다. [ControllerFieldInput](../../diablo2/common/controller.py)은 표준 키보드/마우스 입력에 연결하고, `BotController.attach_field_control(loop)`는 기존 pause/stop 콜백의 키 해제를 연결한다. backend는 key_down/key_up/press/move를 짧게 실행해야 한다.

`FieldObservation`의 적 근접/정리·위치·도착 가능·도착 완료·전리품 확인은 생산자가 검증해 제공해야 한다. [지역 후보](encounter-observation.md)를 이 값으로 자동 승격하지 않는다. `enemy=clear`는 현재 `monsters_clear=True`와 함께여야 하며 보이는/미확인 적은 안전한 버프 장소로 들어갈 수 없다. 마나/생명력은 [같은 프레임의 현재/최대값](survival-observation.md)을 사용한다.

개인 정책과 공유 직업/FCR 카탈로그는 config가 소유한다. 화면/벨트 최신성, 전투 시간, 도착 시간, 포션 결과 대기 시간은 `FieldLimits`에서 명시한다. 시험의 0.5/1000/3/2/1초는 합성 검증용이고 운영 기본값이 아니다. 반경 운영값은 이 모듈에 없다.

## Timing And Confirmation

runtime의 기본 시계는 `time.monotonic`이다. 모든 상태/관찰 시각도 같은 시계여야 한다. 기존 `FramePacket.timestamp`의 wall-clock 시각을 그대로 혼합하지 않는다. 재생은 명시적으로 주입한 상대 시계를 사용한다. 시계가 역행하면 키를 해제하고 해당 제어기를 종료한다.

키 유지의 시한은 최신 관찰 시각+screen_age, 최단 버프 갱신 시각, 전투/도착 상한 중 빠른 값이다. Flash의 확인된 Orders 시각+140초(지속150초−갱신 여유10초)에 새 화면이 없어도 키가 해제된다. 그 뒤 안전한 필드/버프 효과/원래 장비 확인에는 [별도 버프 트랜잭션](field-buffs.md)이 필요하다. 제어기가 `buff`를 반환하는 것만으로 시계를 갱신하지 않는다.

포션 요청은 기존 유지 키를 놓고 1회만 전송한다. 같은 열의 확인된 소비를 `acknowledge_potion`으로 전달한 뒤 최신 회복 상태를 다시 읽는다. 확인이 없거나 실패하면 재전송 없이 hold한다. `reconcile_belt`는 요청 이후 최신 펼친 벨트의 절대 잔량으로 대기를 해소하며 소비량을 다시 차감하지 않는다. 모호한 슬롯은 잔량을 무효화하고 입력을 재전송하지 않는다. 확인된 0개는 화면 지연 중에도 `exit` 요청을 유지한다. 이 요청에는 Esc 메뉴/게임 종료 실행 및 확인을 담당하는 room lifecycle 소비자가 필요하다; 요청을 실제 종료로 기록하지 않는다.

`runtime.publish_hud(reading, scene)`는 같은 프레임의 현재/최대값과 벨트 표시를 공통 관찰에 연결한다. 호출자가 scene과 HUD의 동일 프레임을 보장해야 한다. field/clear/적/도착 정보는 별도로 검증한 생산자 값이며 HUD가 만들어 주지 않는다. 펼친 벨트는 소비 시점에도 최신일 때만 잔량을 갱신하고 이동은 닫힘을 다시 확인할 때까지 보류한다.

Flash <=6개 보충 모드는 목표12개까지 유지한다. 먼 적 접근에는 새 `hunt_aim`과 도달 가능 확인이 필요하며, 처치만으로 포션 잔량이 증가하지 않는다. 전투 후 정리 확인→전리품 확인을 거친 뒤에만 이동한다. 적이 멀어지거나 사라진 것은 정리 확인이 아니다.

텔레포트는 [직업별 참고값](../../config/game-rules/casting.md)의 프레임을 25Hz 논리 시간으로 환산한 하한과 확인된 도착 지연의 이동 평균을 사용한다. 화면 렌더 FPS/실제 시전 프레임을 측정했다고 주장하지 않는다. 도착 확인이 없으면 조준 변경으로 상한을 연장하지 않고 멈춘다.

## Verification

[재생 CLI](../../diablo2/tools/field_replay.py)는 `--config config --character flash --room demo --scenario <json>`로 합성 의미 이벤트만 읽는다. `evidence=synthetic`, 명시적 limits, 최대1000개 event(파일1MiB 이하)가 필요하다. belt/buff/observation/potion_consumed/tick 이벤트를 처리하고 `inputs_sent=0`과 dry 이벤트를 출력한다. 실제 화면 판독 성공이나 live 입력 증거로 사용하지 않는다.

[필드 테스트](../../tests/test_field_control.py)는 키 겹침/전환, 독립 시한, 포션 대기/고갈, 보충 해제, 불확실 처치, 포커스/창 경계, 오류 정리, class/FCR 도착 적응, 시계 역행과 mailbox를 검증한다. 실제 유지 입력, 자동 안전/사망/획득 생산자, 네 방향 완주와 room lifecycle 연결은 [실행 기록](../plans/run/run-20261005-06-field-control-loop.md)의 별도 증거 범위를 따른다.

[감독 입력 probe](../../diablo2/tools/field_input_probe.py)는 명시적 `--live`, 15초 이내의 감독 확인, 긍정 HUD/닫힘/포커스, 최대1초 유지 또는8초 버프 한도를 요구한다. stdin 준비 방식은 backend 초기화 뒤 최신 확인을 받는다. [실행 기록](../plans/run/run-20261005-06-field-control-loop.md)이 짧은 F2/F4·연속 버프 시험의 실제 결과를 소유한다. scene/effect/equipment는 감독 확인이고 완전 제어기의 자동 생산자/GUI 연결을 live 검증한 것은 아니다.
