# EVAL-0016: Rust 설정 문서·선택·보정 계약

## Metadata
- Run: [RUN-20261022-02](../run/run-20261022-02-rust-config-contract.md); Attempt: `1`.
- Feature: [FEAT-0017](../feature/feat-0017-rust-config-contract.md); Spec: [SPEC-0016](../spec/spec-0016-rust-config-contract.md); Parent: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Date: `2026-10-22`.

## Contract
- Result: `PASS`; Evidence Coverage: `complete` for SPEC-0016; Acceptance impact: `not applicable`.
- JSON objects retain insertion order and arbitrary-precision number text through serde_json1.0.154. Domain defaults/builders, recursive merge/BOM/Windows file order, character fallback/preferred profile and shared registered farm lookup are implemented. Typed execution adapters remain with their consumers.
- Config/schema validation preserves inspected Python error categories/messages and stage ordering, including large integer FCR values, bool/int distinction, casting catalog and survival dependencies.
- Approved delta: invalid optional buff pause values are removed with JSON-pointer/original-value/action warnings. Missing-key consumer defaults remain authoritative; valid values remain unchanged and files are not rewritten. This differs intentionally from Python's acceptance/failure for those malformed values.
- Source uses safe ownership, immutable warning slices and no process exits/FFI. Farm references are looked up in the owned profile map. Derived farm/active state is produced by snapshot; active key is exposed through its explicit accessor.

## Functional
- Result: `PASS`; Evidence Coverage: `complete` for SPEC-0016; Acceptance impact: `not applicable`.
- Rust fmt/check/clippy with warnings denied, all-targets locked tests and release build passed. **14 tests passed**: bootstrap4 and config10.
- Config tests compare committed directory/single-file settings, defaults, weak legacy fields, merge/order/BOM, selection/alias, policy rejections, huge integers, malformed containers, parse/read errors and validation-stage priorities against the existing Python implementation.
- Repair tests verify NaN/±Infinity/overflow, negative/unparseable/null/container pause values, duplicate lowercase keys, warning pointer escaping, valid override preservation and absence of warnings in current committed settings. The repaired snapshot is compared with the Python snapshot for equivalent valid/missing overrides.
- Python oracle Black and in-memory compile checks passed. Existing full Python baseline765 tests remains the build run's evidence; this packet did not rerun it or alter Python runtime.

## Evidence Limits And Follow-Up
No GUI warnings, real waits, screen/FFI/runtime state or game execution were tested. JSON syntax outside the spec and arbitrary Python objects are not parity claims. Overall port parity stays Unknown/Provisional. Later adapters must display warnings and check duration/deadline conversion rather than assuming any finite float fits an OS wait. No task scratch remains; test fixtures clean their exact owned Cargo tmp directories after oracle exit. Standard Cargo target remains for ongoing builds.
