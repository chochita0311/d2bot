# Diablo2

Windows-first helper project for observing a Diablo II game window and driving vision-based gameplay flows. Recording supplies evidence for validation and improvement. The desktop GUI and older CLI preview loop remain available.

## Rust Windows package

The migration is tracked by [PRD-0009](docs/plans/prd/prd-0009-rust-windows-port.md) and [the port plan](docs/plans/refactoring/refactor-0002-rust-port.md). Rust implements the GUI, action/Summoner runtimes, template preview, five developer commands and direct MPEG-4 AVI recording. The Windows package is under `dist/d2bot-windows-x64-rust-only/`: 660 files, about24.1MB including4.99MB shared assets. It runs without Python, OpenCV, Cargo, FFmpeg or the Xvid C library. The Python application, tools and test runners have been retired; historical source remains at the pinned Git baseline. Rust tests consume frozen legacy observations without running Python. [Session closure and verification](docs/plans/run/run-20261031-01-rust-source-retirement.md) records coverage and remaining limits.

CPU control uses request-driven capture, shared ROI work, bounded foreground/background compute, overlapping current preparation and pointer-confirmed continuous aim without an additional50ms timer. Current confirmations run in the foreground. A tested ground/landing split added no demonstrated benefit and was removed. The earlier CPU checkpoint G passed331 release tests plus4 explicit native checks and measured source-to-near p50/p95 of58.280/72.101ms and new teleport keydown138.095/172.567ms with recording. The50ms response goal remains unmet; some stages remain slower than native OpenCV. The current Rust-only static-CRT release packet passes370 tests with6 opt-in checks ignored (4 native,2 performance); formatting, all-target checking and warnings-denied Clippy pass. The session closure linked above records the exact scope and artifact. Current expiry guards still refuse stale input. Central reacquisition, complete fresh-room/boss/key acceptance and the earlier Canyon-finish failure remain unresolved.

[The CPU control evaluation](docs/plans/evaluation/eval-0033-cpu-realtime-control.md) owns the current source/build, verification, timings, memory and limits. [The earlier efficiency evaluation](docs/plans/evaluation/eval-0032-rust-runtime-efficiency.md) retains its baseline. [The distribution evaluation](docs/plans/evaluation/eval-0031-rust-windows-distribution.md) retains its historical appearance/transition review. The owner authorized the Rust-only default and Python source retirement on2026-10-31; broader environment/appearance and whole-game acceptance remain limited by the recorded evidence. Test recovery has been verified through existing north return→central waypoint→Act1→save/exit code; it does not use a town-portal scroll or count as a fresh-run success.

Windows Rust development uses the official rustup MSVC toolchain plus Microsoft C++ Build Tools and Windows SDK. Once the pinned toolchain is installed, run from this repository:

```powershell
cargo build --release --locked
.\target\release\d2bot.exe --diagnose
.\target\release\d2bot.exe --help
cargo test --release --all-targets --locked
```

`Cargo.toml` owns Rust dependencies, `Cargo.lock` records their resolved versions, and `rust-toolchain.toml` pins the validated compiler. `config/` and `assets/` retain their established paths. [Rust runtime commands](docs/features/rust-runtime.md) describe invocation, module mapping, ownership and shutdown. Build a portable static-CRT folder with `.\scripts\package-windows.ps1`; it refuses to overwrite an existing package. Keep the existing package separately or choose a new `-OutputDirectory` for another build.

Project constraints and safety boundaries are documented in `docs/project/policy.md`.
Architecture notes are documented in `docs/project/architecture.md`.
Roadmap and open planning items are documented in `docs/project/roadmap.md`.
Game mode notes are documented in `docs/features/game-modes.md`.
Farm profile structure is documented in `docs/features/farm-profiles.md`.
Run coordinator behavior is documented in `docs/features/run-coordination.md`.

## What it does now

- Launches the Rust desktop control panel by default
- Records a named Diablo window, captures snapshots, and selects a capture backend
- Offers Gem Summing, Item Looting, Room Lifecycle, and the current Summoner GUI development run
- Uses character configs for movement keys and pre-run buff sequences
- Supports the retained template/overlay preview through `--cli`, implemented in Rust
- Loads JSON configs for capture, recording, character actions, and reusable run rules

The Summoner GUI connects room entry, automatic buffs, current-screen north navigation, bounded nearby Nova, and conditional loot scans. Empty north endpoints return through the central waypoint; a confirmed Summoner corpse and completed loot inspection lead through the journal and Canyon of the Magi. Both routes finish in Act 1 before exiting. The current development goal is 10 consecutive fresh north runs; that goal, confirmed key acquisition, four-wing completion, and live acceptance remain unfinished. See [Summoner Run](docs/features/summoner-run.md) for behavior and limits.

## Quick start

Keep `d2bot.exe`, `config/`, `assets/` and the license files together. On Windows with a working OpenGL graphics driver, open `d2bot.exe` or start it from the package directory:

```powershell
Set-Location .\dist\d2bot-windows-x64-rust-only
.\d2bot.exe
```

With its sibling `package-manifest.json`, the executable finds default config and assets beside itself even when launched from another folder. Explicit config/input/output paths and relative recording paths retain the caller's working-directory meaning. Rust/MSVC tools are needed to build from source, not to run this package. In the GUI:

- select the Diablo window from the list
- choose `auto`, `window`, or `screen` capture
- if the game body is black in `auto`/`window`, choose `screen` and keep the game visible
- click `Start Recording` to begin saving video
- click `Stop Recording` to finish
- click `Capture Snapshot` for a still image

## Play controls

Select the matching `Character` config before starting an action. This selects skill settings and the preferred run profile; select the actual character in the game separately.

| Control | Starting point and current scope |
| --- | --- |
| `Start Gem Summing` | Open the supported gem stash and Horadric Cube first; see [Gem Summing](docs/features/gem-summing.md) for count and verification behavior. |
| `Summoner Run` | Start at character select on the main weapon set. Uses the selected character/difficulty; `Repeat Count` is 1–5, blank = 1. Enters Arcane Sanctuary, buffs automatically, and attempts north followed by the appropriate waypoint/journal return to Act 1 and exit. An unconfirmed encounter or unfinished route retains the room for review. |
| `Item Looting` | Matches configured ground-item templates and clicks visible labels. |
| `Start Room Lifecycle` | Creates and exits rooms from character select using `Difficulty`. A blank `Repeat Count` repeats until stopped. |
| `Live town loop (1–3 runs)` | Enable before `Start Room Lifecycle` to create a room, visit Act 2 town, return to Act 1, and exit; requires 1–3 repeats. See [town loop](docs/features/town-room-loop.md). |

Play controls send live mouse and keyboard input independently of the CLI `dry_run` setting. Use `Stop Action` or `F10` to stop. Summoner uses the latest main runtime, with no supervisor JSON or inventory/weapon-tab inspection. Its buff sequence returns to the starting main weapon set; Flash renews from the actual S request +140 seconds when current nearby-enemy and resource-ratio checks permit it. Labels stay OFF while moving and turn ON for bounded post-combat loot scans, then OFF again. The reviewed Summoner layout is Korean remastered 1922×1140 window capture with ALT configured as a show/hide toggle.

The removed `North Go Test` and old route implementation are no longer separate execution paths. Current-platform corpse confirmation has passed in same-room recovery; a complete fresh Summoner kill/key run, 10 consecutive fresh north completions, and the 27/30 acceptance test remain unfinished. Recovery and human intervention do not count as fresh successes. Automatic renewal and conditional ALT have bounded live evidence rather than full-run acceptance. See [current execution and next steps](docs/features/summoner-run.md).

## Other commands

List visible windows:

```powershell
.\d2bot.exe --list-windows
```

Run the retained CLI preview loop from the package directory (Escape closes it):

```powershell
.\d2bot.exe --cli --config config
```

## How to steer behavior

Edit files under `config/`. The [config guide](config/config.md) links to the field reference for each area: app/capture settings, shared loot, run rules, and character actions.

For the CLI, `dry_run` defaults to `true`, with `F8` pause and `F9` stop configured in the [system settings](config/system/system.md). Configured hunting, loot, and life rules describe intended behavior; their presence does not mean every rule has an executing engine yet.

## Current seeded run profiles

- `summoner`: Arcane Sanctuary (`비전의 성역`) run for The Summoner (`소환술사`) and `key of hate`

## For contributors

Contributor and coding-agent instructions live in `AGENTS.md`.
The imported agent workflow and its local adoption notes are indexed in [docs/agents/README.md](docs/agents/README.md).
