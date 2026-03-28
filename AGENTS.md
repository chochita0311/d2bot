# AGENTS.md

## Purpose

This file is for contributors and coding agents working inside this repository.
Keep user onboarding, setup, and basic usage in `README.md`.

## Codebase Map

- `main.py`: project entrypoint
- `diablo2/app.py`: CLI argument handling and GUI/CLI dispatch
- `diablo2/ui/`: Tkinter desktop GUI
- `diablo2/common/`: capture, config, and shared helpers
- `diablo2/actions/`: user-triggered actions and sessions
- `diablo2/runs/`: run-specific automation logic
- `config/`: runtime behavior and profiles

## Working Style

- Prefer small, focused changes that preserve current behavior unless the task explicitly asks for a UI or behavior change.
- Read the local code first before changing project structure or patterns.
- Keep edits Windows-friendly.
- Do not revert unrelated user changes.

## Code Style

- Follow existing Python style in the touched file.
- Avoid adding noisy comments for obvious assignments.

## Comment Style

- Keep comments short and intent-focused.
- Prefer comments that explain why a block exists, what decision rule it applies, or how a tuning constant affects behavior.
- Use section comments for grouped constants, control-flow stages, and non-obvious helper functions.
- Avoid line-by-line narration; skip comments that only restate the code literally.
- Write code comments in Korean.
- For UI code, prefer section comments that explain layout intent over line-by-line commentary.
- Do not regenerate Korean comments through lossy shell or whole-file rewrite flows that can collapse characters into `?`.
- Prefer targeted edits when changing Korean comments, especially in existing files with mixed old encodings.
- After editing Korean text, verify the file still contains real Unicode Korean characters instead of literal `?`.
- If terminal output looks broken, verify the file bytes or decoded text rather than trusting the console rendering.

## GUI Notes

- The main desktop window lives in `diablo2/ui/gui.py`.
- Keep `diablo2/ui/gui.py` as the main reference point for GUI layout and window-size tuning guidance.
- Default startup size is controlled by:
  - `DEFAULT_WINDOW_WIDTH`
  - `DEFAULT_WINDOW_HEIGHT`
  - `MIN_WINDOW_WIDTH`
  - `MIN_WINDOW_HEIGHT`
- When adjusting the main window, prefer changing those constants instead of scattering raw geometry strings.
- Preserve the existing layout rule that the log panel expands while the right control panel stays close to its natural width.

## Config Notes

- Prefer changing behavior through `config/` when possible instead of hard coding values.
- Keep capture-related behavior aligned with the config guides and current runtime behavior.

## Verification

- For Python-only edits, run at least `python -m py_compile` on changed files when practical.
- If a change affects GUI layout, verify the touched module still imports and compiles cleanly.

## Documentation

- Update nearby comments when changing non-obvious behavior.
- When code changes, check whether nearby comments became stale and refresh them if needed.
- If a new constant or manual tuning point is introduced, make it easy to find and edit.
- Keep relevant project description files in `.md` up to date when behavior, structure, setup, or developer workflow changes.
- Put developer-facing maintenance guidance in `AGENTS.md`, and keep first-time user guidance in `README.md` or other user-facing Markdown docs.
