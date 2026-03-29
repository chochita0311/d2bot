# Developer Guide

This document holds development workflow details that are too specific or verbose for `AGENTS.md`.

## Working Style

- Prefer small, focused changes that preserve current behavior unless the task explicitly asks for a UI or behavior change.
- Read the local code first before changing project structure or patterns.
- Keep edits Windows-friendly.
- Do not revert unrelated user changes.
- Prefer changing behavior through `config/` when possible instead of hard coding values.

## Code Style

- Follow existing Python style in the touched file.
- Avoid adding noisy comments for obvious assignments.
- Use the repository formatter configuration in `pyproject.toml` when formatting Python code.
- Run the formatter after meaningful Python edits, especially before finishing a task, when touching multiple lines in a file, or when a change leaves formatting inconsistent with nearby code.

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

## Verification

- For Python-only edits, run at least `python -m py_compile` on changed files when practical.
- If a change affects GUI layout, verify the touched module still imports and compiles cleanly.

## Documentation

- If a new constant or manual tuning point is introduced, make it easy to find and edit.
- Keep relevant project description files in `.md` up to date when behavior, structure, setup, or developer workflow changes.
- Keep developer-facing maintenance guidance in repository docs such as `AGENTS.md`, `docs/project/*.md`, and `exec-plans/*.md`.
- Keep first-time user guidance in `README.md` or other user-facing markdown docs.

## Execution Plans

- Follow `exec-plans/execution-plan.md` for the shared workflow and template.
