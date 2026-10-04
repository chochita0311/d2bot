# FEAT-0001: Harness Documentation Ownership

## Metadata

- ID: `feat-0001`
- Status: `passed`
- Type: `foundation`
- Surface: `docs`
- Execution Profile: `docs-content`
- Required Evaluators: `contract`, `functional`
- Parent PRD: [prd-0001](../prd/prd-0001-harness-documentation-conformance.md)
- Created: `2026-10-04`
- Updated: `2026-10-04`
- Boundary approval: the user's explicit request for the canonical installation and current documentation owners.

## Goal

Make the shared harness the single owner of planning/execution rules, with current operational guidance in its feature/development owners and unapproved package direction in a draft PRD.

## Acceptance Contract

- Entrance/developer guidance routes planning and execution through the shared policies and canonical artifact directories.
- The maintained tree uses canonical artifacts and current references.
- Useful operational guidance has a feature or development owner and active links resolve.
- Managed shared bytes and manifest remain unchanged.

## Scope Boundary

- In: local documentation routes, current operational references, draft package planning, and this run's artifacts.
- Out: code, settings, shared policy specialization, personal configuration, and unapproved package implementation.

## Contract Surfaces

`AGENTS.md`, local developer/harness guides, feature-maintenance references, canonical planning artifacts, and the import manifest's managed file set.

## Dependencies

- The installed shared harness is verified against its committed baseline.
- Any file removal must be confined to the approved documentation boundary and exclude active concurrent work.

## Writing Contract

- ID: `spec-0001`
- Form: the compact writing contract permitted by [execution governance](../../policies/harness/execution-loop-governance.md#docs-content-writing-contract).
- Authority: the parent PRD and installed shared policies. Preserve project policy and all unrelated code/config changes.
- Permitted changes: consolidate useful maintenance guidance into current references; keep package questions in a draft PRD; align documentation owners, links, and indexes; commit/push the reviewed session-owned subset under the parent's publication boundary.
- Preservation: retain current implementation meaning and unrelated changes. Shared assets and their manifest are fixed baselines. Do not invent approval states or remove a file while another task is writing it.
- `contract` checks: source fidelity, canonical owners, real current approval provenance, and unchanged managed hashes.
- `functional` checks: useful-guidance coverage, local link/anchor resolution, navigation, and whitespace. Runtime/game behavior is outside this contract.

## Pass Or Fail Checks

- The document tree follows the installed artifact owners and shared lifecycle.
- Reusable gem, waypoint, GUI, and interpreter-diagnostic guidance is discoverable in current references.
- Removed document paths have no remaining references in maintained Markdown.
- The pending package scope is represented by a `draft` PRD, with no approved feature or executable spec.
- Every managed target still matches its recorded rendered hash and source provenance.
- Current Markdown links and anchors resolve, excluding literal template/code examples.

## Regression Surfaces

User onboarding, project/config references, shared harness content, and preexisting code changes.

## Harness Trace

- Active Spec: `spec-0001` — [Writing Contract](#writing-contract)
- Active run: [run-20261004-01](../run/run-20261004-01-harness-documentation-conformance.md)
- Execution profile: `docs-content`
- Evaluator reports: [contract](../evaluation/eval-0001-contract-harness-documentation.md), [functional](../evaluation/eval-0002-functional-harness-documentation.md)

## Continuity Notes

- `2026-10-04`: the approved boundary is canonical documentation ownership and current references. The run reports contract/navigation evidence; package implementation requires separate approval.
- `2026-10-04`: both required document evaluations passed with complete evidence for this boundary. The user authorized publication after whole-repository structure/composition review; preexisting unrelated work remains excluded.
