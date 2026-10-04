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

## Interpreter Diagnostics

Use the project's `.venv\Scripts\python.exe` for both IDE and terminal checks. If package installation reports missing SSL support, first inspect that exact interpreter:

```powershell
.\.venv\Scripts\python.exe -c "import sys, ssl; print(sys.executable); print(sys.prefix); print(sys.base_prefix); print(ssl.OPENSSL_VERSION)"
.\.venv\Scripts\python.exe -m pip --version
```

Check that the executable and pip belong to the intended environment. A failing `import ssl` must be resolved at the interpreter/base-environment boundary before diagnosing a particular package. Compare the same executable inside and outside the IDE. If rebuilding the environment is required, verify SSL in the chosen base interpreter first, then follow [README setup](../../README.md#quick-start) and select the rebuilt `.venv` in the IDE. Shell-only DLL/PATH fixes are diagnostic evidence rather than a stable IDE setup contract.

## Evidence Handling

Feature references own reusable operating and tuning guidance. Plans/runs and their evaluator reports own execution decisions and validation evidence. Keep the current approved boundary and actual evidence coverage in those canonical artifacts.

For visual automation changes, retain the recordings and logs needed to explain a recognition, movement, or state-transition decision. The GUI log is a bounded live view; enabled file logging provides durable diagnostics. Mark recordings that must survive retention pruning and avoid removing evidence while another session uses it. See [system settings](../../config/system/system.md) for logging and recording retention, and the feature reference for the states that need observation.

## Documentation

- If a new constant or manual tuning point is introduced, make it easy to find and edit.
- Keep relevant project description files in `.md` up to date when behavior, structure, setup, or developer workflow changes.
- Keep project-specific maintenance guidance in repository docs such as `AGENTS.md` and `docs/project/*.md`. Shared harness policies own planning and execution rules; `docs/plans/` owns their artifacts.
- Keep first-time user guidance in `README.md` or other user-facing markdown docs.

## Planning And Execution

- Use [the local harness guide](../agents/README.md) for the shared workflow and role contracts, and [planning artifacts](../plans/README.md) for templates and current records.
- Project-specific implementation and verification rules remain in this guide; planning, approval, and run lifecycle rules remain in the installed shared policies.
