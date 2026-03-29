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
- `scripts/`: local helper scripts
- `exec-plans/`: active and completed execution plans

## Key Docs

- `README.md`: user-facing project overview and startup
- `docs/project/policy.md`: project safety and technical boundaries
- `docs/project/architecture.md`: product direction and system structure notes
- `docs/project/roadmap.md`: build order, open questions, and near-term upgrades
- `docs/project/developer-guide.md`: detailed development workflow, style, verification, and documentation rules
- `docs/project/gui-maintenance.md`: GUI layout and window-tuning maintenance notes
- `config/config.md`: config directory overview
- `exec-plans/execution-plan.md`: execution-plan workflow and template

## Working Rules

- When changing code or documents, review related comments, nearby docs, and adjacent maintenance guidance, and update them if they became stale.
- Use kebab-case for markdown filenames under `docs/` unless there is a strong reason to preserve an existing name.
- Always keep project guidance neat, clean, and concise. If information is duplicated across docs, merge it into the appropriate upper-layer source and restructure the docs so they stay maintainable.
- Put detailed maintenance guidance in the linked docs above instead of expanding `AGENTS.md` unless the change affects the entrance-map itself.

## Execution Plans

- Before developing, refactoring, or making a substantial documentation change, always start by writing a detailed plan in `exec-plans/` so the current task context, decisions, and progress are preserved.
- For multi-step implementation, refactor, or documentation work, keep the active plan up to date under `exec-plans/active/`.
- Execution-plan workflow and template live in `exec-plans/execution-plan.md`.
