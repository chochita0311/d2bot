# Arcane North Observation Assets

[manifest.json](manifest.json) is the region owner consumed by the input-free [observer](../../../../diablo2/common/encounter_vision.py) and [replay checks](../../../../tests/test_encounters.py). Character names, keys, FCR, potion policy and combat radius belong to personal configuration/caller policy. Existing private room assets and legacy monster files were preserved.

The 2026-10-05 supervised captures supplied two north appearances: a stair bridge and a flat bridge. These are human layout labels, not an automatic map classifier or an exhaustive list. World crops use native 1922×1140 coordinates with viewport `[120,180,1720,890]`; the observer translates crop-local matches back to native coordinates. Name/time/room overlays and HUD are excluded. Three separate name crops support the reviewed **구울 군주**, **지옥혈족원**, and **화염 저항의 신단** labels.

The five templates retain alive Ghoul Lord/Hell Clan appearances, a Ghoul Lord corpse, a Hell Clan corpse group and a fire-resistance shrine. The shrine is a negative monster example: its tower shape was initially mistaken for an enemy. A corpse group has no per-instance ground point or kill count. Template matches are appearance candidates, not verified life states or identity links.

Eight scene crops are bounded runtime/replay references. Source sequence, elapsed recording time, original JPEG hash and crop box are in the manifest. JPEG quality 88 was used for acquisition; saving the crops as PNG does not restore lost detail. Original development recordings were removed after promotion, and no consumer needs their paths.

The `.90` template threshold is a same-scene calibration limit, not a probability or measured independent-room accuracy. The alive source frame matches; the immediately following pose does **not**. The corpse-group follow-up matches at about `.985`. Keep this miss as unknown rather than death, and collect more poses/lighting/effect variants before live combat integration. Multiple matching boxes are candidates, not counts of distinct monsters.

The floor BGR palette reuses the existing north-route reference without importing its input executor. The gray `(125,126,126)` void color was omitted because of floor overlap. Lab distances remain explicit manifest values. Gray walls, pillars, statues and elevated surfaces can share floor colors; `floor-candidate` never proves teleport reachability. Actual test patches distinguish one gray floor, dark void and obscured character area. Map connectivity, landing success, camera motion and other layouts remain unverified.

Short names identify kind/state inside a contextual directory; `template` is unnecessary here. This naming choice applies to these new assets only. [Execution evidence and limits](../../../../docs/plans/run/run-20261005-05-arcane-north-encounters.md) own the actual session results.
