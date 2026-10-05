# Shared Survival HUD Assets

[manifest.json](manifest.json) owns the reviewed `remastered-ko-window-1922x1140` layout and matching limits used by the [observer](../../../diablo2/common/survival_vision.py). These shared assets contain HUD labels, digit/slash masks, belt edges and item appearances. Character names, bindings, FCR, thresholds and belt policy belong to personal configuration. Existing private town-loop assets have not been moved or renamed.

## Provenance

The `2026-10-05` Act 1 town capture supplied labels, the 985/985 and 1075/1075 digits, belt row appearances and one empty upper slot. Existing [public HUD reference](../../character/play/status.png) supplied 3/4 glyphs. The same session's character panel supplied 2/6 masks; their use in other HUD values still needs actual screen validation. Glyph extraction retains only the numeral masks.

Revision `screen-reference-2026-10-05-v2` adds five HUD crops from one supervised Arcane central-waypoint buff sequence: before, weapon swap, before Orders, after Orders, and restored battle equipment. Life maxima were 985, 895, 895, 1490, and 1580; mana maxima were 1075 before Orders and 1543 afterward. These samples support the observed numbers and label shifts, not autonomous skill or field-safety recognition.

`references/` contains those five field crops plus the four town crops: expanded, eleven potions, closed, and restored. Each image is 1400×247 and is embedded at `canvas_box` in a blank 1922×1140 canvas by replay tests. Counts and resource pairs are the human-reviewed expected values in the manifest. Samples from the same scene are follow-up observations, not independent rooms. Closed field crops have unknown hidden-row counts.

The observer searches for a reviewed label within its bounded region before reading adjacent digits. The buffed mana-label variant handles the captured rendering difference without lowering the label threshold. `belt-closed-edge.png` supplies positive closed-state evidence; failure to match an expanded belt alone does not prove closure.

Full new screenshots, OCR probes and intermediate crops were task-owned diagnostics and were removed after extracting these durable references. No runtime or test depends on those temporary files.

Revision `screen-reference-2026-10-05-v3` adds `references/town-mana-obscured.png`, a 1400×247 actual town HUD crop at `[250,880,1650,1127]`. Background light overlaps the numeric separator: human review shows 1075/1075, but CV returns `glyph_ambiguous`. `negative_references` owns its checksum and expected rejection; it is a regression case, not a glyph training template. Digit thresholds have not been lowered to force a read.

Revision `screen-reference-2026-10-05-v4` adds two program-captured HUD crops from bounded held-input tests in [RUN-20261005-06](../../../docs/plans/run/run-20261005-06-field-control-loop.md). `field-nova-mana.png` reads 1580/1580 life and 1457/1543 mana after repeated Nova. `field-mana-obscured.png` shows human-reviewed 1530/1543 mana but remains `glyph_ambiguous`; the travel probe released its key on that uncertainty. Both use the same crop geometry, closed belt and manifest checksums, contain no character/room name, and provide numeric acceptance/rejection cases without changing thresholds. Hidden potion rows remain unknown.

`references/belt-after-north-11.png` adds the actual expanded belt after one supervised north-encounter potion use: columns 1–4 contain 3/4/4/4 items. It is a 270×235 native crop at `[1070,885,1340,1120]`, without HUD numbers. `belt_only_references` in the manifest owns this geometry and counts. The encounter replay test composes it with the earlier town HUD; this is a synthetic composition of two actual captures, not simultaneous combat HUD/belt evidence. The belt was subsequently closed before leaving the character in town.

## Calibration Limits

- Label/glyph scores are mask similarities; item/edge distances are normalized color differences. They are calibration limits, not probabilities or measured safety guarantees.
- Purple samples show the large full-rejuvenation item. Small rejuvenation potions and other graphics are unsupported. Upper/middle/base references preserve actual row appearance differences.
- Only one real upper empty slot was captured. Other empty positions, combat overlays, poison, resource values beyond the collected samples, other layouts/languages and independent field accuracy require further data.
- Keep coordinates, template shapes and revision together when adding a reviewed layout. Do not resize unknown layouts to fit or lower limits to force ambiguous reads. Unknown slots/numbers remain unknown.

See [usage and evidence limits](../../../docs/features/survival-observation.md), the [town record](../../../docs/plans/run/run-20261005-03-survival-screen-observation.md), and the [field record](../../../docs/plans/run/run-20261005-04-field-buff-confirmation.md).
