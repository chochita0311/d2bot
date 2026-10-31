# system.json

Purpose: app-wide behavior that is not specific to one farming run.
Defaults below come from the config loader when a field is absent; values in JSON override them.

## Top-level fields

- `dry_run` (default `true`): logs clicks instead of sending input through the CLI `BotController`.
- `overlay` (default `true`): shows the annotated preview in CLI mode (native Rust preview).
- `log_level` (default `INFO`): sets standard CLI logging verbosity.

GUI play actions send input directly and do not use `dry_run` or the CLI pause/stop settings. `Stop Action` and `F10` stop Summoner, Gem Summing, Item Looting and Room Lifecycle. Summoner's field F8 currently requests stop rather than resumable pause. Shared dry-run and resumable pause coverage remains development work.

## capture

- `fps` (default `8`): sampling rate for CLI capture and recording. Rust continuous Summoner control has a separate CPU capture owner and one waiting frame. It captures on consumer request, permits one initial prefetch, and does no further capture while idle. The60fps cap is an upper bound, not a measured decision rate. Single-shot observations require a capture started after their request. Focused continuous control selects screen capture for a named window with `capture_backend=auto`; explicit window mode, preview and recording retain their configured selection. Recording retains its separate sampling/writer owner.
- `monitor_index` (default `1`): monitor number used when neither a window nor a region is configured.
- `preview_scale` (loader default `0.75`, current JSON `0.7`): CLI preview scale.
- `region` (default `null`): screen rectangle with `left`, `top`, `width`, and `height`; used when no window title is set.
- `window_title` (loader default `null`, current JSON `Diablo`): target title or partial title; takes priority over `region`.
- `window_title_mode` (default `contains`): `contains` or `exact` title matching.
- `follow_window` (default `true`): refreshes the target rectangle if the game window moves.
- `capture_backend` (default `auto`): tries named-window capture and falls back to its screen rectangle. `window` fails if named-window capture fails; `screen` captures the rectangle directly.
- `reference_layout` (loader default `null`, current JSON `assets/ui/layouts/remastered-1922x1140/viewport.json`): named-window analysis uses this reviewed frame/client layout. The shared capture boundary scales game content separately from OS borders/title bar; inputs map reference points back to the physical window. The current layout accepts client aspect error up to1%; larger differences stop before input. Unconfigured captures stay native. GUI recording and snapshot explicitly retain native frames; their role is evidence collection. A smaller window can lose text/texture information even when coordinates map correctly.

`capture.target` and `current_target()` always describe physical desktop bounds. `FramePacket.geometry`/`ScreenCapture.frame_geometry` describe the analysis frame and both coordinate directions. The shared `frame_to_desktop` and `desktop_to_frame` helpers refuse geometry changes after capture. Live actions keep their existing focus/geometry and current-screen deadlines; a changed window invalidates pending input. Focusing restores minimized windows only, preserving split/maximized layouts.

## recording

| Field | Default | Scope and behavior |
| --- | --- | --- |
| `enabled` | `false` | Automatic recording in the older CLI flow. |
| `output_path` | `recordings/session.avi` | CLI recording path. |
| `directory` | `recordings` | Manual GUI recordings. Summoner evidence uses the separate directory described below. |
| `codec` | `XVID` | Legacy configuration field from the former Python implementation. The approved Rust implementation writes direct MPEG-4 q4, silent AVI/OpenDML at nominal8fps in both GUI and CLI; this legacy field does not select another Rust encoder or C codec. |

Current Summoner evidence uses `recordings/summoner/evidence/gui-<timestamp>-<unique-id>/` with per-room reports and event JSONL. The removed North Go runner's retention flags are no longer loaded settings; their former defaults and behavior remain in the [historical run reference](../../docs/features/summoner-run-legacy.md#removed-runner-settings).

Development-session close follows the [development cleanup rule](../../docs/project/developer-guide.md#evidence-handling): promote required recognition assets, summarize results, then remove unnecessary development recordings, logs, frames, and temporary files. Recording settings do not require indefinite storage of development artifacts.

The [real-time/recording PRD](../../docs/plans/prd/prd-0008-realtime-runtime-and-recording-retention.md) proposes shared count/byte/age/free-space limits and isolated writes for long runs. These settings currently do not impose a total quota on town-loop evidence directories. The proposed limits are not implemented configuration fields yet.

## gui

- `visible_log_lines` (default `300`): caps the desktop log display, clamped to at least 50 lines. It does not set file-log retention.

The GUI keeps bounded live text. Summoner's durable reports and event JSONL belong to its evidence directory; generic CLI messages use standard logging controlled by `log_level`. The obsolete asynchronous file logger and `north_go_tuning` settings were removed with their unused runner. They do not configure the current GUI.

Summoner applies the selected difficulty and accepts 1–5 fresh north attempts, blank = 1. Room Lifecycle also applies difficulty, but its blank repeat field repeats until stopped. The optional live town loop remains limited to 1–3. See [Summoner behavior](../../docs/features/summoner-run.md) for preconditions, automatic buffs, conditional ALT and incomplete-result handling.

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
