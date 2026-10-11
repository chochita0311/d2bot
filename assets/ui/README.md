# Shared UI appearances

| Owner | Purpose and boundary |
| --- | --- |
| `town-loop/remastered-ko-window/` | Current Play, difficulty, HUD and exit calibration; its manifest references region world appearances and other UI owners. |
| `waypoint/` | Reviewed panel header and act-list crops consumed by the shared town manifest. Full town scene sources remain with their region. |
| `world-labels/` | Generic shrine and NPC-cancel labels. Matching text does not establish a region, object identity or safe interaction. |
| `stash/` | Generic stash header above personal tabs/content; gem observation retains full-panel coordinates and selects the gem tab separately. |
| `horadric-cube/` | [Cube panel/button appearances](horadric-cube/README.md); the actual item icon belongs under `items/horadric-cube/`. |
| `character-select/` | Uncalibrated ladder/ruleset marker catalog and a combined example. These do not implement character-row classification or select an identity. |
| `survival/`, `weapon-set/`, `automap/` | Current recognition templates and bounded replay evidence; each existing manifest/guide owns geometry, thresholds and limits. |
| `gem-count-digits/`, `layouts/` | Exact shared digit atlas and reviewed camera layout, with their existing provenance and current-observation requirements. |

Keep reusable UI with its widget/state owner. Actual names, personal tabs, inventory contents and account selections remain ignored local evidence. Use existing manifest file paths when recalibrating, preserve meaningful state variants, and keep crops/replay sources distinct from uncalibrated catalog examples.
