# AGENTS.md

## Purpose

This file is the project entrance map for contributors and coding agents working inside this repository.
Keep this file short and map-like. Put detailed guidance in the linked Markdown documents.
Keep user onboarding, setup, and basic usage in `README.md`.

## Codebase Map

- `main.py`: project entrypoint
- `diablo2/`: main Python package
- `config/`: runtime behavior and profiles
- `docs/`: project, feature, setup, and maintenance documentation
- `assets/`: image templates and other runtime assets
- `docs/plans/`: PRD, feature, spec, run, evaluation, fix, and heuristic artifacts

## Key Docs

- `README.md`: user-facing project overview and startup
- `docs/project/policy.md`: project safety and technical boundaries
- `docs/project/architecture.md`: product direction and system structure notes
- `docs/project/roadmap.md`: build order, open questions, and near-term upgrades
- `docs/project/developer-guide.md`: detailed development workflow, style, verification, and documentation rules
- `docs/project/gui-maintenance.md`: GUI layout and window-tuning maintenance notes
- `docs/agents/README.md`: local harness entrypoint, task routing, and import maintenance
- `config/config.md`: config directory overview
- `docs/plans/README.md`: canonical planning artifacts and current work

## Working Rules

- When changing code or documents, review related comments, nearby docs, and adjacent maintenance guidance, and update them if they became stale.
- Use kebab-case for markdown filenames under `docs/` unless there is a strong reason to preserve an existing name.
- Always keep project guidance neat, clean, and concise. If information is duplicated across docs, merge it into the appropriate upper-layer source and restructure the docs so they stay maintainable.
- Put detailed maintenance guidance in the linked docs above instead of expanding `AGENTS.md` unless the change affects the entrance-map itself.

## Planning And Execution Gate

- Follow `docs/agents/README.md` and the shared workflow for planning and approved feature execution; keep each artifact in its canonical owner under `docs/plans/`.
- PRD requests are planning-only until the human owner approves the boundary. Draft PRDs and unapproved feature proposals must not trigger executable specs, code changes, or evaluation runs.
- If an unresolved planning point can change scope, acceptance, dependencies, or user-visible behavior, resolve it with the human owner before execution.
- When a canonical target starts or resumes and prior context materially affects the work, apply `docs/policies/harness/operator-briefing-and-review-receipts.md`; otherwise preserve the normal response shape.
