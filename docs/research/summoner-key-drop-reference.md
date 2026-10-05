# 서모너 열쇠 드롭과 위험 참고 자료

- 조사일: `2026-10-04` (Asia/Seoul)
- 용도: 서모너 PRD의 파밍 조건·통계·위험 자료. 현재 클라이언트의 실제 드롭률 측정 또는 실행 평가가 아니다.
- 대상 사용자 확인: 온라인 소프트코어 기본 Resurrection 원소술사 Flash. 현재 인원·버전·공포의 영역(TZ)·라벨 표시 조건은 아직 확인하지 않았다.

## 열쇠와 계획 기준값

[Basin Wiki의 Pandemonium Event](https://www.theamazonbasin.com/wiki/index.php/Pandemonium_Event)는 지옥 난이도의 서모너가 증오의 열쇠를 드롭하며, 솔로 확률을 `0.0861533746`(약 **8.615%**, 약 **11.61회 처치당 한 번 이상 드롭**)로 제시한다. MF는 열쇠 확률을 올리지 않으며, 한 처치에서 여러 열쇠가 나올 수 있다고 설명한다. 열쇠의 일반 인벤토리 공간은 1×2로 안내한다.

이 값을 **실험 예산의 잠정 기준**으로 사용한다. 현재 게임 계열/패치/인원별 검증된 확률로 확정하지 않는다. 횟수는 클릭·방 생성 횟수가 아니라 관찰이 충분한 확정 처치 수다. 여러 열쇠가 나올 수 있으므로 드롭 발생 방의 빈도와 처치당 열쇠 수를 별도로 센다.

[Diablo Wiki의 The Summoner](https://diablo2.diablowiki.net/The_Summoner)는 1–2/3–4/5–6/7–8인 표를 각각 8.6/11.6/12.5/12.8%로 제시하지만 해당 페이지는 2012년 수정 자료다. 같은 페이지의 싱글 플레이 이벤트 제한은 구 LoD 설명으로 D2R에 그대로 적용할 수 없다. 인원 표도 온라인의 인원·파티 조건에 대한 현재 검증 없이 운영 값으로 사용하지 않는다.

[Maxroll 드롭 계산기](https://maxroll.gg/d2/d2-drop-calculator#monster=boss,summoner)는 이번 조사 도구에서 내용을 읽지 못했다. 검색 결과의 숫자나 포럼의 상충하는 `1/11`, `1/12`, `1/15`를 현재 정답으로 채택하지 않았다. 실제 조건을 맞춘 계산기 결과 또는 재현 가능한 현재 근거가 확보되면 기준값과 버전을 함께 갱신한다. 내부 게임 상태를 읽어 검증하는 방식은 프로젝트 범위 밖이다.

## 드롭 운과 인식 실패의 구분

독립 처치마다 같은 잠정 확률 `p = 0.0861533746`를 가정하면, `n`회에서 한 번도 드롭이 없을 확률은 `(1-p)^n`이다. 아래는 이 가정으로 계산한 값이며 게임에서 측정한 결과가 아니다.

| 확정 처치 수 | 열쇠 드롭 0회 확률 |
| --- | --- |
| 20 | 약 16.50% |
| 30 | 약 6.70% |
| 50 | 약 1.11% |
| 100 | 약 0.0122% |

적어도 한 번 드롭을 볼 확률이 95%를 넘는 첫 정수 횟수는 이 가정에서 34회다. 이는 드롭 보장이나 회수 성공 보장이 아니다. 열쇠가 안 나오면 먼저 처치·난이도·필터·라벨 가림·사후 관찰 누락을 확인하며, 재시도할 때 다음 드롭이 보장된다고 생각하지 않는다.

100회는 기능이 안정된 뒤의 초기 관찰 표본으로만 제안한다. 이 기준 확률 근처에서 드롭 방 비율의 95% 오차 폭 ±3%p를 목표로 하는 정규근사 표본은 약 337회이므로 수집 예산으로 약 350회를 검토할 수 있다. 실제 보고는 Wilson 구간과 조건·관찰 누락을 함께 제시한다. 조건 변화·상관된 자료·여러 열쇠 수량을 단순 베르누이 빈도에 섞지 않는다.

## 게임 계열과 화면 조건

[Blizzard의 Reign of the Warlock 안내](https://news.blizzard.com/en-us/article/24243863/rain-annihilation-in-reign-of-the-warlock)는 Classic/Resurrected/ROTW 계열 분리와 아이템 라벨에 영향을 주는 Loot Filter를 안내한다. 따라서 현재 게임이 단순히 실행 중이라는 사실만으로 라벨·보관 방식·템플릿 호환을 확정하지 않는다. 필터로 숨겨진 아이템은 화면 인식기가 직접 검출할 수 없다.

현재 Flash 설정의 `resurrection` 메타데이터와 사용자 확인이 일치한다. 실제 버전·라벨 표시 조건은 별도 화면 확인이 필요하며, 이 조사에서는 계열 이동이나 설정 변경을 하지 않았다.

## 몬스터 위험과 인식 자료

| 대상 | 자료에서 확인한 특징 | 이 프로젝트에서 필요한 관찰 |
| --- | --- | --- |
| Summoner | 화염/냉기 주문과 Weaken 사용, 단상 앞 방해 몬스터가 추가 피격을 유발할 수 있음 | 단상·이름/체력·주문 효과·처치 전후, 안전한 접근 |
| Specter/Wraith | 마나를 빼앗으며 지옥의 물리 면역이 안내됨 | 노바/텔레포트에 필요한 마나 고갈, 겹친 적과 길 가림 |
| Ghoul Lord/Vampire | 화염 주문, 지옥의 냉기 면역이 안내됨 | 화염 지면·공격 효과·생명력 변화; 현재 노바 빌드로 별도 검증 |
| Hell Clan | 근접 적이며 추가 화염 피해 가능성 안내 | 근접 압박·계단/좁은 길 차단·노바 효과 아래 식별 |
| Lightning Spire | 원거리 번개 위험, 높은 저항, 아이템 드롭 없음 | 무조건 공격 대상으로 두지 않고 위험 회피 판단 검토 |

근거는 Blizzard의 [Summoner](https://classic.battle.net/diablo2exp/monsters/act2-summoner.shtml), [Wraith](https://classic.battle.net/diablo2exp/monsters/act1-Wraith.shtml), [Vampire](https://classic.battle.net/diablo2exp/monsters/act1-vampire.shtml), [Goatman](https://classic.battle.net/diablo2exp/monsters/act1-goatman.shtml), [Lightning Spire](https://classic.battle.net/diablo2exp/monsters/act2-lightningspire.shtml) 안내다. 구 게임의 기본 위험 참고이며 현재 공포의 영역·고유 적·캐릭터 저항을 대신하지 않는다. 특정 면역 대응 스킬이나 생존 수치를 임의로 추가하지 않는다.

## 아이템·문서·용병의 기본 근거

- 이스트 이상 목록은 Blizzard [Runes](https://classic.battle.net/diablo2exp/items/runes.shtml)의 순서를 따라 Ist/Gul/Vex/Ohm/Lo/Sur/Ber/Jah/Cham/Zod로 정리했다. 회수 기준은 희귀도 통칭이 아니라 사용자가 지정한 명시적 목록이다.
- Blizzard [Charms](https://classic.battle.net/diablo2exp/items/charms.shtml)는 참 그림과 옵션이 연결되지 않음을 설명한다. [Small Charm affixes](https://classic.battle.net/diablo2exp/items/magic/smallcharms.shtml)에서 MF 7%·번개/독 단일 저항 11%·번개 피해 옵션, [Grand Charm affixes](https://classic.battle.net/diablo2exp/items/magic/largecharms.shtml)에서 원소술사 번개/아마존 투창과 창 스킬을 참고할 수 있다. 실제 판정은 감정 후 툴팁의 합산 표시값이며 사용자 기준은 PRD-0007이 소유한다.
- Blizzard [Basics](https://classic.battle.net/diablo2exp/basics/)는 감정/귀환 스크롤과 문서, 상점 보충, 문서당 최대 20장 보관을 안내한다. 현재 기본 Resurrection UI에서 잔량·보충 성공을 별도 확인한다.
- Blizzard [Hirelings](https://classic.battle.net/diablo2exp/basics/hirelings.shtml)는 기존 용병 부활이 능력·경험·장비를 보존하며 새 고용은 다른 결과임을 안내한다. 계획은 기존 용병 부활만 포함한다. 초상화를 숨길 수 있으므로 미검출만으로 사망을 판정하지 않는다.

## 문서 소유

요구와 불확실성은 [서모너 PRD와 실험 계획](../features/summoner-experiment-plan.md), 실제 수집·사람 정답·판정은 승인 이후 해당 런/평가가 소유한다. 조사 자료는 파밍 시작 승인이나 실행 성공 증거가 아니다.
