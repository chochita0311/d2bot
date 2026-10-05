# casting.json

Purpose: shared class/skill casting references consumed by the input-free [casting catalog](../../diablo2/common/casting.py). Individual names, equipment FCR and bindings belong in [character configuration](../characters/characters.md#casting).

## casting_rule_sets

The recursive config loader reads this top-level list into `BotConfig.casting_rules`. Each character explicitly selects one rule-set ID. Keep each shared rule set in one file: the loader replaces lists rather than concatenating them.

| Field | Meaning |
| --- | --- |
| `rule_set_id` | Unique non-empty catalog ID, referenced by character `casting`. |
| `ruleset_family` | Exact game-family context; this initial set uses `resurrection`. |
| `revision` | Non-empty data revision, returned with the estimate. The current revision records the reference import date, not a verified client patch. |
| `source_url` | Source provenance returned with the estimate. Loading does not fetch the URL. |
| `evidence_status` | Only `reference` is accepted by this initial contract. |
| `rules` | Non-empty list of unique `character_class` / `skill_id` / `form` contexts. |
| `breakpoints` | Each rule's non-empty list of integer `minimum_fcr` / positive integer `frames` pairs. Starts at FCR 0; thresholds strictly increase and frames strictly decrease. |

The initial `d2r-normal-teleport-reference-v1` set contains Sorceress and Paladin `teleport` / `normal` references from [Librarian's original tables](https://www.mannm.org/d2library/faqtoids/stattab_eng.html). [Research notes](../../docs/research/class-cast-rate-reference.md) own source age, exceptions and applicability limits. The table contains full animation frames, separately from action frames, rendering/capture FPS and observed arrival.

## Lookup behavior

`get_active_casting_estimate(config)` reads the explicitly selected character. `CastingCatalog.estimate(...)` selects the greatest threshold at or below its current FCR, without interpolation. Sorceress FCR 105 yields 8 reference frames; Paladin FCR 105 yields 10. New FCR or catalog revisions require a fresh query; no observation modifies the shared table.

Missing active character, casting settings, class or FCR produces `hold`. Unsupported rule set, game family, class/skill/form also produces `hold`, without choosing another character or table. Malformed catalog/profile fields fail validation rather than inventing values. Empty catalogs are valid unconfigured state. Results carry the requested context and, when a reference is selected, its threshold, frames, ID, revision, source and evidence status.

[FEAT-0004](../../docs/plans/feature/feat-0004-class-casting-rules.md) implements reference lookup only. The later [common field controller](../../docs/features/field-control.md) consumes frames/25 as a reference lower bound and adapts aim timing from confirmed arrival delays. Actual arrival/ready producers, equipment verification and slower-gear live comparison remain incomplete. Other classes, forms, skills and game families have no seeded runtime rule here.
