# Architecture

This document organizes the high-level product direction and system structure for the Diablo 2 Windows GUI automation project.

## Current implementation

| Area | Responsibility |
| --- | --- |
| `main.py`, `diablo2/app.py` | CLI arguments and dispatch to the desktop GUI or CLI preview. |
| `diablo2/ui/gui.py` | Tkinter capture/recording controls, character-config selection, and action launch/stop controls. |
| `diablo2/actions/` | Recording, gem combining, ground-template loot pickup, and room create/exit sessions. |
| `diablo2/runs/` | Run definitions and the staged Summoner orchestrator; route pieces and runtime helpers live under `runs/summoner/`. |
| `diablo2/common/` | Config loading, Windows capture, template detection, movement input, real-time vision workers, CLI controller, and asynchronous logs. |
| `diablo2/core/bot.py` | Older CLI capture/template/overlay loop. |
| `config/`, `assets/` | Runtime settings and profiles, plus screenshot-derived matching assets. |

The current Summoner stages are documented in [summoner-run.md](../features/summoner-run.md). The north route uses capture, fast/slow vision, and decision workers; detection can delay route decisions, but a complete survival/combat/loot coordinator is still a target.

[Character survival state](../../diablo2/common/survival.py) now provides character-owned policy validation, confirmed per-room buff/belt state, and input-free recovery/exit/replenishment requests. It has no capture or input dependencies and can be consumed by other runs. [Character configuration](../../config/characters/characters.md#survival) owns the fields; [FEAT-0003](../plans/feature/feat-0003-character-survival-state.md) owns the tested boundary. Input coordination, cooldowns and action-result confirmation remain separate integration work.

[Survival HUD observation](../features/survival-observation.md) adds a separate CV producer and bounded input-free CLI. Shared layout/glyph/item assets read current/max resource pairs and expanded belt slots for one reviewed layout. Ratios never reuse pre-buff maxima; unknown readings remain unknown. The producer supplies no field, clear-surroundings or buff-effect confirmation and is not wired into the staged run's input executor.

[Field buff confirmation](../features/field-buffs.md) adds a shared per-character/room transaction for ordered requests, dispatch acknowledgement and separately verified visual effects/equipment restoration. Confirmed timers use the conservative pre-input observation time. A supervised Arcane sequence supplied actual changed-max HUD references; generic I/II tab assets identify the active set without selecting an individual's battle gear. Belt inspection requires positive fresh closure before its movement gate passes. Field safety and whole-sequence effects currently require supervised evidence; the staged token executor remains unconnected.

[Casting lookup](../../diablo2/common/casting.py) now combines a shared class/skill/form reference catalog with the selected character's class and current FCR. [Shared data](../../config/game-rules/casting.md) owns the initial Sorceress/Paladin tables; [personal configuration](../../config/characters/characters.md#casting) owns Flash's confirmed FCR 105. [FEAT-0004](../plans/feature/feat-0004-class-casting-rules.md) covers input-free frame estimates and unsupported-context holds. The catalog does not issue inputs or enable adaptive movement.

[Common field control](../features/field-control.md) consumes that catalog with confirmed arrival timing, one held-key owner and a latest-observation mailbox. Independent deadlines release movement/attack on stale screens, buff renewal or action timeout; recovery, confirmed depletion, belt closure and loot confirmation preempt travel. A same-frame HUD bridge reconciles expanded belt counts without double-counting consumption. The injected continuous buff executor consumes each profile's order and gaps while retaining separate effect/equipment confirmation. These shared modules are implemented and short standard-input probes have run, but automatic scene/clear/death/pickup producers and staged GUI integration remain incomplete; [RUN-20261005-06](../plans/run/run-20261005-06-field-control-loop.md) owns current live limitations.

The CLI controller owns configurable dry-run, pause, and stop behavior. GUI action sessions send live input directly and have separate interruption handling; shared safety and pause coverage remain incomplete. See [system settings](../../config/system/system.md) for the actual setting and hotkey scope.

The sections below describe product requirements and future extensions unless explicitly marked as current.

## Future real-time observation and recording

The [runtime and recording PRD](../plans/prd/prd-0008-realtime-runtime-and-recording-retention.md) owns the proposed boundary for fast screen changes and long runs. This is a future requirement, not the current town loop's behavior.

```text
capture -> latest valid frame -> vision -> decision / single input owner
     |                                      |
     +-> bounded recording admission <------+ compact events
                    |
              encode / writer -> closed segments -> retention worker

safety / stop -> input owner (does not wait for writer flush)
```

Control consumes the latest valid observation; recording has bounded count/byte capacity and may sample or drop replaceable diagnostics with visible counters. Frame ownership must prevent a delayed writer from using a recycled image. Vision results carry the frame and run/movement generation they observed so late results cannot authorize input after teleportation. State/input ownership remains explicit even with multiple workers.

Encoding, JSON formatting, file writes, rotation, directory scans, deletion, and shutdown drain belong outside the control path. Queue saturation must not fall back to synchronous producer writes. Storage health is distinct from capture/survival health: diagnostic loss is reported, emergency input stays available, and evidence-dependent continuation is gated at a verified safe boundary. Separate workers still require latency/CPU/memory measurements.

The north runtime already separates capture/fast/slow/decision workers; its logger rotates bounded segments but has a synchronous high-priority overflow fallback. Town-loop evidence and PNG writes are synchronous. These are reuse candidates with unresolved gaps, not proof that this target is implemented. Retention will cover active segment bounds, total bytes/count/age, failed runs, free-space margins, and orphaned sessions; required assets are protected. [Development cleanup](developer-guide.md#evidence-handling) separately removes unneeded development data at session close.

## Future adaptive teleport movement

[Common encounter observation](../features/encounter-observation.md) now preserves region candidates, native crop/ground context, explicit personal radius and positive linked life-state evidence in input-free modules. Region manifests own shared appearances and terrain palettes; character policy owns thresholds/bindings. Run-specific input executors remain separate. Actual north supervision exposed pose/HUD misses and buff expiry during return, so live combat requires independent safety/buff supervision and confirmed observations before integration.

The [navigation PRD](../plans/prd/prd-0005-arcane-navigation-and-recovery.md#adaptive-teleport-requirements) owns future teleport pacing across early-season and equipped characters. [Sourced class/casting references](../research/class-cast-rate-reference.md) distinguish shared rules from personal equipment and session observations. The proposed ownership is:

| Layer | Ownership |
| --- | --- |
| Shared game rules | Verified ruleset/version, caster class, skill animation family and applicable form/animation conditions; FCR thresholds and cast frames. Profiles of the same class share this catalog. |
| Individual character | Reference to the class, confirmed total FCR of the active equipment set, movement skill/bindings and personal safety settings. Display name is identity, not a casting rule. |
| Shared control and session state | Rule lookup, bounded pacing/confirmation policy, observed arrival/observation delay and context validity. Session measurements do not rewrite class rules. |
| Run | Landmarks, destination and completion rules; consumes the common movement control. |

The common field controller already adjusts aim timing from class/FCR references and producer-confirmed arrival delays, with bounded waits and independent safety/stop releases. Automatic route landing/readiness producers and staged executor integration remain incomplete. Equipment swaps or class/skill/form changes invalidate assumptions as required by the selected contract. Casting expectations and capture/vision delay remain distinct under the [runtime PRD](../plans/prd/prd-0008-realtime-runtime-and-recording-retention.md). Missing class/FCR or unsupported rules never inherit another character's table.

Each run supplies its landmarks, destination and completion rules. Summoner and future Diablo runs can consume the same timing/confirmation control; each route still needs its own validation. The staged north route still uses its existing controls. [RUN-20261005-06](../plans/run/run-20261005-06-field-control-loop.md) separates injected adaptation checks and short supervised input from the unverified full route and slower-equipment comparison.

## Product direction

The app should operate through normal Windows GUI interaction only:

- observe the screen
- understand game state from images and text
- decide what to do from configurable rules
- send normal mouse and keyboard input when enabled

Current observation tools include CLI dry-run/overlays, logging, recording, and snapshots. Further support should include:

- shared dry-run behavior across action sessions
- replay analysis

## Core system areas

### 1. Control surface

The Tkinter desktop GUI is the current operator-facing control layer; the CLI preview remains available.

Further controls:

- start bot profile
- pause and resume
- stop immediately
- enable dry-run vs live input
- switch between farm profiles

Possible future surfaces:

- tray icon
- small overlay status panel

### 2. Human override and interruption

Manual user input must take priority over automation.

Required behavior:

- if the user presses the configured stop hotkey, the bot stops
- if the user presses the configured pause hotkey, the bot pauses
- if the user starts actively using mouse or keyboard, automation should suspend or stop based on configuration
- bot actions should always be recoverable without restarting the program

Implementation ideas:

- global hotkeys
- recent-user-input watcher
- cooldown period before automation resumes

### 3. Survival logic

The bot should protect the character before trying to optimize farming.

Required signals:

- life orb or life bar monitoring
- mana monitoring
- potion belt availability
- mercenary survival status if relevant
- dangerous-state detection such as death screen, low life, frozen screen, or disconnect-like states

Required actions:

- drink life potion
- drink mana potion
- use rejuvenation potion
- retreat or town portal in critical state
- stop automation if survival logic is uncertain

### 4. Character behavior

Combat behavior should depend on the character build.

Current character configs provide movement keys and buff action sequences. The GUI selects these configs; automatic in-game character-row selection and build-aware combat remain future work. Field details live in [characters.md](../../config/characters/characters.md).

Examples:

- hammerdin
- blizzard sorceress
- lightning sorceress
- javazon
- summon necromancer

Behavior inputs:

- primary attack skill
- secondary attack skill
- buff cycle
- movement style
- potion thresholds
- targeting rules

### 5. Game mode and character grouping

The app should separate shared repeatable behavior from character-specific behavior.

Shared screen/action services own repeatable UI transitions; character profiles supply bindings and overrides. Character-row detection and mode-marker design belong to [game-modes.md](../features/game-modes.md); configured fields belong to [characters.md](../../config/characters/characters.md).

### 6. Farm profiles

Automation should be organized by farm payload/profile instead of one giant script.

Early profile candidates:

- Diablo run
- Baal run
- Terror Zone farm
- Summoner run
- Pindleskin run
- Travincal run

Each profile should eventually define:

- entry conditions
- route steps
- combat rules
- loot rules
- exit conditions
- recovery rules

### 7. Loot intelligence

Loot handling should be data-driven, not buried inside code.

The current pickup session matches configured ground-item templates and clicks their labels. OCR, affix evaluation, and a survival-aware approach/pickup coordinator remain future work.

Needed features:

- item label OCR
- keep or ignore decision rules
- item category rules
- character/profile-specific keep rules
- logging of dropped and kept items

Data sources to use as references:

- Blizzard classic item reference: [The Arreat Summit item pages](https://classic.battle.net/diablo2exp/items/)

Suggested internal categories:

- runes
- keys
- gems
- charms
- jewels
- bases
- uniques
- sets
- crafting materials
- gold
- consumables
