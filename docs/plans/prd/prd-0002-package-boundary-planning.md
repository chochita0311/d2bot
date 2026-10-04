# PRD-0002: Package Boundary Planning

## Metadata

- ID: `prd-0002`
- Status: `draft`
- Owner role: `human`
- Created: `2026-10-04`
- Updated: `2026-10-04`

## Request Summary

Define package/documentation ownership for separate planning review. This draft does not authorize a package refactor.

## Source Set

- Implementation sources: [shared run exports](../../../diablo2/runs/__init__.py), [configuration models/loading](../../../diablo2/common/config.py), [startup wiring](../../../diablo2/app.py), and [CLI runtime](../../../diablo2/core/bot.py).
- Current references: [architecture](../../project/architecture.md), [developer guide](../../project/developer-guide.md), and [roadmap](../../project/roadmap.md).
- Planning authority: [PRD and feature management](../../policies/harness/prd-feature-management.md).

## Product Intent

Contributors should have clear ownership for shared run contracts, configuration, runtime wiring, and route-maintenance guidance without accidental behavior changes.

## Confirmed Scope

- Review the neutral run namespace and Summoner-specific ownership.
- Resolve ownership of config models/loading and startup/runtime wiring.
- Resolve the durable home and discoverability of route-maintenance guidance.
- Preserve existing user-facing behavior and compatibility expectations during any later approved work.

## Excluded Scope

- Implementation, module moves, import/API changes, game inputs, and behavioral tuning before review.
- Completed document consolidation and harness installation.

## Uncertainty

- The concrete module boundaries, migration order, compatibility requirements, and validation evidence are not approved.
- The current route work and package-local documentation may justify retaining nearby guidance; its relocation is a planning question, not a predetermined rename.
- Possible `app/`, `automation/`, `services/`, or `models/` boundaries need a concrete dependency and compatibility review before selecting a layout.

## Constraints

- Preserve [project policy](../../project/policy.md) and unrelated worktree changes.
- Draft candidates require human scope approval before feature execution.
- Foundation and downstream product outcomes must be separated when planning reveals a dependency.

## Acceptance Envelope

The human owner accepts bounded scope, exclusions, dependency meaning, compatibility obligations, and evidence requirements before any feature planning or implementation handoff.

## Candidate Features

- A neutral shared-run contract, if current consumers justify that boundary.
- A configuration/runtime ownership contract, if one stable prerequisite can be isolated.
- Route-maintenance discoverability, if a concrete navigation problem remains after current link corrections.

These are unapproved candidates; no feature documents or execution artifacts exist for them.

## Continuity Notes

- `2026-10-04`: scope remains `draft`; module boundaries, compatibility, sequencing, and acceptance evidence require human planning review.
