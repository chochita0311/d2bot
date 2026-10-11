# Config Guide

The loader reads JSON files recursively under `config/`, so each config area can live in its own folder.
Keep the real values in JSON files and use the matching `*.md` guide in each folder for field notes and examples.

| Config file | Field guide and ownership |
| --- | --- |
| `system/system.json` | [System guide](system/system.md): app behavior, capture, CLI hotkeys/logging, recording paths, current Summoner evidence ownership, and `gui` display limits. |
| `loot/loot.json` | [Loot guide](loot/loot.md): shared fixed-item templates and ignore rules. |
| `runs/runs.json` | [Run guide](runs/runs.md): reusable hunting, loot, life, and run-specific rules. |
| `characters/characters.json` | [Character guide](characters/characters.md): metadata, preferred runs, bindings/buff sequences, individual survival policy, and class/current FCR references. |
| `game-rules/casting.json` | [Casting guide](game-rules/casting.md): shared class/skill/form reference thresholds and provenance for input-free estimates. |

`gui` is a top-level JSON section, not a config directory. Fields absent from JSON use loader defaults. The removed North Go tuning, recording-retention flags and asynchronous file-logger fields are described only in the [historical run reference](../docs/features/summoner-run-legacy.md#removed-runner-settings).

Files are loaded in sorted path order and dictionaries merge recursively. Later values replace earlier scalar/list values, so avoid defining the same setting in multiple files.
