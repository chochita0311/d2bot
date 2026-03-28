# Future Package Refactoring

## Why this note exists

The current package tree is workable, but some boundaries are starting to blur:

- `diablo2/app.py` is handling entrypoint and runtime composition
- `diablo2/ui/` is the desktop presentation layer
- `diablo2/core/` sounds like domain/core logic, but real behavior is also spread across `actions/`, `runs/`, and `common/`
- `diablo2/common/` is becoming a broad shared bucket instead of a clearly scoped module

This is not an urgent rewrite target.
It is a future cleanup direction so package responsibilities stay easier to understand as the project grows.

## Current structure assessment

Reasonable now:
- `ui/` is clear and should remain easy to find
- `actions/` and `runs/` are meaningful concepts in this repo
- root `app.py` is acceptable while startup flow is still relatively small

Less clean now:
- `common/` is too generic and can become a junk drawer
- `core/` is vague and does not clearly define what belongs there
- startup/runtime wiring will likely outgrow a single root `app.py`
- automation logic is conceptually split across several folders without one clear top-level home

## Recommended direction

Prefer a light refactor path instead of a large rewrite.

Suggested future shape:

```text
diablo2/
  app/
    __init__.py
    cli.py
    entry.py
  ui/
    gui.py
  automation/
    actions/
    runs/
  services/
    capture.py
    config.py
    detectors.py
    movement.py
    realtime.py
  models/
```

## Meaning of the proposed folders

- `app/`: startup, argument parsing, mode selection, bootstrap wiring
- `ui/`: desktop presentation layer
- `automation/`: behavior that performs in-game work such as actions and run flows
- `services/`: technical services and shared runtime helpers such as capture, config, movement, vision, and detectors
- `models/`: shared state objects, dataclasses, and domain-level data shapes if they become large enough to justify their own home

## Practical migration suggestions

If we do this later, prefer incremental moves:

1. Keep `ui/` as-is.
2. Move `diablo2/app.py` toward `diablo2/app/entry.py`.
3. Gradually replace `common/` with more explicit modules or subpackages.
4. Group `actions/` and `runs/` under `automation/`.
5. Only keep `core/` if its ownership rule is clearly defined; otherwise phase it out.

## What not to do

- Do not do a full package rename in one step unless there is a strong reason.
- Do not move files just for aesthetics without clarifying ownership.
- Do not keep both old and new package meanings ambiguous for long.

## When to start this

This becomes worth doing when one or more of these happen:

- `app.py` becomes crowded with startup/runtime responsibilities
- `common/` keeps growing without a clear rule
- new contributors cannot tell where new modules should live
- GUI, CLI, and automation runtime wiring start stepping on each other

## Refactor goal

The goal is not a "perfect architecture".
The goal is a package tree where a developer can quickly answer:

- Where does startup logic belong?
- Where does GUI code belong?
- Where does automation behavior belong?
- Where do technical runtime helpers belong?
- Where should a new file go without guessing?
