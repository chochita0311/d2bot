# loot.json

Purpose: shared fixed-item loot that should stay easy to maintain in one place.

## shared_loot

- `description`: free text note for the shared loot set.
- `ignore_labels`: labels you already know should not matter in the basic fixed-item flow.
- `fixed_items`: the main list of deterministic items such as keys and gems.

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
  "label": "amethyst",
  "ground_template": "assets/items/gems/amethyst_on_the_ground.png",
  "threshold": 0.8
}
```

## Override direction

Keep the global default list here.
Later, run-level or character-level overrides can add or remove items without rewriting this base file.
