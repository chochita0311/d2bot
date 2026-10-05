# Shared Weapon-Set Tabs

[manifest.json](manifest.json) owns the reviewed 1922×1140 inventory-tab regions and matching limits used by [WeaponSetObserver](../../../diablo2/common/buffing_vision.py). `active-1.png` and `active-2.png` concatenate the two generic weapon-tab strips. They contain no character name or equipment image.

The source was a supervised Act 1 inventory check on `2026-10-05`, after the Arcane buff test. Set I was captured, switched to II, then restored to I; the inventory was closed afterward. These are one session's calibration samples. Closed inventory, occlusion, an unknown layout or ambiguous matching returns unknown.

The observer identifies the visible active I/II tab only. It does not identify a build, verify gear or FCR, or choose a character's expected battle set. That expectation and bindings belong to the individual profile; the consumer must also verify current context and inventory closure before movement. Matching distances are calibration limits rather than probabilities.

See [buff confirmation](../../../docs/features/field-buffs.md) and the [execution record](../../../docs/plans/run/run-20261005-04-field-buff-confirmation.md). Raw equipment screenshots and extraction diagnostics are not runtime dependencies.
