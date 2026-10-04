# runs.json

Purpose: define the reusable run-profile catalog and its supported settings. Design intent lives in [Farm Profiles](../../docs/features/farm-profiles.md); current execution coverage lives in [Summoner Run](../../docs/features/summoner-run.md#current-staged-implementation).

## Top-level fields

- `run_profiles`: named run definitions.

Payload modules such as Summoner Run should resolve the run profile they need directly from this catalog.

## run_profiles.<name>

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

These are model defaults when a field is absent; seeded profile values can override them. The current north-route stage does not execute a full hunting, loot, or life-management engine, so these settings are not evidence of active survival handling.

## Example

Use `run_profiles.summoner` when testing the Summoner flow.
