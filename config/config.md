# Config Guide

The loader reads JSON files recursively under `config/`, so each config area can live in its own folder.
Keep the real values in JSON files and use the matching `*.md` guide in each folder for field notes and examples.

Folders:

- `config/system/`: capture, recording, hotkeys, and app-level behavior
- `config/loot/`: shared fixed-item loot that all runs can inherit
- `config/runs/`: reusable run definitions for hunting, loot, life, and rules
- `config/characters/`: character metadata and preferred run profile mapping

Files:

- `config/system/system.json`
- `config/loot/loot.json`
- `config/runs/runs.json`
- `config/characters/characters.json`
