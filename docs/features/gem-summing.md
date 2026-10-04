# Gem Summing

This document owns gem-combining behavior and maintenance guidance. The implementation is [gem_summing.py](../../diablo2/actions/gem_summing.py); operator controls and live-input scope are in the [README](../../README.md#play-controls).

## Starting State And Completion

Open the supported gem stash and Horadric Cube before starting. The action requires the stash reference and cube transmute button to remain detectable. It uses real mouse/keyboard input; `Stop Action` and `F10` stop the session.

The planner combines non-perfect stacks toward fewer than ten gems per tier. Perfect gems are retained. A slot that fails cube/result verification is blocked for the rest of that session. Completion means no eligible plan remains; the final message alone does not prove that blocked slots are below the target count. Review warnings and the remaining stash state.

## Count And State Contract

- Counts use three-frame consensus, weighted by icon and digit confidence. Slot identity must match the expected gem family/tier before its sample contributes a vote.
- Missing valid votes fall back to tracked counts when available, otherwise the first sample. A fallback warning identifies the affected slots; consensus does not guarantee correct recognition.
- After a verified combine, tracked counts subtract three source gems and add one next-tier gem. Preserve this state across successful batches so noisy rereads cannot revive processed stacks.
- Resync occurs after failed work or the configured number of successful plan steps. Large count changes retain the tracked value instead of accepting the reread.

The implementation owns these tuning constants:

| Constant | Current value | Purpose |
| --- | --- | --- |
| `COUNT_SCAN_SAMPLES` | `3` | Frames per consensus scan |
| `RESYNC_EVERY_SUCCESS_STEPS` | `3` | Successful plan steps between rescans |
| `RESYNC_MAX_DELTA_WITH_TRACKED` | `6` | Maximum accepted difference from tracked counts |

## Cube Verification

Transfer uses Ctrl+Shift clicks. After transmute, require exactly one occupied cube cell and verify its icon against the expected next tier before moving it back. Occupancy and result identity are separate checks. A mismatch blocks the source slot rather than repeatedly scheduling the same uncertain combine.

Warnings about zero or two occupied cells require inspecting transfer timing, leftover items, and occupancy detection. If a slot repeats unexpectedly, compare tracked-count updates with resync decisions before changing movement speed.

## Assets And Recognition Maintenance

- The stash reference is [stash_open_gems_focused.png](../../assets/stash/stash_open_gems_focused.png).
- [Gem icon assets](../../assets/items/gems/icons/) should contain the icon without text labels or neighboring-slot noise. Check extraction per family; emerald and skull shapes may need different crop bounds.
- Compare stash-slot and cube-result crops separately before changing icon thresholds. Weak skull matches or high-tier result mismatches need captured evidence, not a blanket threshold reduction.
- Expand digit references with real count crops, especially ambiguous `1`, `7`, `8`, and two-digit transitions. Include brightness and hover states when they affect matching.
- Inspect [cube assets](../../assets/items/horadric_cube/) and settled frames when occupancy disagrees with the screen.

For tuning, capture the source count, transfer, transmute result, tracked update, and resync in one recording/log sequence. Preserve state validation when reducing movement or settle delays. Evidence ownership and retention are described in the [developer guide](../project/developer-guide.md#evidence-handling).
