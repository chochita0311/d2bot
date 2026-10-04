# Farm Profiles

This document defines the reusable structure for farming runs.

## Design goal

Separate farming behavior into four layers:

- hunting: how we reach and kill the target
- loot: what we keep and what we ignore for the active run, on top of shared fixed-item loot
- life: when we drink, retreat, or stop
- run-specific rules: assumptions that belong only to one farm

That keeps the hunting, loot, and life engines reusable across Summoner, Pindleskin, Diablo, and later runs.

## Shared fixed-item loot

Use `shared_loot.fixed_items` for priceless fixed items that should stay on the basic keep list regardless of run payload or character.

Examples:

- keys
- gems
- other fixed items with deterministic labels

This shared item list should drive the first reusable loot pickup flow.
Advanced rare, unique, or affix-threshold evaluation can be layered on later as a separate rule system.

## Shared model

Each run profile can define the sections below. [Run configuration](../../config/runs/runs.md) owns their supported fields and defaults; this document owns the reusable design intent. Configured intent does not imply that every behavior is already implemented.

### `hunting`

Use this for reusable combat and pathing intent.

See [hunting fields](../../config/runs/runs.md#hunting).

### `loot`

Use this for keep or ignore decisions.

See [loot fields](../../config/runs/runs.md#loot).

### `life`

Use this for safety and sustain rules.

See [life fields](../../config/runs/runs.md#life).

### `run_specific_rules`

Use this only for facts that should not leak into the shared engines.

Examples:

- boss-specific completion conditions
- profile-only exit rules
- map-specific assumptions
- one-off loot exceptions

## Summoner Run profile

The first real profile is the Summoner Run, stored under the internal profile id `summoner`.

Purpose:

- use the Act 2 waypoint system
- travel to Arcane Sanctuary (`비전의 성역`)
- locate and kill The Summoner (`소환술사`)
- keep `key of hate`
- ignore low-value loot and unnecessary full-clear behavior

### Profile-only rules

These should stay in the Summoner Run profile instead of the shared hunting or loot engine:

- Arcane Sanctuary is the target area
- The Summoner is the target monster
- the run succeeds only after the Summoner dies and the key pickup decision is made
- the run should end after approved loot is handled

## Implementation direction

General live action priority and coordination are documented in [run-coordination.md](run-coordination.md).

The code should read the active run profile and eventually hand its sections to:

- a reusable hunting engine
- a reusable loot engine
- a reusable life-management engine
- a thin farm-specific coordinator

