# 생존 화면 관찰

[관찰 기능](../plans/feature/feat-0005-survival-screen-observation.md)은 생명력·마나 숫자와 펼친 벨트의 종류/잔량을 읽는 입력 없는 진단 도구다. 현재 검증한 한국어 리마스터 창의 native 캡처 1922×1140 배치만 지원한다. 게임 입력이나 기존 서모너 실행기에 연결된 자동 회복 기능은 없다. 실제 자료는 [마을 기록](../plans/run/run-20261005-03-survival-screen-observation.md)과 후속 [필드 버프 기록](../plans/run/run-20261005-04-field-buff-confirmation.md)이 소유한다.

## 사용

프로젝트 루트에서 안전한 마을의 게임 창을 최소화하지 않고 실행한다. 잔량을 읽으려면 벨트를 펼치고, 확인 후 backtick으로 닫는다. 명령은 창을 활성화하거나 벨트를 토글하지 않는다.

```powershell
.\.venv\Scripts\python.exe -B -m diablo2.tools.survival_observe --live --frames 3
```

`--image <path>`로 전체 native 창 PNG를 읽을 수도 있다. live/image 중 하나만 선택하고 `--frames`는 1–5개로 제한한다. `--output <path>`는 JSON만 저장하며 원본 화면을 저장하지 않는다. 다른 배치를 강제로 resize해서 현재 레이아웃으로 취급하지 않는다.

선택적 정책 진단에는 `--character flash --room <room-id> --age-limit <seconds>`를 함께 제공한다. `--character`는 정책 선택이며 게임 안의 캐릭터 이름을 확인하는 기능이 아니다. age limit은 사용자가 명시한 진단 값이며 검증된 실게임 생존 지연 상한이 아니다. 각 프레임을 독립적으로 판단하며 버프 시각·이전 잔량을 가정하지 않는다.

## 결과와 개인 설정

- 생명력·마나: `current`, `maximum`, `ratio`, 판독 이유. 같은 프레임의 현재값/최대값으로 비율을 계산한다. 전투 지시나 장비 변경으로 최대값이 달라져도 이전 최대값을 재사용하지 않는다. 값 하나라도 불명확하면 해당 비율은 미확인이다.
- 벨트: `belt_visibility`의 `expanded`/`closed`/`unknown`과 4×4 슬롯의 종류. 닫힘은 별도의 테두리 증거로 확인한다. 전체 슬롯과 [개인 정책](../../config/characters/characters.md#survival)의 열별 종류/capacity가 맞아야 잔량을 전달한다. 닫힌 벨트의 한 줄로 총량을 추정하지 않는다.
- 추적 정보: layout/revision, 프레임 순번, 캡처 시작/획득/판독 종료의 `perf_counter` 시각과 각 구간 소요 시간. 이는 도구 호출 경계이며 게임 렌더러의 원본 프레임 시각을 증명하지 않는다. 별도 프로세스의 값을 버프 타이머로 합치지 않는다.
- 정책 결과: 포션·보충·종료·보류 **요청**만 반환한다. 위치는 `unknown`, 주변 적 없음은 미확인, 버프 효과/시전 시각도 미확인이다. 일반 상태에서 `buff_requires_field` 보류는 출발 허가가 아니다.

Flash의 임계값은 현재 최대 생명력의 50% 미만 또는 최대 마나의 10% 미만이다. 최대 생명력이 985에서 1970으로 바뀌면 985는 정확히 50%이므로 요청하지 않고 984부터 요청한다. 둘이 동시에 낮아져도 포션 요청은 하나다. 임계값·키·벨트 배치·버프 지속시간은 캐릭터 설정, 화면 모양/숫자는 [공통 자산](../../assets/ui/survival/README.md)의 책임이다.

## 자료 범위와 후속 작업

한 Act 1 마을의 전체/한 칸 빈 벨트·닫힘·복구와 후속 아케인 중앙 버프 시전의 최대값 변화를 대조했다. 네 자리 최대값에 따른 라벨 이동과 관찰한 마나 라벨 변형을 처리한다. 정확한 표본 값·crop geometry는 [공통 자산의 출처](../../assets/ui/survival/README.md#provenance), 실제 준비와 대조 결과는 위의 마을/필드 기록이 소유한다.

같은 장면의 자료이므로 독립적인 여러 방/배치의 정확도 증거로 계산하지 않는다. 50%/10%·6/0개 판단, 시전 중 적 접근·지연·모호한 효과는 합성 검증이다. 실제 저체력, 독·격전, 모든 빈 슬롯 위치, 다른 창/언어와 자동 주변 안전·버프 종류 판독은 미검증이다. 숫자 2/6은 패널 추출 마스크로서 실제 HUD의 해당 값 검증이 남아 있다.

현재 `purple_potion`은 관찰한 큰 보라색 완전 활력 포션 외형만 지원한다. 일반 활력 포션이나 다른 그래픽은 자료를 별도로 확보해야 한다. [Arreat Summit의 포션 자료](https://classic.battle.net/diablo2exp/items/potions.shtml)는 일반/완전 활력 포션의 회복량과 벨트 기본 동작을 설명하는 참고다. 현재 HUD의 정답은 직접 캡처에서 확인하며, [Reign of the Warlock의 공식 안내](https://news.blizzard.com/en-us/article/24243863/rain-annihilation-in-reign-of-the-warlock)는 별도 확장/변경 확인에 사용한다. 과거 자료를 현재 모든 게임 계열의 검증으로 확대하지 않는다.

후속 [버프 확인/벨트 종료 계약](field-buffs.md)은 감독 시각 증거와 입력 요청을 분리한다. 런 통합에는 자동 안전 필드/주변 적 확인·효과 증거 생산·포션 소모/획득 확인과 입력 선점이 필요하다. [Arcane 이동 PRD](../plans/prd/prd-0005-arcane-navigation-and-recovery.md)는 이를 바탕으로 웨이포인트 진입, 텔레포트 도착/다음 입력 가능 관찰, 직업별 FCR과 관측 지연에 맞춘 이동을 다룬다.
