# Architecture

This document organizes the high-level product direction and system structure for the Diablo 2 Windows GUI automation project.

## Current implementation

| Area | Responsibility |
| --- | --- |
| `main.py`, `diablo2/app.py` | CLI arguments and dispatch to the desktop GUI or CLI preview. |
| `diablo2/ui/gui.py` | Tkinter capture/recording controls, character-config selection, and action launch/stop controls. |
| `diablo2/actions/` | Recording, gem combining, ground-template loot pickup, and room create/exit sessions. |
| `diablo2/runs/` | Run definitions and the current Summoner GUI adapter, main loop, navigation, encounter and room coordination under `runs/summoner/`. |
| `diablo2/common/` | Config loading, Windows capture, template detection, movement/input ownership, survival/buff policies and producers, and the CLI controller. |
| `diablo2/core/bot.py` | Older CLI capture/template/overlay loop. |
| `config/`, `assets/` | Runtime settings and profiles, plus screenshot-derived matching assets. |

The current Summoner GUI calls `SummonerGuiSession` → `main_run.run` → `run_once`. It connects current-screen navigation, HUD/resource checks, independent input deadlines, automatic buff dispatch, bounded nearby Nova and conditional post-combat loot scans. The old north-route workers and staged entry/helpers have been removed. [Summoner guidance](../features/summoner-run.md) owns operation; the [integration run](../plans/run/run-20261009-07-summoner-live-integration.md) owns actual failures, evidence and next-session work. Fresh-room north completion remains 0/5 and accepted full runs remain zero; connected code is not live acceptance.

[Character survival state](../../diablo2/common/survival.py) provides character-owned policy validation, confirmed per-room buff/belt state, and input-free recovery/exit/replenishment requests. It has no capture or input dependencies and can be consumed by other runs. [Character configuration](../../config/characters/characters.md#survival) owns the fields; [FEAT-0003](../plans/feature/feat-0003-character-survival-state.md) owns the tested boundary. Input coordination, cooldowns and action-result confirmation belong to its consumer. Main separately consumes live resource ratios and emergency recovery; a request does not confirm consumption, acquisition or successful escape.

[Survival HUD observation](../features/survival-observation.md) supplies a CV producer and bounded input-free CLI. Shared layout/glyph/item assets read current/max resource pairs and expanded belt slots for one reviewed layout; main consumes the same producer. Ratios never reuse pre-buff maxima; unknown readings remain unknown. HUD readings alone supply no field, clear-surroundings or buff-effect confirmation.

[Field buff confirmation](../features/field-buffs.md) owns the shared per-character/room transaction for ordered requests, dispatch acknowledgement and separately verified visual effects/equipment restoration. Confirmed timers use the conservative pre-input observation time. A supervised Arcane sequence supplied actual changed-max HUD references; generic I/II tab assets identify the active set without selecting an individual's battle gear. Belt inspection requires positive fresh closure before its movement gate passes. Current main uses a separate approved dispatch policy: main weapon set is a starting precondition, nearby candidates/resource stability are observed automatically, and Flash renews at the actual S request +140 seconds. It performs no inventory inspection or supervisor-JSON wait. Dispatch remains unconfirmed and whole-sequence effects remain independently unverified; timely automatic renewal still needs live validation.

[Casting lookup](../../diablo2/common/casting.py) now combines a shared class/skill/form reference catalog with the selected character's class and current FCR. [Shared data](../../config/game-rules/casting.md) owns the initial Sorceress/Paladin tables; [personal configuration](../../config/characters/characters.md#casting) owns Flash's confirmed FCR 105. [FEAT-0004](../plans/feature/feat-0004-class-casting-rules.md) covers input-free frame estimates and unsupported-context holds. The catalog does not issue inputs or enable adaptive movement.

[Common field control](../features/field-control.md) consumes that catalog with confirmed arrival timing, one held-key owner and a latest-observation mailbox. Independent deadlines release movement/attack on stale screens, buff renewal or action timeout; recovery, confirmed depletion, belt closure and loot confirmation preempt travel. A same-frame HUD bridge reconciles expanded belt counts without double-counting consumption. The injected continuous buff executor consumes each profile's order and gaps while retaining separate effect/equipment confirmation. These shared modules are implemented and short standard-input probes have run. Current GUI main composes shared input pieces with its own screen producers; full `FieldRuntime`/`ArcaneSearchRuntime` composition remains separate. Independent whole-scene clear, linked target death and acquired-item producers remain incomplete. [RUN-20261005-06](../plans/run/run-20261005-06-field-control-loop.md) owns the earlier API/probe evidence; the integration run owns current main behavior and live limitations.

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

Earlier north workers and their asynchronous segmented file logger have been removed. The GUI keeps a bounded live text view; current Summoner evidence/report output and town-loop PNG writes do not establish the isolated recording pipeline above. Standard CLI logging remains separate. Proposed retention will cover active segment bounds, total bytes/count/age, failed runs, free-space margins, and orphaned sessions; required assets are protected. [Development cleanup](developer-guide.md#evidence-handling) separately removes unneeded development data at session close.

## Future adaptive teleport movement

[Common encounter observation](../features/encounter-observation.md) preserves region candidates, native crop/ground context, explicit personal radius and positive linked life-state evidence in input-free modules. Region manifests own shared appearances and terrain palettes; character policy owns thresholds/bindings. Run-specific input executors remain separate. Earlier north supervision exposed pose/HUD misses and buff expiry during return. Current bounded nearby Nova and automatic rebuff dispatch do not establish verified target death, whole-scene safety or timely live renewal.

The [navigation PRD](../plans/prd/prd-0005-arcane-navigation-and-recovery.md#adaptive-teleport-requirements) owns future teleport pacing across early-season and equipped characters. [Sourced class/casting references](../research/class-cast-rate-reference.md) distinguish shared rules from personal equipment and session observations. The proposed ownership is:

| Layer | Ownership |
| --- | --- |
| Shared game rules | Verified ruleset/version, caster class, skill animation family and applicable form/animation conditions; FCR thresholds and cast frames. Profiles of the same class share this catalog. |
| Individual character | Reference to the class, confirmed total FCR of the active equipment set, movement skill/bindings and personal safety settings. Display name is identity, not a casting rule. |
| Shared control and session state | Rule lookup, bounded pacing/confirmation policy, observed arrival/observation delay and context validity. Session measurements do not rewrite class rules. |
| Run | Landmarks, destination and completion rules; consumes the common movement control. |

The common field controller adjusts aim timing from class/FCR references and producer-confirmed arrival delays, with bounded waits and independent safety/stop releases. Its full `ArcaneSearchRuntime` composition and broader landing/readiness validation remain incomplete. Equipment swaps or class/skill/form changes invalidate assumptions as required by the selected contract. Casting expectations and capture/vision delay remain distinct under the [runtime PRD](../plans/prd/prd-0008-realtime-runtime-and-recording-retention.md). Missing class/FCR or unsupported rules never inherit another character's table.

Each run supplies its landmarks, destination and completion rules. Summoner and future Diablo runs can consume the same timing/confirmation control; each route still needs its own validation. Current Summoner main uses the selected class/FCR reference with current terrain/background observations and held-input deadlines. [RUN-20261005-06](../plans/run/run-20261005-06-field-control-loop.md) separates injected adaptation checks and short supervised input from the unverified full route and slower-equipment comparison.

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
