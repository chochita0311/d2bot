# PRD-0001: Harness Documentation Conformance

## Metadata

- ID: `prd-0001`
- Status: `approved`
- Owner role: `human`
- Created: `2026-10-04`
- Updated: `2026-10-04`
- Boundary authority: the user's explicit request for a canonical harness installation and current documentation owners.

## Request Summary

Align the project's documentation and planning entrypoints with the installed ai-assets harness. Maintain current operational guidance and canonical planning artifacts.

## Source Set

- Human request: canonical harness installation and current documentation ownership.
- Golden source: the ai-assets `agents/ADOPTION-GUIDE.md` and shared policies at the revision recorded in the [manifest](../../agents/harness-import-manifest.json).
- Installed authority: [planning management](../../policies/harness/prd-feature-management.md), [execution governance](../../policies/harness/execution-loop-governance.md), and [traceability](../../policies/harness/traceability-and-link-hygiene.md).
- Project sources: current entrance, developer, feature, configuration, and route documentation, checked against the relevant implementation.

## Product Intent

Readers should find one planning and execution system, clear document owners, and usable guidance for the current implementation.

## Confirmed Scope

- Canonical planning/execution directories and shared workflow own new harness artifacts.
- The project entrance and local guide point directly to those owners.
- Reusable operating and diagnostic guidance belongs to the relevant feature or development reference.
- Unfinished package work remains unapproved planning input.
- Publish the reviewed session-owned documentation through a focused commit/push to the configured repository, as explicitly requested by the user.

## Excluded Scope

- Runtime changes, package refactoring, personal Codex configuration, and publication of preexisting unrelated work.
- Changes to the 35 managed shared assets or their manifest baselines.
- Broad rewrites of stable user, config, feature, or architecture references.

## Uncertainty

- Package refactor boundaries require separate human planning review.

## Constraints

- Preserve useful operational guidance and preexisting code/config changes.
- Record only supported approval states and validation results.
- Use repository-relative active links and preserve committed shared-file provenance.
- Keep preexisting Python changes and the mixed-ownership config overview and package-local route document out of staging; preserve their working-tree bytes.

## Acceptance Envelope

- Active entrypoints use the canonical workflow and artifacts.
- Maintained documents contain current guidance and canonical artifacts with valid references to their owners.
- Remaining package planning has a draft owner rather than executable approval.
- Managed file hashes, local links, and scoped whitespace checks pass.
- No file in active use by another task is removed.
- The staged file set is limited to the reviewed documentation/harness changes from this session.

## Candidate Features

- [feat-0001](../feature/feat-0001-harness-documentation-ownership.md): establish canonical documentation ownership and current operational references.

## Continuity Notes

- `2026-10-04`: the user approved canonical documentation ownership and current-reference consolidation. Package refactoring remains outside the approved scope.
- `2026-10-04`: the user requested whole-repository reviews with both documentation skills, then commit/push of only this session's documentation changes.
