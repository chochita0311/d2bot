# Run Coordination

This document describes the general coordinator model for live Diablo2 runs.

## Design goal

A real run should not treat hunting, combat, loot, and safety as isolated phases.
The bot should keep watching the live screen and choose the highest-priority safe action at each moment.

## Core concerns

A reusable run coordinator should continuously manage:

- hunting and pathing
- combat and target kill actions
- loot watching and pickup decisions
- life and emergency safety behavior

## Priority order

Use this default order unless a run has a strong reason to override it:

1. life protection and emergency actions
2. active combat or mandatory blocker kill
3. fixed-item loot pickup if safe
4. route progress and search movement

## Practical rules

- keep a loot watcher active during the hunt instead of only after boss death
- do not let loot clicks interrupt unsafe combat situations
- allow the run to temporarily switch from hunting to pickup and then resume hunting
- keep boss or survival situations stronger than loot greed
- treat the coordinator as one loop, not several independent clickers competing with each other

## Implementation direction

A future run payload should behave like one coordinator loop that:

- reads the current frame
- evaluates safety first
- checks for mandatory combat actions
- checks for safe fixed-item pickup opportunities
- resumes route progress when no higher-priority action is needed

This model should be reusable across Summoner, Diablo, Pindleskin, and later runs.


## Loot approach policy

Loot pickup should not assume that clicking a visible label is enough in every context.
The coordinator should treat approach and pickup as separate steps.

Default interpretation:

- in town, the character will usually walk to the item after the click
- in the field, the character should first move close enough to the item, usually by teleport, and then click the label

That means a field pickup should eventually follow this sequence:

1. detect wanted item label
2. move close enough to the item
3. click the item label to pick it up
4. resume the coordinator loop

Practical note:

- town loot can tolerate slower walk-based pickup timing
- field loot should prefer teleport-based reposition before the pickup click when the build allows it
- this approach rule belongs to the shared coordinator model, not to one run only
