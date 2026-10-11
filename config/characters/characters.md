# characters.json

Purpose: character metadata, movement/buff bindings, current equipment FCR, and character-owned survival policies.

## game_modes

Reference values for character classification; [Game Modes](../../docs/features/game-modes.md) owns the marker and row-detection design.
These notes describe intended visual classification; the config loader does not automatically scan or select in-game character rows.

- `progression_modes`: examples include `ladder` and `standard`.
- `ruleset_families`: examples include `rotw` and `resurrection`.
- `detection_notes`: human notes for distinguishing character list markers. Use explicit present-versus-absent rules so ladder/standard and Resurrection/ROTW are not ambiguous.

## characters

Each entry is one known character.

Fields:

- `display_name`: readable in-game name.
- `progression_mode`: one of the progression modes.
- `ruleset_family`: one of the ruleset families.
- `preferred_run_profile`: default run profile for that character.
- `actions`: character-specific input settings described below.
- `survival`: optional character policy for the shared input-free state/decision contract below. Missing or `null` stays unconfigured; another character's policy is not substituted.
- `character_class`: optional canonical class ID for shared casting lookup, such as `sorceress` or `paladin`. Names and bindings do not establish class identity.
- `casting`: optional reference selection and current equipment FCR, described below. Missing or `null` stays unconfigured.

The loader initially selects the first configured character. The GUI `Character` selector changes the active config and resolves its preferred run profile when available; select the matching character separately in the game. Summoner starts each room on that character's main weapon set and sends the configured balanced buff sequence without inventory/weapon-tab inspection. Personal key/FCR/buff policies remain config; reusable class/HUD/terrain images belong in shared assets.

## actions

Fields used by current Summoner or the shared movement/buff APIs:

| Field | Behavior |
| --- | --- |
| `movement_skill_key` | Required movement key for the north route. Defaults to `null`. |
| `movement_travel_mode` | Shared movement API: `hold` keeps the movement key down during travel; `press` sends individual presses. Defaults to `hold`. Latest Summoner uses its continuous held-input driver. |
| `movement_reposition_mode` | Shared movement API's `hold`/`press` input mode for repositioning. Defaults to `press`. |
| `primary_attack_skill_key` | Current Summoner's bounded nearby Nova binding; the selected character must configure it. Defaults to `null`. |
| `pre_run_buff_order` | Ordered input tokens, including repeated tokens. The model permits mouse tokens, but current Summoner's keyboard buff consumer rejects `left-click` and `right-click`. Defaults to an empty list. |
| `buff_action_pause_seconds` | Settle time after each matching buff token, in seconds; keys are normalized to lowercase. Current Summoner defaults unspecified tokens to 0.4 seconds. The common continuous executor requires an explicit caller default. Defaults to an empty mapping. |

The loader also preserves `left_skill_key`, `interact_skill_key`, `buff_skill_keys`, `attack_pattern`, and `engagement_style` for broader action behavior. The current buff sequence is driven by `pre_run_buff_order`, not by `buff_skill_keys` alone.

Flash's current sequence is `w → a → a → s → d → F1 → w`, as confirmed by the owner on `2026-10-09`. The shared state contract permits initial/renewal buffs only in a verified clear field outside town, including outside every Act's town waypoint. Latest Summoner uses the separately approved nearby/resource dispatch policy described in [field buffs](../../docs/features/field-buffs.md); it sends the sequence automatically and records effect status as unconfirmed. The old Arcane token executor was removed.

The owner identifies `d` as 마력 보호막/에너지 쉴드 ([Energy Shield](https://classic.battle.net/diablo2exp/skills/sorceress-lightning.shtml)) and `F1` as 냉기 갑옷. The exact English cold-armor subtype still needs an in-game tooltip comparison with the [Arreat Summit cold skills](https://classic.battle.net/diablo2exp/skills/sorceress-cold.shtml). These names do not change the configured token order or establish visual effect confirmation.

Flash currently supplies 0.4 seconds for each `w`, `a`, `s`, `F1` and `d` token. The [common continuous executor](../../docs/features/field-buffs.md) consumes these gaps without waiting for human confirmation between keys. Capture/reading and transient unknown-HUD waits add time between actual requests. These are configurable supervised-test settings, not measured optimal cast intervals; [RUN-20261005-06](../../docs/plans/run/run-20261005-06-field-control-loop.md) owns actual timing evidence.

## survival

The [shared contract](../../diablo2/common/survival.py) and [approved feature](../../docs/plans/feature/feat-0003-character-survival-state.md) load these values independently for each character. JSON edits change the loaded policy; the GUI selects profiles and has no policy editor. Latest Summoner consumes live resource ratios, emergency recovery, nearby combat and timed rebuffing; their live acceptance and actual resource pickup remain incomplete. See [current execution](../../docs/features/summoner-run.md).

The [HUD/belt observer](../../docs/features/survival-observation.md) supplies candidate readings for the reviewed layout through an input-free CLI and latest Summoner. Resource ratios use current/maximum from the same frame, including after Battle Orders or equipment changes; unreadable values are not filled with old maxima. Those readings alone do not establish clear surroundings or a buff effect.

[Buff confirmation and belt inspection](../../docs/features/field-buffs.md) add a shared input-free transaction consuming this profile's order and timed rules. Key acknowledgement alone does not start a confirmed timer; fresh effect/sequence/battle-equipment evidence is required. Belt toggling invalidates visibility until observation; movement requires positive fresh closure after inspection. Expected battle equipment belongs to the individual profile/consumer, while the shared I/II reader only identifies the visible tab. Current Summoner instead requires the main weapon set at start, performs no inventory inspection, and schedules renewal from actual key requests without claiming this confirmation transaction passed.

| Field | Meaning |
| --- | --- |
| `life_potion_below`, `mana_potion_below` | Ratios in `(0,1]`; strictly below either threshold requests one configured potion. Flash uses `0.5` / `0.1`. |
| `potion_kind` | Resource kind counted for recovery, depletion and replenishment. Flash uses `purple_potion`; the current observer supports the captured large full-rejuvenation appearance only. Other appearances require separate evidence. |
| `potion_pickup` | Optional `preferred_kind`, with paired `fallback_kind` / `fallback_at_or_below`. Flash prefers `full_rejuvenation_potion` and permits `rejuvenation_potion` only at a currently confirmed count ≤6. This selects allowed types; it does not identify labels or authorize input. |
| `replenish_at_or_below` | Inclusive count requesting replenishment. Flash uses `6`; this request does not define when a running combat mode ends. |
| `exit_on_empty` | Confirmed zero recovery potions requests exit when true, otherwise hold. Flash uses true; the consumer must perform Esc/save-exit and confirm the result. |
| `belt_toggle_key` | Configured belt observation binding; Flash uses backtick (U+0060). The string is data here; platform key-token validation remains with the input adapter. |
| `belt_columns` | Unique `column` (1–4), `key`, `kind`, `capacity` (1–4), `target_count` (0–capacity or null). Potion columns need explicit targets. |
| `buffs` | Unique `name`, configured `key`, positive `duration_seconds`, and `renew_before_seconds` in `[0,duration)`. Only confirmed casts start the shared state contract's confirmed timers. Current Summoner separately tracks request deadlines as described above. |

Flash reserves 1–3 for purple potions (four each, total 12), and 4 for four town-portal scrolls, as confirmed on 2026-10-09. A first observation of two TP scrolls therefore reports a deficit of two. Profiles that explicitly keep a null target still use their first valid observation as the replacement baseline. No scroll shortage/exit policy is inferred from this.

`potion_pickup_kinds(now, max_belt_age_seconds=...)` returns permitted types only for a fresh, known belt below its potion target. Flash permits small rejuvenation at six or fewer, then stops permitting it as soon as the current count reaches seven; the separate replenishment-combat mode may still continue to twelve. Confirmed zero with `exit_on_empty`, full/unknown/stale/future belts and missing pickup policy return no types. Safe approach, label identity and actual acquisition remain the pickup consumer's responsibility. [FEAT-0011](../../docs/plans/feature/feat-0011-potion-pickup-policy.md) owns this input-free contract. Identification/portal tomes, gold and other resupply values are deferred by the owner.

Flash's timed buff is `battle_orders` / `s` with 150 seconds and a 10-second renewal margin. Other timed buffs can be added with their own verified durations. Initial/renewal requests require field location, clear surroundings and healthy observed resources; pause does not stop elapsed time. Missing effect/consumption/acquisition evidence is never created by a request.

`CharacterSurvivalState` belongs to one character and room. Producers provide context, increasing sequence numbers, monotonic observed times, complete belt kinds/counts and confirmed changes. Manual changes or contradictions require a new belt observation. `decide` additionally requires explicit current-screen and belt age limits; no measured live defaults are supplied by this feature. Consumers must coordinate input, confirmation, retry/cooldown, stop and safe escape separately.

## casting

The [shared casting catalog](../game-rules/casting.md) owns class/skill thresholds. [FEAT-0004](../../docs/plans/feature/feat-0004-class-casting-rules.md) loads the optional fields and provides input-free estimates. The [common field controller](../../docs/features/field-control.md) and latest Summoner GUI runtime consume the selected character's class/FCR; reference frame timing does not prove actual arrival. The GUI selects profiles and has no FCR editor.

| Field | Meaning |
| --- | --- |
| `rule_set_id` | Explicit shared catalog selection; no fallback to another table. |
| `skill_id`, `form` | Exact skill/form context; the initial references use `teleport` / `normal`. |
| `fcr` | Current equipment's total FCR as an integer ≥0, or `null` when unknown. Bool, fractional and negative values are rejected. |

Flash is configured as `sorceress`, using `d2r-normal-teleport-reference-v1`, with user-confirmed FCR `105` on `2026-10-05`. This selects 8 reference frames. Other profiles' classes/FCR remain unconfigured. Missing active character/class/FCR/settings or unsupported context returns `hold`; the casting adapter does not choose another character when selection is invalid.

Update FCR when the active equipment changes, then query again. This user-reported value does not automatically verify the equipped set. Runtime observation corrections belong to session state and do not rewrite the class table. See [adaptive movement ownership](../../docs/project/architecture.md#future-adaptive-teleport-movement) and [research limits](../../docs/research/class-cast-rate-reference.md).

## Expansion direction

This file is the right place for future character-level overrides such as:

- preferred difficulty
- loot additions or removals
- additional recovery/escape rules after their live evidence and feature boundary are confirmed
