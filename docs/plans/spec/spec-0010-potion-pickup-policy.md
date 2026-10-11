# SPEC-0010: 포션 종류 회수 계약

- ID: `spec-0010`; Status: `implemented`; Run: `run-20261009-04`; Attempt: `1`
- Feature: [FEAT-0011](../feature/feat-0011-potion-pickup-policy.md); Profile: `foundation-contract`
- Evaluators: Contract, Functional; Created / Updated: `2026-10-09`

`CharacterSurvivalPolicy.potion_pickup`은 선택적 `PotionPickupPolicy(preferred_kind, fallback_kind, fallback_at_or_below)`다. fallback 종류와 임계값은 함께 주거나 모두 생략한다. 종류는 비어 있지 않고 서로 달라야 하며 임계값은정수0이상·포션 목표 이하이다. 기본값은 None이며 기존 프로필의 포션 종류를 회수 가능한 세부 종류로 추측하지 않는다.

`CharacterSurvivalState.potion_pickup_kinds(now, max_belt_age_seconds=...)`는 같은 방/캐릭터에서 확인한 벨트만 사용한다. 현재 시각/나이 상한은 유한·양수, 미래/오래된 stamp·미확인 잔량·미설정 policy는 빈 tuple이다. 목표량 이상은 회수 없음이고, exit_on_empty에서0개도 회수 없음이다. 나머지는 preferred, 현재 잔량이 fallback 임계값 이하일 때만 fallback을 추가한다. 보충 전투의12개까지 유지되는 모드와 별개로 매번 현재 잔량을 계산한다.

Flash JSON은 preferred=full_rejuvenation_potion, fallback=rejuvenation_potion, fallback_at_or_below=6,4열 target_count=4다. aggregate purple_potion 잔량은 유지하며 종류 인식/획득 확인·보호/안전/접근 예산은 실제 회수 소비자 의무다. 현재 HUD의 큰 포션 외형만으로 작은 포션을 확인했다고 주장하지 않는다. 나머지 보급 기준은 보류한다.

검증: policy 유효/잘못된 종류·임계값,7/6/0/12,6→7→6,stale/future/unknown,다른 캐릭터,TP4장 결손과null baseline 보존. 변경 Python Black/compile와 전체 unittest. 게임 입력 없음.
