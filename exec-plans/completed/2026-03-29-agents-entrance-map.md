# AGENTS Entrance Map Review

## Metadata

- `status`: `active`
- `agent`: `main`
- `created`: `2026-03-29`
- `updated`: `2026-03-29`
- `scope`: `Evaluate whether AGENTS.md should stay as a high-level entrance map and move detailed guidance into dedicated markdown documents`
- `related_files`: `AGENTS.md`, `exec-plans/execution-plan.md`, `docs/`, `config/`, `https://openai.com/ko-KR/index/harness-engineering/`

## Request

Review the current `AGENTS.md` against the user's preference that it remain a project entrance map, compare that goal with the referenced OpenAI Harness Engineering guidance, and identify what details should stay in `AGENTS.md` versus move into more concrete markdown files.

## Context

- The repository already uses `AGENTS.md` as contributor guidance.
- The user wants details managed in separate markdown documents when possible.
- A prior active plan already covers package and documentation structure review, but this task is specifically about the role and scope of `AGENTS.md`.

## Findings

- The OpenAI Harness Engineering article recommends treating `AGENTS.md` as a map or table of contents rather than a large encyclopedia, with deeper guidance stored in structured repository docs.
- The current `AGENTS.md` had drifted beyond an entrance map because it included detailed contributor workflow rules, comment policy, verification rules, GUI maintenance details, and execution-plan instructions.
- The repository already had a usable `docs/project/` area, which made it practical to move detailed maintenance guidance into dedicated markdown files without inventing a new documentation root.
- The most natural split in this repository is:
  - `AGENTS.md` for project map and top-level rules
  - `docs/project/developer-guide.md` for detailed development workflow and maintenance rules
  - `docs/project/gui-maintenance.md` for GUI-specific maintenance guidance
  - `exec-plans/execution-plan.md` for execution-plan process and template
- Execution-plan instructions should not be duplicated in detail across multiple docs. `AGENTS.md` should keep the short general rule, and `exec-plans/execution-plan.md` should remain the detailed source of truth.
- The rule to review and update related files when making changes is general enough to belong in `AGENTS.md` rather than only in the contributor detail doc.

## Decisions

- Keep `AGENTS.md` intentionally concise and map-like.
- Move detailed contributor workflow guidance into `docs/project/developer-guide.md`.
- Move GUI-specific maintenance guidance into `docs/project/gui-maintenance.md`.
- Keep execution-plan workflow in `exec-plans/execution-plan.md` and link it from `AGENTS.md`.
- Add a repository rule to keep documentation neat, clean, and concise, and to merge duplicated guidance upward into the right canonical document.
- Rename the detailed contributor workflow document to `docs/project/developer-guide.md`.

## Task List

- [x] Create an execution plan for this AGENTS structure review.
- [x] Review the OpenAI Harness Engineering article and extract relevant guidance.
- [x] Compare the current `AGENTS.md` contents to the desired entrance-map role.
- [x] Move overly detailed guidance out of `AGENTS.md` into dedicated markdown files if appropriate.
- [x] Leave `AGENTS.md` as a concise directory and rule entrypoint with links.
- [x] Remove duplicated execution-plan detail from secondary docs and keep one detailed canonical source.
- [x] Move the general "update related things" rule into `AGENTS.md`.
- [x] Rename the detailed contributor workflow document to `docs/project/developer-guide.md`.

## Risks

- Over-splitting documentation can make contributor onboarding harder if links are unclear.
- Under-splitting will keep `AGENTS.md` dense and harder to maintain.

## Next Step

Use the new split as the default pattern for future maintenance: keep `AGENTS.md` brief and move detailed contributor guidance into dedicated docs as the repository grows.
