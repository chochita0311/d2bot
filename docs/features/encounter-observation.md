# 공통 지형·몬스터 조우 관찰

[FEAT-0007](../plans/feature/feat-0007-arcane-north-encounters.md)의 입력 없는 경계다. [지역 관찰기](../../diablo2/common/encounter_vision.py)는 명시한 지역 manifest의 템플릿·팔레트를 사용하고, [거리/생존 상태 계약](../../diablo2/common/encounters.py)은 현재 프레임과 호출자가 제공한 개인 정책/확인 근거만 소비한다. 기존 북쪽 자동 실행기와 연결하지 않았다.

## 입력과 결과

- `SceneContext`: 기존 `ObservationStamp`의 캐릭터/방/순번/단일 시계 시각과 지역, 이동 세대, native 화면 크기, 관찰 viewport. 이미지와 viewport 크기는 정확히 일치해야 한다. 임의 리사이즈는 하지 않는다.
- `RegionObserver.scan`: 종류·alive/corpse/landmark **외형**, 원본 화면 박스·점수·선별된 지상 기준점이 있는 후보 목록. 여러 최고점을 보존하고 겹치는 같은 템플릿 최고점을 제한한다. 점수는 확률이 아니며 후보를 실제 개체 수/확정 생존으로 해석하지 않는다.
- `terrain`: 호출자가 지정한 patch의 바닥/우주 팔레트 비율과 `floor-candidate`/`reject-or-unknown`/`unknown`. 밝은 회색만으로 발판 높이·벽·도달 가능·텔레포트 성공을 구별하지 못한다. 통과/거부 한계는 호출자가 명시한다.
- `proximity`: 현재 프레임의 플레이어 발·적 지상점 사이 거리 `hypot(dx/native_width, dy/native_height)`와 near/boundary/far/unknown. 화면상의 타원형 정규화 거리이며 세계 좌표/노바 실제 사거리 주장이 아니다. `CombatRadius(enter, leave)`는 반드시 명시하며 기본 운영값은 없다. 이전 near 유지에는 동일하게 연결한 `target_id`, 방/캐릭터/이동 세대와 유효한 직전 관찰이 필요하다. 미연결 대상은 `target_id=None`으로 전달하고 이전 상태를 유지하지 않는다.
- `life_state`: 명시적 양성 근거의 alive, 연결된 alive→dead 변화와 **독립 최신 사후 관찰**의 dead, 그 외 unknown. 외부 생산자가 대상 ID·변화 연결·근거 출처를 책임진다. `template-candidate`, 미검출, 화면 이탈, 노바 효과, 개체를 연결하지 못한 사체 무더기로 사망을 선언하지 않는다. 같은 이미지의 반복 호출도 독립 관찰이 아니다.

호출 시 `now`와 stamp는 같은 단일 시계 영역을 사용하고 `max_age`를 명시한다. 새 텔레포트/카메라 이동 뒤 이동 세대를 갱신하고 플레이어/적 지상점을 다시 생산한다. 과거 후보를 현재 화면으로 바꿔 끼우지 않는다. 현재 API는 입력/공격 요청/안전한 필드 판정을 생성하지 않는다.

## 사용과 검증 범위

공통 자산 owner는 [Arcane manifest](../../assets/regions/arcane-sanctuary/remastered-ko-1922x1140/README.md)다. 다른 지역은 별도 manifest/자료로 구성하며 공통 코드를 Flash나 Summoner 전용으로 복제하지 않는다. 개인 반경의 실제 숫자는 다수 정답 장면과 공격 전후를 대조하여 설정해야 한다. 테스트의 `.10/.20`, 실제 좌표 비교의 `.20/.25`는 진단값이며 Flash 설정에 쓰지 않았다.

실제 north alive 장면의 사람 기준점 `(961,620)`에 대해 Ghoul Lord `(700,760)` 거리 약 `.183`, Hell Clan `(1292,425)` 약 `.243`이다. 진단 `.20/.25`를 쓰면 각각 near/boundary이며 당시 실제 입력은 감독자가 수행했다. 자동 반경 공격 성공 근거가 아니다.

[17개 검증](../../tests/test_encounters.py)은 실제 crop 재생/성소 오인 방지/바닥·우주 대조와 합성 반경 경계·다중 후보·잘못된 맥락·가림·사망 연결을 구분한다. 교전 뒤 11개 벨트 crop을 기존 마을 HUD와 합성한 검증도 있어 실제 포션 소모 위치를 판독한다. 이 합성은 동시 교전 HUD/벨트 관측이 아니다.

실제 교전 자료에서 기존 HUD 숫자 판독의 미확인 비율이 높았다. 감독 포션 회복은 수행했으나 무감독 감시/포션/140초 재버프/포션 보충 제어의 완료 근거는 없다. 새로운 live 진행 전 이 제어와 다중 자세/개체 연결·실제 landing 확인을 구현하고 검증해야 한다. 상세 실패·잔량·수집 범위는 [RUN-20261005-05](../plans/run/run-20261005-05-arcane-north-encounters.md)가 소유한다.
