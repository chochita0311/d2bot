# loot.json

Purpose: shared fixed-item loot that should stay easy to maintain in one place.

## shared_loot

- `description`: free text note for the shared loot set.
- `ignore_labels`: labels you already know should not matter in the basic fixed-item flow.
- `fixed_items`: the main list of deterministic items such as keys and gems.

The owner requested runes at or above Ist and no gems on 2026-10-30. The current shared list enables Key of Hate and the two available named rune crops, Ist and Ohm, at a conservative 0.95 rune threshold. Generic rune and gem candidates are removed. Other qualifying runes remain unsupported until their names can be recognized and validated; an unknown rune is skipped. This is a limited named allowlist, not complete Ist-or-higher recognition.

The Summoner run consumes this list; its loot flow is not restricted to the boss key. It inspects after actual Nova activity has quieted, and on the encounter path after Nova dispatch or confirmed death. See the [Summoner behavior](../../docs/features/summoner-run.md).

Ground recognition matches the configured image crops; it has no general rune-name OCR or gem-grade reader. The full Ist/Ohm crops contain the name and the rune suffix; the old generic crop contains only the suffix. A fresh candidate can lead to approach and click, but label disappearance or a click alone is not confirmed acquisition. The current flow does not use `inventory_template` to verify collection. Template tests do not establish discrimination against every unseen lower-rune label.

## fixed_items entries

Each entry can include:

- `label`: human-readable item name used in logs.
- `ground_template`: image path used to detect the item on the ground.
- `inventory_template`: optional image path for later inventory verification.
- `threshold`: match confidence threshold for template matching.

## How to add an item

Add one new object under `shared_loot.fixed_items`.

Example:

```json
{
  "label": "ist rune",
  "ground_template": "assets/items/runes/ist-on-the-ground.png",
  "threshold": 0.95
}
```

## Override direction

Keep the global default list here.
Later, run-level or character-level overrides can add or remove items without rewriting this base file.
