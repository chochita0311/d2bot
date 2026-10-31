# Rust Runtime

One root Cargo package builds `d2bot.exe`; `src/main.rs` only adapts process arguments/output/exit status. `src/app/` owns argument parsing and routing, `src/ui/` the native control panel/preview, `src/core/` the legacy template loop, `src/tools/` developer commands, and the existing domain modules own actions/policies/vision. Windows FFI stays under `src/platform/windows/`. The final distribution gate is [FEAT-0028](../plans/feature/feat-0028-rust-windows-distribution.md); source/fake/stored checks do not establish full-game success.

## Invocation

Development runs resolve runtime assets and default config from the working directory. A packaged EXE with its sibling `package-manifest.json` resolves assets and default config beside the EXE even when invoked from another directory. Keep the package together; explicit `--config`, tool input/output and relative recording paths retain the caller's working-directory meaning. `--config` accepts a JSON file or directory. From a development checkout:

```powershell
cargo build --release --locked
.\target\release\d2bot.exe
.\target\release\d2bot.exe --cli --config config
.\target\release\d2bot.exe --list-windows
.\target\release\d2bot.exe --help
.\target\release\d2bot.exe --diagnose
```

No arguments open the control panel. `--cli` runs ordered template matching, configured dry-run/live actions and the optional annotated preview; Escape/close requests cancellation and waits for worker cleanup. Its configured pause/stop chords remain unchanged (defaults F8/F9). GUI actions retain their live-input behavior and F10 stop, independently of the general CLI's `dry_run`. Summoner retains its own original stop/pause meanings and defaults to dry on the CLI; `--live` enables that route's game input. Hotkeys still reach the game, and the source extra-held-key limitation remains as requested.

Main routing precedence remains visible-window listing, town/Arcane, Summoner, GUI, then `--cli`. The original flags, defaults, choice/exclusion rules, last-value wins and unambiguous long-option abbreviations are retained. `--help`, usage errors, version and diagnose initialize no game capture/input. `--list-windows` reads window metadata without loading config. Exit0 means successful command completion within its stated scope; usage errors return2, unexpected runtime/setup/I/O failures1, and bounded action/probe failure or interruption follows the source's return2. General CLI console interruption returns130 after orderly cleanup; its normal configured Stop/Escape returns0. A dry result supplies no live success evidence.

## Developer Command Mapping

Each command's `--help` lists its inner arguments. Former Python module names below identify migration history only. The executable commands run Rust implementations; no Python source or interpreter is required.

| Former Python module | Rust command | Effect boundary |
| --- | --- | --- |
| `diablo2.tools.survival_observe` | `d2bot survival-observe` | Image/live-screen HUD and optional policy observation; no input |
| `diablo2.tools.field_replay` | `d2bot field-replay` | Bounded synthetic scenario only; no capture/input |
| `diablo2.tools.arcane_navigation_replay` | `d2bot arcane-navigation-replay` | Stored frames/calibration/optional overlay PNGs; no input |
| `diablo2.tools.field_input_probe` | `d2bot field-input-probe` | Explicit bounded supervised probe; `--live` required for input, with the retained fresh permit/proof conditions |
| `diablo2.actions.town_loop_vision` | `d2bot town-loop-vision` | Capture/calibration asset writer or one persisted planner step; no input |

Probe stdin has one explicit reader owner, a4096-character line bound and cancellation/join handling. Buff dispatch acknowledgement remains separate from supervised effect/equipment proof. The step and one-renewal options retain their source eligibility, proof deadlines and no-retry semantics. A later failed proof preserves earlier dispatch data in the report. Input-free replay cannot supply such proof.

## Recording, Configuration And Shutdown

GUI manual recording uses unique timestamp files and refuses collisions. General CLI automatic recording retains `recording.enabled` and the fixed `recording.output_path`, replacing that explicitly configured file as the source did. Both use the approved direct Rust MPEG-4 implementation, q4, silent AVI/OpenDML and fixed8fps; `recording.codec` remains a legacy config field and does not select a C/Xvid/FFmpeg runtime. The CLI writes full-size annotated frames when overlay is enabled; preview scaling never changes recorded resolution. Current codec scope/quality/size/time limits are in [EVAL-0028](../plans/evaluation/eval-0028-rust-mpeg4-recording.md).

Invalid optional buff-gap overrides are removed in memory, with their existing missing-key default retained; the input files are unchanged. This covers NaN, infinity, negatives, conversion overflow and finite numbers outside `Duration`'s representable range. Summoner uses0.4 seconds for a missing token. The runtime pause schedule also validates conversion before waiting. The GUI log/main CLI show the path, original value and repair action. Developer commands send these warnings to stderr so JSON stdout remains parseable. Invalid required safety/casting policy still fails explicitly.

Capture, hotkey registrations, input state, video finalization and workers have explicit owners. Ctrl+C/Ctrl+Break for owned running sessions request cancellation; the native callback does no I/O/locking/join. Preview close and broken output propagate cancellation independently of vision/encoding. Cleanup errors remain errors, and timeout alone does not establish worker completion. Normal Windows process termination is distinct from this orderly cancellation path.

Rust uses ownership/borrowing and deterministic `Drop`, without a tracing garbage collector. Immutable image buffers are owned and bounded; shared `Arc` references do not form a garbage collector. Fallible OS/file/codec work uses explicit `Result`/close paths; `Drop` is fallback cleanup. Safe Rust helps memory safety, while timing, deadlocks, native API results and behavior still require the recorded tests and actual runtime checks.

Active observations/sessions share at most eight Rust compute workers, capped by available parallelism. With eight available, six serve foreground work and two serve broad background searches; smaller machines divide the budget and a one-core machine shares one pool. Nested jobs stay in their entering pool, and scoped borrows end before frame reuse. The registry holds only a weak reference, and the final family owner shuts down and joins both pools. Recording reuses owned YUV/reconstruction buffers and keeps exact repeated blocks through standard MPEG-4 skip coding. GOP,8fps timing, native dimensions and quality gates remain unchanged. [The CPU control evaluation](../plans/evaluation/eval-0033-cpu-realtime-control.md) owns current timings and limits; [the earlier efficiency evaluation](../plans/evaluation/eval-0032-rust-runtime-efficiency.md) retains its baseline.

Continuous Summoner capture has one producer and one waiting frame. Apart from its initial frame, capture requires consumer demand, so idle consumers do not generate 60fps work. Focused named `auto` control capture selects screen mode and records the actual backend; explicit window mode and other consumers retain their selection. Each GDI call owns an initialized top-down DIB, flushes drawing before reading, copies into an owned BGR frame and closes its bitmap/DCs before returning. No raw pixels or GDI handles cross that call's thread boundary.

Current-frame panel preparation overlaps HUD work and joins before camera authorization. Broad terminal/objective hints retain their source and require current confirmation; pending or stale negatives never authorize movement. Continuous aim has no additional fixed50ms settling delay: pointer confirmation, focus, cancellation, generation and expiry checks still apply. Eligibility reads the clock again after pointer confirmation. Discrete approaches and configured positive settling/repeat timing keep their own waits. These changes do not establish a50ms end-to-end bound.

Default packages retain non-reference shared assets, the two town reference images actually used at runtime, config-selected shared assets and all required notices. Development screenshots remain in the source tree; `-IncludeDevelopmentAssets` includes those references when needed. Register new loaders and required references as described in [the asset policy](../../assets/README.md).

## Reproducible Development Measurements

`examples/runtime-benchmark.rs` measures decoded1922×1140 PNGs,30 samples after3 warmups, with nearest-rank p50/p95. The Python benchmark was retired after its recorded comparisons; historical results remain in EVAL-0032/0033. Supply a current full-window PNG and a new output directory. The two checked-in north-step frames provide path/ORB inputs. The current PNG provides grayscale, preview, HUD/panel and static encoding inputs; compare input/pixel/grid hashes and decisions before interpreting times. `stored_packet` combines normalization/gray, HUD, path and a two-frame ORB pair; it is a development workload, not the real loop's frame rate or capture-to-game acknowledgement latency.

```powershell
cargo run --release --locked --example runtime-benchmark -- CURRENT.png target/tmp/rust-measurement
```

The optional `--capture` reads the currently visible Diablo window using screen mode; the Rust helper additionally records8 seconds and measures actual cancellation/finalization/join. It sends no game inputs. Live capture timings use the same target/backend at different moments and do not claim pixel-identical animated frames. Benchmark output is task-owned diagnostic media/JSON; summarize results in the active evaluation and remove it after diagnosis. Regression tests read frozen legacy observations using Rust; test fixtures are excluded from distribution.

The Rust helper also times the current camera/ground, nearby hazards, terminal/objective, native hub and legacy town-hub observers separately. The Rust helper includes extra HSV/label-mask/town-normalization/journal-map diagnostic stages; those are not silently paired with a different Python workload. Cold panel samples use a fresh immutable frame allocation, while repeated camera calls use the same frame and its validated panel memo. Neither measurement stands in for actual concurrent navigation timing.

The Rust helper's explicit `--record-during` option starts the actual Windows recorder in the same process/shared compute pool during the fixed-image vision stages, then stops and joins it before the standalone static encoding stage. This checks recording contention without game input; keep the game visible. Compare it to a separate run without the flag. The recorded live scene differs from the fixed vision inputs, so a character-selection recording does not certify moving-combat overhead. `--capture` remains the separate capture/8-second recording measurement. The helper writes only to the requested new diagnostic directory.

`--control-packet` compares serial and overlapping panel preparation in the actual native near/hover pipeline, with and without broad searches. Timing starts after immutable frame allocation and excludes capture and input. It checks final consumer-frame release. Source-to-input game timings require the explicit observation metadata in supervised reports; neither this benchmark nor stage-percentile sums replace those timings.

Some GPU/game combinations produce a black game body through auto/window capture while window chrome is still visible. Choose the existing `screen` option and keep the game visible. The retained backend contract does not infer a fallback from black content or change gameplay thresholds.
