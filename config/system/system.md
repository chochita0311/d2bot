# system.json

Purpose: app-wide behavior that is not specific to one farming run.
Defaults below come from the config loader when a field is absent; values in JSON override them.

## Top-level fields

- `dry_run` (default `true`): logs clicks instead of sending input through the CLI `BotController`.
- `overlay` (default `true`): shows the OpenCV preview overlay in CLI mode.
- `log_level` (default `INFO`): sets standard CLI logging verbosity.

GUI play actions send input directly and do not use `dry_run` or the CLI pause/stop settings. `Stop Action` stops GUI sessions. `F10` is registered by Gem Summing, Item Looting, and standalone Room Lifecycle; the current Summoner/North Go orchestrator does not register it. Shared dry-run and pause/stop coverage remains development work.

## capture

- `fps` (default `8`): sampling rate for CLI capture and recording. The north-route runtime has separate timing constants.
- `monitor_index` (default `1`): monitor number used when neither a window nor a region is configured.
- `preview_scale` (loader default `0.75`, current JSON `0.7`): CLI preview scale.
- `region` (default `null`): screen rectangle with `left`, `top`, `width`, and `height`; used when no window title is set.
- `window_title` (loader default `null`, current JSON `Diablo`): target title or partial title; takes priority over `region`.
- `window_title_mode` (default `contains`): `contains` or `exact` title matching.
- `follow_window` (default `true`): refreshes the target rectangle if the game window moves.
- `capture_backend` (default `auto`): tries named-window capture and falls back to its screen rectangle. `window` fails if named-window capture fails; `screen` captures the rectangle directly.

## recording

| Field | Default | Scope and behavior |
| --- | --- | --- |
| `enabled` | `false` | Automatic recording in the older CLI flow. |
| `output_path` | `recordings/session.avi` | CLI recording path. |
| `directory` | `recordings` | GUI recordings and north-go attempt recordings/summaries. |
| `codec` | `XVID` | CLI codec; GUI `RecordingSession` currently uses XVID directly. |
| `keep_failed_runs` | `true` | Keeps north-go recordings when status is neither `end` nor `end_latest_frame`, including stopped attempts. |
| `keep_successful_runs` | `false` | When true, keeps every north-go attempt recording. |

North-go retention also keeps the first `north_go_tuning.keep_reference_runs` attempts. Other raw attempt videos are removed; summary JSON files are retained. These flags do not prune ordinary manual GUI recordings.

## logging

Asynchronous file logging currently mirrors Summoner/north-go events. These settings do not replace the CLI logging configuration.

| Field | Default | Behavior |
| --- | --- | --- |
| `directory` | `logs` | Destination for `north-go-tuning.log` and rotated segments. |
| `max_segment_size_mb` | `10` | Rotation threshold in MiB, clamped to at least 1. |
| `retained_segments` | `5` | Number of rotated segments retained in addition to the active log, clamped to at least 1. |
| `flush_interval_seconds` | `0.5` | Periodic flush interval, clamped to at least 0.1 seconds. |
| `queue_max_size` | `5000` | Pending record capacity, clamped to at least 100. When the queue is full, low-priority records can be dropped and priority 0 records are written synchronously instead. |

## gui

- `visible_log_lines` (default `300`): caps the desktop log display, clamped to at least 50 lines. It does not set file-log retention.

## north_go_tuning

- `rerun_count` (default `1`): attempt count when the North Go Test repeat field is blank, clamped to at least 1. An explicit GUI count overrides it.
- `auto_record_runs` (default `true`): records each north-route attempt after room entry and buffs.
- `keep_reference_runs` (default `1`): keeps recordings for the first N attempts per test invocation, clamped to at least 0.

The GUI's blank repeat field has different behavior for Room Lifecycle: it repeats until stopped. Room Lifecycle applies the selected difficulty; current Summoner/North Go room creation still uses its default Hell difficulty.

## hotkeys

- `pause` (default `f8`): toggles pause in the CLI controller.
- `stop` (default `f9`): requests stop in the CLI controller. The preview also accepts Escape.

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
