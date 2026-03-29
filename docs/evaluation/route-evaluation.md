# Evaluation Standard

Use these checks when revising route logic, real-time steering, or fast/slow vision behavior so recordings can be compared consistently across the project.

## Primary Success Checks

- the run should reach its intended goal without user interruption
- the route should not freeze on one repeated family/candidate pair for a long stretch
- the cursor should not keep aiming into obvious void or space when the usable walkway is visible

## Log Signals To Watch

- `family`, `choice`, `base_ratio`, `final_ratio`
- `north_open` or the route-specific gate signal
- `frame_change`
- `side_stuck_steps` or the route-specific stuck counter
- `fast_age`
- `fast_gap`
- `fast_proc`

## What Usually Counts As Improvement

- `fast_proc` trends down instead of staying in an older higher range
- `fast_gap` is more often `2-3` than `4-6`
- `fast_age` trends down and spends less time around `200ms+`
- the route does not keep steering to endpoints that are visibly in space
- path-aware steering lands on a meaningful mid/far ray area rather than hugging a too-close sample near the character
- escape logic breaks repeated east/west churn instead of looping indefinitely

## What Usually Counts As Regression

- the first fork does not happen after the initial stale-fast hold
- the route gets stuck repeating the same `family`, `choice`, and nearly identical `final_ratio`
- gate logic keeps one family locked even though the chosen endpoint is obviously bad
- steering becomes too conservative and only points close to the character
- steering becomes too aggressive and keeps aiming at empty-space endpoints
- `fast_proc`, `fast_gap`, or `fast_age` get worse without a clear behavioral benefit

## Comparing Recordings

- judge behavior first, timing second
- if one version is slightly slower in logs but reaches the goal more reliably, that can still be a better build
- if timing improves but steering quality gets worse, do not count it as a win
- prefer changes that improve both path quality and consistency before chasing more raw speed
