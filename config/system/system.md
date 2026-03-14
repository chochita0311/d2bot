# system.json

Purpose: app-wide behavior that is not specific to one farming run.

## Top-level fields

- `dry_run`: `true` means log actions without performing live clicks in controller-driven flows.
- `overlay`: show the OpenCV preview overlay in CLI mode.
- `log_level`: logging verbosity such as `INFO` or `DEBUG`.

## capture

- `fps`: how often the screen is sampled.
- `monitor_index`: monitor number for screen capture fallback.
- `preview_scale`: preview window scale for CLI overlay.
- `region`: fixed capture box. Use `null` to rely on window tracking.
- `window_title`: the target game window title or partial title.
- `window_title_mode`: `contains` or `exact`.
- `follow_window`: keep capture aligned if the game window moves.
- `capture_backend`: `auto`, `window`, or `screen`.

## recording

- `enabled`: whether the older CLI flow records automatically.
- `output_path`: default recording path.
- `codec`: video codec such as `XVID`.

## hotkeys

- `pause`: hotkey for pause in controller-driven flows.
- `stop`: hotkey for stop in controller-driven flows.

## Example

```json
{
  "dry_run": true,
  "capture": {
    "window_title": "Diablo II: Resurrected",
    "window_title_mode": "contains",
    "capture_backend": "auto"
  }
}
```
