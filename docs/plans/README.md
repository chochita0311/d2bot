# Planning And Execution Artifacts

Use the [shared workflow](../agents/flows/workflow.md) for sequence and approval boundaries, [planning management](../policies/harness/prd-feature-management.md) for scope/status, and the [runner](../agents/operations/runner.md) to start or resume an approved feature. These are the installed harness's owners of the process.

## Artifact Owners

| Directory | Content | Template |
| --- | --- | --- |
| `prd/` | Bounded scope, exclusions, uncertainty, and acceptance envelope | [PRD](prd/template-prd.md) |
| `feature/` | One loop-sized product or foundation boundary | [Feature](feature/template-feature.md) |
| `spec/` | Implementation-facing execution contract | [Spec](spec/template-spec.md) |
| `run/` | Current execution pass, attempts, routing, and review outcome | [Run](run/template-run.md) |
| `evaluation/` | Evaluator result and evidence coverage | [Evaluation](evaluation/template-evaluation.md) |
| `fix/` | Targeted corrections tied to evaluator findings | [Fix log](fix/template-fix-log.md) |
| `heuristic/` | Non-blocking interaction suggestions | [Heuristic backlog](heuristic/template-heuristic-backlog.md) |

For `docs-content`, an adequate [compact writing contract](../policies/harness/execution-loop-governance.md#docs-content-writing-contract) may be designated as the active spec. Record its real ID and file/section locator in the run and downstream artifacts.

## Current Work

- [Documentation conformance PRD](prd/prd-0001-harness-documentation-conformance.md), [feature](feature/feat-0001-harness-documentation-ownership.md), and [run](run/run-20261004-01-harness-documentation-conformance.md): canonical installation and documentation ownership; see the run for validation and review state.
- [Package-boundary PRD](prd/prd-0002-package-boundary-planning.md): draft direction for shared-run, configuration, and runtime ownership; feature execution requires separate approval.

## Reference Ownership

Project policy, architecture, developer guidance, config references, and domain documentation retain their local owners. The harness manifest tracks only shared-origin assets, not these product/planning records.
