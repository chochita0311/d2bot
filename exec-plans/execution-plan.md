# Execution Plans

- Before starting a multi-step implementation, refactor, or documentation pass, write or update a plan document under `exec-plans/active/`.
- Keep the active plan current as understanding changes, especially when scope, findings, risks, or next steps change.
- When the work covered by a plan is finished, move that plan document to `exec-plans/completed/`.
- If follow-on work is still needed, prefer either:
  - adding new unchecked tasks and keeping the plan in `exec-plans/active/` when it is still the same continuing plan
  - or moving the finished plan to `exec-plans/completed/` and creating a short completion note or a new active plan for the next scope when the follow-on work is meaningfully separate
- When a completed plan surfaced meaningful development hardships, debugging traps, or reusable lessons, record them in a separate markdown file under `docs/lessons/` so future work can refer back to them.
- Use kebab-case filenames with a date prefix when helpful, for example `2026-03-29-package-doc-refactor.md`.
- Treat the plan document as the working memory for the task: decisions, findings, risks, and next actions should live there instead of only in chat.

## Plan Template

Use this structure for every execution plan document:

```md
# <Plan Title>

## Metadata

- `status`: `active` | `blocked` | `completed`
- `agent`: `main` | `<subagent-name>`
- `created`: `YYYY-MM-DD`
- `updated`: `YYYY-MM-DD`
- `scope`: `<short scope summary>`
- `related_files`: `<paths or n/a>`

## Request

Brief restatement of the user request or task trigger.

## Context

Relevant project background, constraints, and assumptions.

## Findings

- Concrete observations with file references when possible.

## Decisions

- Chosen direction and why.

## Task List

- [ ] Step to do
- [ ] Step to do
- [x] Completed step

## Risks

- Known risks, open questions, or dependencies.

## Next Step

Immediate next action to take when work resumes.
```
