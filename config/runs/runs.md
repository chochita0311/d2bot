# runs.json

Purpose: define the reusable run-profile catalog and its supported settings. Design intent lives in [Farm Profiles](../../docs/features/farm-profiles.md); current execution coverage lives in [Summoner Run](../../docs/features/summoner-run.md#current-staged-implementation).

## Top-level fields

- `run_profiles`: named run definitions.

Payload modules such as Summoner Run should resolve the run profile they need directly from this catalog.

## run_profiles.<name>

Summoner 개발 검증 순서는 북쪽 단독10연속→같은 방 북남 조합10회→동쪽 단독10회→북동남 통합→북동남서다. 이 단계/집계 조건은 [통합 run](../../docs/plans/run/run-20261009-07-summoner-live-integration.md#development-resume-20261020)이 소유하며 현재 JSON에 통합 순서가 구현됐다는 뜻은 아니다. 정비/TP/아카라 보급은 현재 시험에 추가하지 않는다. 공격은ON을 유지하고 경로상 실제 근접 위협만 필요한 만큼 처리한다.

- `name`: internal profile id.
- `goal`: short summary of the run.
- `description`: longer explanation.
- `hunting`: target, waypoint, route, and combat intent.
- `loot`: run-level loot preferences on top of shared fixed items.
- `life`: potion, retreat, and safety thresholds.
- `run_specific_rules`: assumptions that belong only to this run.
- `templates`: extra run-only templates, such as boss or loot visuals.

## hunting

Use for navigation and combat intent.

| Field | Loader default | Meaning |
| --- | --- | --- |
| `objective` | `watch` | Run navigation/combat intent. |
| `waypoint_act`, `waypoint_name` | `null` | Target waypoint act and name. |
| `target_monsters`, `target_areas` | `[]` | Named targets. |
| `route_notes` | `[]` | Human-readable route intent. |
| `fight_style` | `safe` | Intended combat style. |
| `search_timeout_seconds` | `60` | Intended search time limit. |
| `disengage_on_uncertainty` | `true` | Intended uncertainty fallback. |

## loot

Use for run-local rules, not the global fixed-item list.

| Field | Loader default | Meaning |
| --- | --- | --- |
| `keep_labels`, `ignore_labels` | `[]` | Run-local keep/ignore intent. |
| `potion_columns_reserved` | `2` | Belt columns reserved for potions. |
| `free_inventory_slots_min` | `6` | Intended minimum free inventory slots. |
| `identify_before_drop` | `false` | Intended identification rule. |
| `pickup_gold` | `false` | Intended gold pickup rule. |

## life

Use for survival thresholds.

| Field | Loader default | Meaning |
| --- | --- | --- |
| `use_healing_potion_below` | `0.65` | Life ratio for healing-potion intent. |
| `use_rejuvenation_below` | `0.35` | Life ratio for rejuvenation intent. |
| `emergency_retreat_below` | `0.2` | Life ratio for retreat intent. |
| `use_mana_potion_below` | `0.3` | Mana ratio for potion intent. |
| `town_portal_on_risk` | `true` | Intended escape behavior. |
| `stop_on_death_screen` | `true` | Intended death-screen fallback. |
| `belt_restock_healing_below`, `belt_restock_mana_below` | `4` | Intended potion restock counts. |

These are model defaults when a field is absent; seeded profile values can override them. Latest Summoner uses the character policy below for live resource checks and emergency recovery, rather than treating this run-level intent as a complete hunting, loot, or life-management engine.

The input-free [character survival contract](../characters/characters.md#survival) uses each character's `survival` policy directly; current Summoner consumes that character policy. The contract does not fall back to this run-level `life` section or send input itself. These older fields remain available in the model; consumers must choose an explicit owner rather than silently mixing the two policies. Current live acceptance and actual recovery/acquisition limits remain in [Summoner Run](../../docs/features/summoner-run.md).

## Example

Use `run_profiles.summoner` when testing the Summoner flow.
