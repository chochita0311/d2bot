# Package And Documentation Structure Review

## Metadata

- `status`: `active`
- `agent`: `main`
- `created`: `2026-03-29`
- `updated`: `2026-03-29`
- `scope`: `Package structure review, documentation overlap review, and staged refactor direction`
- `related_files`: `AGENTS.md`, `README.md`, `config/runs/runs.md`, `config/characters/characters.md`, `docs/features/farm-profiles.md`, `docs/features/game-modes.md`, `docs/features/summoner-run.md`, `diablo2/runs/__init__.py`, `diablo2/common/config.py`, `diablo2/app.py`, `diablo2/core/bot.py`

## Request

Scan the whole project, review package structure, review the documentation set, and suggest refactoring or merges where the current layout is duplicated or unstable.

## Context

- The repository is Windows-first and uses a mixed GUI, CLI, config, and automation structure.
- There are existing user edits in the worktree, so this review should not revert or overwrite unrelated changes.
- The repository already contains `exec-plans/active/` and `exec-plans/completed/` folders, so this task should establish a concrete execution-plan convention there.

## Findings

- `diablo2` already has a package-boundary problem, and the current layout is starting to lock it in.
  - `diablo2/common/config.py` mixes datamodels, config loading, merge rules, and selection logic in one module.
  - `diablo2/app.py` and `diablo2/core/bot.py` split runtime wiring across `common`, `core`, and `ui` with no clear ownership rule.
  - `ui/` is already a clear, stable boundary, but `common/` is too generic, `core/` is vague, and startup/runtime wiring will likely outgrow the current single-file `app.py` shape.
  - The most natural future direction is an incremental split toward `app/`, `automation/`, `services/`, and optionally `models/`.
- The generic `runs` package is coupled to the only current run implementation.
  - `diablo2/runs/__init__.py` re-exports Summoner-specific symbols from the package root, so `diablo2.runs` is no longer a neutral namespace for run abstractions.
- Run and profile docs are duplicated across several layers.
  - Similar concepts appear in `README.md`, `config/runs/runs.md`, `docs/features/farm-profiles.md`, and `docs/features/summoner-run.md`.
  - The files are not contradictory today, but they repeat field lists, profile rules, and planner intent.
- Character and game-mode documentation is split between a design doc and a config guide with overlapping semantics.
  - `docs/features/game-modes.md` and `config/characters/characters.md` both define mode vocabulary and marker interpretation.
- Developer docs are mixed into importable package paths.
  - `diablo2/runs/summoner/routes/docs/north_go.md` is a useful doc but does not belong under the runtime package tree.
- `README.md` still contains a forward-looking `Suggested next upgrades` section that fits better in a project roadmap or notes document than in a user-facing entry document.
- `docs/logs/` is misleadingly named because its current content is not runtime or program logs; it stores retrospective lessons learned.
- `AGENTS.md` and the detailed developer workflow document were still duplicating several working-style rules, which weakened the entrance-map separation and made the guidance harder to maintain cleanly.
- General environment setup guidance is split between `README.md` and `docs/setup/environment.md`, and the latter is too specific to one local Anaconda/DLL issue for a general-purpose project setup document.
- The repo currently has both `.venv` and `venv` directories locally, but neither is tracked, so docs can safely standardize on one canonical environment path without requiring repository-level migration.
- Several markdown files under `docs/` used underscores in filenames, which conflicts with the cleaner kebab-case naming convention already used elsewhere in the repo.
- The legacy helper script `scripts/activate-project.ps1` became obsolete after rebuilding `.venv` from a healthy standalone Python interpreter, because normal `.venv` activation now works without path hacks.
- `docs/project/evaluation-standard.md` is not really a whole-project governance document; it is an evaluation rubric for route or steering behavior and fits better in a dedicated evaluation documentation area.
- `docs/project/notes.md` is overloaded: it mixes architecture direction, roadmap planning, open questions, near-term upgrades, and retrospective Summoner lessons in one file.

## Decisions

- Use an incremental refactor direction rather than a big package rename.
- Keep `ui/` as-is, move startup/runtime wiring toward an `app/` boundary, gradually replace `common/` with more explicit service-oriented modules, group `actions/` and `runs/` under an automation-oriented boundary later, and only keep `core/` if its ownership becomes explicit.
- Treat `docs/` as the canonical home for architecture and implementation notes.
- Keep `config/*/*.md` focused on schema and field contract guidance.
- Record future multi-step work in `exec-plans/active/` before proceeding so package and doc cleanup can be tracked consistently.
- Keep `README.md` focused on current usage and move roadmap-style upgrade lists into `docs/project/notes.md`.
- Rename `docs/logs/` to `docs/lessons/` so the folder name matches its actual purpose.
- Extend the execution-plan workflow so completed plans with meaningful development hardships produce a reusable lesson document under `docs/lessons/`.
- Keep detailed day-to-day development rules in one detailed guide and leave `AGENTS.md` with only entrance-map and cross-cutting repository rules.
- Rename `docs/project/contributor.md` to `docs/project/developer-guide.md`.
- Make `README.md` the canonical general setup document and remove the machine-specific environment setup doc from the maintained documentation set.
- Standardize documentation on `.venv` as the single recommended local virtual environment path.
- Rename markdown files under `docs/` to kebab-case and keep that as the default documentation naming style.
- Remove `scripts/activate-project.ps1` because it is a legacy workaround for the old broken environment and is no longer part of the supported workflow.
- Create `docs/evaluation/` for evaluation rubrics and move route-evaluation guidance there instead of keeping it under `docs/project/`.
- Resolve standalone refactor notes into this active package/doc structure plan instead of keeping a separate architecture note under the importable package tree.
- Split `docs/project/notes.md` by purpose into separate architecture, roadmap, and lessons documents, then remove `notes.md`.

## Task List

- [x] Scan repository structure and documentation layout.
- [x] Identify package-boundary issues and documentation duplication.
- [x] Capture the review in a structured plan document under `exec-plans/active/`.
- [x] Update `AGENTS.md` with the execution-plan requirement and template.
- [x] Move the `Suggested next upgrades` roadmap out of `README.md` into a more appropriate project-notes document.
- [x] Rename the misleading lessons folder from `docs/logs/` to `docs/lessons/`.
- [x] Add a rule in the execution-plan guide for recording development hardships as lesson files after plan completion.
- [x] Remove duplicated working-style guidance between `AGENTS.md` and the detailed contributor/developer guide.
- [x] Rename `docs/project/contributor.md` to `docs/project/developer-guide.md`.
- [x] Make `README.md` the canonical general setup document.
- [x] Remove the machine-specific maintained environment setup doc.
- [x] Standardize docs on `.venv` as the canonical environment path.
- [x] Rename `docs/` markdown files from underscore style to kebab-case where appropriate.
- [x] Remove the legacy `scripts/activate-project.ps1` helper script and related references.
- [x] Create `docs/evaluation/` and move the route evaluation rubric there.
- [x] Merge the standalone future package refactor note into this active package/doc structure plan.
- [x] Split `docs/project/notes.md` into focused docs and remove the mixed notes file.
- [ ] Use this plan as the working document for the next package/doc cleanup task.

## Risks

- Package moves will have broad import fallout if done too early or without an incremental sequence.
- Documentation consolidation can accidentally delete useful nuance if canonical sources are not chosen first.
- There are existing user changes in the worktree, so future edits must avoid unrelated files unless explicitly in scope.

## Next Step

Use this plan as the baseline for the next task, starting with low-risk documentation consolidation and package-boundary cleanup rules before moving Python modules.
