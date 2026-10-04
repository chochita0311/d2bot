# Local Agent Harness

This directory contains the repository-owned harness for planning, implementation, and review. Start with [project development guidance](../project/developer-guide.md) and [project policy](../project/policy.md); those documents own this application's implementation rules and technical boundaries.

## Planning And Execution

| Request | Working route |
| --- | --- |
| A PRD or feature-planning request | Use the [planning workflow](flows/workflow.md) and [planning policy](../policies/harness/prd-feature-management.md). Stop at the requested planning/review boundary. |
| Start or resume an approved feature run | Use the [runner](operations/runner.md), begin with [Orchestrator](roles/orchestrator.md), and continue from the current run record. |
| Document work within an approved feature | Select `docs-content`; designate the [compact writing contract](../policies/harness/execution-loop-governance.md#docs-content-writing-contract) before updating or evaluating documents. |

The [workflow](flows/workflow.md#workflow-assembly) is composable: select the roles and evaluators required by the approved boundary. The installed shared policies own readiness, approval, execution, and return rules. Use their [canonical artifacts and templates](../plans/README.md).

## Document Owners

| Owner | Content |
| --- | --- |
| [README](../../README.md) | User setup and current usage |
| [Project docs](../project/) | Product policy, architecture, roadmap, and developer/GUI maintenance |
| [Config guide](../../config/config.md) | Configuration navigation; each config subfolder owns its field reference |
| `roles/`, `flows/`, `operations/`, [profiles](profiles/README.md) | Shared role contracts, sequence, invocation, and loop presets |
| [Harness policies](../policies/harness/) | Planning, execution, traceability, delegation, and operator continuity |
| [Planning artifacts](../plans/README.md) | PRD, feature, spec, run, evaluation, fix, and heuristic documents and templates |
| [Design review](../policies/design/design-evaluation.md), [interaction review](../policies/experience/interaction-evaluation.md) | Reusable checks selected for the actual approved surface |
| [Import manifest](harness-import-manifest.json) | Shared-file provenance and refresh baselines |

The PRD/feature owns scope and the run owns active execution state. Evaluations and fix logs own their respective evidence and corrections. Template examples are placeholders; replace their IDs and links when creating real artifacts. Put reusable operating guidance in the relevant project or feature reference.

## Profiles In This Project

Choose the smallest fitting profile through [execution-profile policy](../policies/harness/execution-profiles.md). Local surface meanings are:

- `frontend-product`: Tkinter windows, panels, visible controls, and interaction. Use desktop screenshots and direct UI observation for affected states; browser evidence applies only when a browser surface exists.
- `backend-product`: Python worker behavior, vision decisions, action orchestration, and CLI behavior.
- `foundation-contract`: config models, package ownership, route interfaces, and other shared contracts.
- `infra-devtool`: environment, tooling, and operational developer workflows.
- `docs-content`: documentation ownership, source fidelity, and navigation.
- `fullstack-product`: one approved feature that must coordinate coupled GUI and runtime surfaces. Use explicit lanes only when needed.

Keep desktop implementation details in project guidance. Optional browser tools, skills, model bindings, and personal Codex adapters are not prerequisites for this repository. Role documents describe responsibilities; they do not install or require separate runtime agents.

Apply [operator continuity](../policies/harness/operator-briefing-and-review-receipts.md) only when prior context or a meaningful change needs explanation. Its [template](templates/operator-briefing.md) is a response scaffold, not a permanent briefing log.

## Import And Refresh

The shared source is the `agents` package in the `ai-assets` repository. Resolve that checkout explicitly for each maintenance session. Normal project use reads only these local copies; the manifest records logical source paths and a full committed revision.

This installation includes portable roles, flows, operations, profiles, templates, harness policies, and design/interaction review assets. Personal `adapters/codex/` assets and the source adoption guide/example are not installed as project policy. Local entrance and project documents remain outside the manifest.

Mapping version `1` renders committed UTF-8 Git blobs with LF line endings. The repository's `.gitattributes` preserves those bytes across Windows checkouts so line-ending conversion does not appear as local drift:

- Roles, flows, operations, and profiles map into the matching directory under `docs/agents/`.
- Product/execution templates map into `docs/plans/<kind>/template-<name>.md`; the operator template maps into `docs/agents/templates/`.
- Harness policies map into `docs/policies/harness/`; review policies map into `docs/policies/design/` and `docs/policies/experience/`.
- Rewrite relative policy and operator-template links for those owners, preserving anchors. The design review's sibling interaction link becomes `../experience/interaction-evaluation.md`.

For a refresh:

1. Read the source package's `ADOPTION-GUIDE.md` and validate the installed manifest before changing shared content. A missing manifest with existing harness files requires bootstrap; an invalid one requires recovery from supported evidence.
2. Verify each `managed` target against its recorded rendered hash. Classify local differences before replacement; preserve `local` entries and their detachment baselines.
3. Render the selected committed source into the local layout. Keep product docs, local guidance, existing plans, and run artifacts intact. Review previously tracked files before retiring them.
4. Check entrance references, local links and anchors, selected-file completeness, unique ordered mappings, committed provenance, and source/rendered hashes. Treat template placeholders as examples.
5. Generate the candidate manifest from the resolved files. Replace the installed manifest only after all checks succeed. Keep the shared layer and manifest together in a focused change.

Update general reusable rules upstream before refreshing them here. Put intentional project specialization in local guidance; if a shared file must diverge, record an explicit ownership decision instead of silently changing its baseline hash.
