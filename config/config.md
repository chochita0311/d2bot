# Config Guide

The loader reads JSON files recursively under `config/`, so each config area can live in its own folder.
Keep the real values in JSON files and use the matching `*.md` guide in each folder for field notes and examples.

| Config file | Field guide and ownership |
| --- | --- |
| `system/system.json` | [System guide](system/system.md): app behavior, capture, hotkeys, recording paths/retention, asynchronous `logging` directory/rotation/flush/queue settings, `gui` display limits, and `north_go_tuning` rerun/reference-recording defaults. |
| `loot/loot.json` | [Loot guide](loot/loot.md): shared fixed-item templates and ignore rules. |
| `runs/runs.json` | [Run guide](runs/runs.md): reusable hunting, loot, life, and run-specific rules. |
| `characters/characters.json` | [Character guide](characters/characters.md): character metadata, preferred run profiles, movement bindings, and buff sequences. |

`logging`, `gui`, and `north_go_tuning` are top-level JSON sections, not config directories. Fields absent from JSON use loader defaults; the system guide records the added recording and tuning fields even when they are omitted from the checked-in file.

Files are loaded in sorted path order and dictionaries merge recursively. Later values replace earlier scalar/list values, so avoid defining the same setting in multiple files.
