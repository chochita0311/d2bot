# Gem Summing Lessons Learned

Date: 2026-03-14
Scope: Diablo II Resurrected stash gem counting, cube transmute verification, and automated gem summing behavior.

## Context

This log captures the practical lessons from building and debugging the gem summing feature in this repo.

The biggest pain points were not just OCR quality. The hard parts were:
- keeping stash state detection stable while the UI is open
- verifying that the cube result is the expected next gem form
- preventing noisy rereads from undoing already successful combines
- balancing speed improvements against fragile visual checks
- handling cases where the cube still contains items or appears empty at the wrong time

This write-up is meant to save future iterations from repeating the same mistakes.

## What Was Built

The current gem summing flow lives in `d2bot/gem_summing.py`.

Current structure:
- detect the opened stash using `assets/stash/stash_open_gems_focused.png`
- detect the Horadric Cube transmute button using template matching
- read gem counts from stash slots using a template-based digit reader
- verify slot identity using separated gem icon assets in `assets/items/gems/icons`
- build a combine plan from current counts
- perform ctrl+shift click transfers to/from cube
- verify transmute output against the expected next gem icon
- maintain tracked counts and use guarded resync logic so bad OCR does not revive already processed stacks

## Important Asset Decisions

### 1. Separate gem icons were necessary

Original gem screenshots included both icon and text label. Those were not reliable as slot identity references.

We split the gem assets into one icon per file, for example:
- `chipped_diamond.png`
- `flawed_diamond.png`
- `normal_diamond.png`
- `flawless_diamond.png`
- `perfect_diamond.png`

This was done for all gem families.

Result:
- slot identity checks became possible
- cube result verification became more direct
- icon-first validation could be added to count reading

### 2. Emerald needed special handling

The first generic icon split produced noisy emerald assets because the crop still captured label noise and right-side artifacts.

Lesson:
- do not assume one crop rule works equally well for every gem family
- if an asset family looks thin, shifted, or unusually small, re-extract it with gem-specific bounds
- emerald was the first clear example of this

### 3. Skull icons remain weak

Even after separation, skull slots often fall back during consensus reads.

Observed repeated low-confidence/fallback areas:
- chipped skull
- flawed skull
- normal skull
- flawless skull
- perfect skull

Likely reasons:
- skull shape and contrast differ more from the other gem families
- current icon crop normalization may be too simplistic for skull silhouettes
- the threshold for icon trust may be slightly too strict for skull slots

## Count Reading Lessons

### 1. Single-frame reads were too fragile

The original count path read each slot from a single frame. This was fast enough, but not trustworthy enough.

Failure pattern:
- one noisy frame could turn a correct tracked value like `9` back into `21`
- the planner would then schedule the same slot again
- this caused repeated attempts and, in one case, effectively an endless loop on `diamond normal`

### 2. Consensus read is safer than one-shot OCR

The current system reads multiple frames and combines them:
- three count samples per scan
- each sample contributes a weighted vote
- votes are weighted by icon confidence plus digit confidence

This does not solve OCR perfectly, but it meaningfully reduces random single-frame mistakes.

### 3. Icon-first validation helps constrain bad reads

The count reader now checks whether the slot icon matches the expected gem family and tier before fully trusting the number result.

Lesson:
- reading digits without first validating slot identity is too permissive
- a count is only meaningful if the slot itself looks like the expected gem

### 4. OCR still needs more training data

The current digit reader is template-based and learned mostly from one stash reference image.

This is useful for bootstrapping, but not enough for precision work.

Future improvement path:
- collect many real count crops for each digit `0-9`
- focus especially on ambiguous cases like `1`, `7`, `8`, and two-digit transitions
- include examples across brightness changes and hover/no-hover states if they exist

## Planning and Tracking Lessons

### 1. Speed optimization introduced a major logic bug

To make summing faster, the bot was changed to perform multiple combines from one count read instead of rescanning after every combine.

The idea itself was valid.

The bug was in the resync behavior:
- tracked counts were updated correctly after successful combines
- then a new OCR rescan happened immediately
- the noisy rescan restored old wrong values
- the planner repeated already-processed slots forever

Observed example:
- `diamond normal` kept getting scheduled repeatedly because the bad reread restored it to `21`

### 2. Tracked counts should be the source of truth after success

The fix was not to remove rescans entirely.
The correct lesson was:
- after a successful combine, prefer tracked counts
- only resync periodically or after failures
- on resync, do not accept huge jumps away from tracked values without skepticism

### 3. Guarded resync is essential

The current logic now:
- keeps tracked counts after successful steps
- only resyncs after failures or after several successful steps
- clamps suspicious reread jumps if they differ too much from tracked counts

This solved the infinite-repeat behavior and allowed the run to complete.

## Cube Interaction Lessons

### 1. ¡°Cube had 2 items¡± means the transfer/transmute state is not clean

Frequent warnings like:
- `cube had 2 item(s) after transmute`
- `cube had 0 item(s) after transmute`

usually mean one of these happened:
- the source move did not behave as expected
- a previous item remained in the cube
- the occupancy detector read the cube too early or too late
- the visual occupancy heuristic is too crude for some states

This is not only an OCR problem. It is also a state-transition and timing problem.

### 2. Result verification must be stricter than occupancy detection

The current implementation first detects whether the cube contains something, then verifies the result icon against the expected next gem form.

This split is useful:
- occupancy tells us whether something is present
- icon verification tells us whether it is the right thing

Lesson:
- do not trust occupancy alone
- do not trust icon verification alone if the cube state is not clean
- both are needed

### 3. Some result checks are still too brittle

Observed repeated failures:
- diamond flawed result mismatch
- topaz flawless result mismatch
- sapphire flawless result mismatch
- skull flawed/normal/flawless result mismatch

This suggests one or more of the following:
- the icon crop for result cells is not perfectly aligned
- expected icon assets do not match the in-cube presentation closely enough
- skull and some higher-tier gems may need lower/adjusted thresholds or better icon references

## Speed Lessons

### 1. Raw speed improvements were worth keeping

The following changes helped responsiveness:
- shorter movement delays
- fewer move interpolation steps
- shorter settle times around clicks/transmute
- batching multiple combines from one trusted state

These changes should stay, as long as state validation remains conservative.

### 2. Speed without state protection is dangerous

The core lesson is not ¡°go slower.¡±
The core lesson is:
- faster actions are acceptable
- faster rereads are not acceptable if they overwrite reliable tracked state with noisy OCR

## What Worked Well

These changes produced clear gains:
- separated gem icon assets
- consensus count reads
- icon-aware slot validation
- tracked count updates after successful combines
- guarded resync that resists large OCR regressions
- blocking slots that repeatedly fail verification

A successful later run showed that the bot could:
- progress across multiple gem families
- avoid infinite repeats on previously processed slots
- finish with `All non-perfect gem stacks are now below 10.`

## What Still Looks Weak

Known weak areas after the latest tests:
- skull icon confidence remains poor
- some flawless/perfect result checks are still fragile
- cube occupancy heuristics still sometimes misread `0` or `2` items
- the GUI message `Gem count scan result` does not fully reflect that the read path is now consensus-based
- count OCR precision is still limited by the narrow template bank

## Recommended Next Steps

Priority order for future improvement:

1. Build a better digit sample bank.
   Use real captured crops from many runs and difficult slots.

2. Improve skull icon references.
   Skull assets likely need more careful extraction or a different similarity threshold.

3. Refine result icon verification.
   Compare stash-slot icons and cube-result icons using references captured from both contexts if needed.

4. Improve cube occupancy detection.
   Replace the simple brightness/stddev heuristic if false positives/negatives continue.

5. Align logs with actual behavior.
   Make the manual scan button/log wording explicitly say `consensus` so debugging is clearer.

## Rules To Remember For Future Work

- Never let one noisy reread overwrite reliable tracked counts right after success.
- Use icon validation before trusting numbers.
- Keep per-gem asset quality under review; generic crops are not always enough.
- Treat skulls as a special-problem family until proven otherwise.
- If a slot repeats unexpectedly, inspect the resync logic before touching speed.
- If cube warnings mention `0` or `2` items, inspect timing/state detection before blaming count OCR.
- Recordings are essential. Visual evidence was what exposed the repeat-loop behavior clearly.

## Proven Pain Points

These are the recurring hardships future work should remember immediately:
- stash OCR can be ¡°good enough to plan¡± and still be bad enough to revive a finished slot
- the hardest bugs were caused by state inconsistency, not just recognition quality
- speeding up a stable but naive loop is easy; keeping it correct under noisy reads is the real difficulty
- feature quality improved only after combining multiple defenses: better assets, consensus reads, tracked counts, and guarded resync

## Reference Files

Main implementation:
- `d2bot/gem_summing.py`

Related assets:
- `assets/stash/stash_open_gems_focused.png`
- `assets/items/horadric_cube/`
- `assets/items/gems/icons/`

Asset generation helper:
- `scripts/split_gem_assets.py`
