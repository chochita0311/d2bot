# Rogue Encampment appearances

`templates/` and `references/` own the generic Act 1 area-title, waypoint, tent and town-layout appearances formerly mixed with personal town-loop captures. Full sources contain game UI, NPC labels and system messages without player/account identifiers. Image bytes are unchanged.

The [shared town manifest](../../../ui/town-loop/remastered-ko-window/manifest.json) owns normalized geometry, thresholds, click offsets and provenance. These samples support the bounded town observer, not arbitrary map classification or successful arrival on their own.

`layout-tent-automap.png` adds the same spawn flag under the current automap at the unchanged .82 threshold. Its flag-only reference and source/crop hashes live in that manifest. The prior flag scored .832768 before Tab and .747329 after Tab in the reviewed 2026-10-20 room, causing the longer fallback probe route. This variant retains current layout classification; it does not reveal an unseen waypoint or bypass camera/current-town checks.

`references/maintenance/stash-body.png` and `stash-hover-ko.png` are distinct uncalibrated stash body/hover catalog examples for deferred maintenance work. They have no current detector or action consumer. Their old whole-area source was removed; crop dimensions do not establish the original viewport geometry or current matching compatibility. Waypoint-list UI crops live under `assets/ui/waypoint/`.
