# Compiler change workflow

Use this for changes to parsing, checking, lowering, code generation and
their IDE/runtime callers. Select checks from the
[validation matrix](validation.md); do not stop at a Rust unit test when the
reported behavior happens through the CLI, LSP or browser.

## 1. Establish the contract and scope

- Inspect the branch, working tree and relevant repository instructions.
  Do not change another task's files or switch a worktree under its owner.
- Write down the input, entry point, current result and expected result.
  Derive the expectation from the specification, existing public contract
  or explicit requirement. If these conflict, resolve that before coding.
- Trace the real call path. Include dependencies across syntax, types, IR,
  model, backend, runtime and IDE only where the behavior reaches them.
- For shared work, agree on owners, allowed files and interface changes.
  Keep a bounded task inline rather than automatically spawning agents.

## 2. Reproduce and pin the behavior

- Run the smallest example through the entry point where it was reported.
- Add a regression test to the existing harness and confirm that it fails
  for the intended reason before the fix. For a new feature, pin the missing
  behavior. A refactor needs an existing behavioral baseline.
- Add a nearby accepted case and relevant rejected/boundary cases so a fix
  cannot pass by rejecting everything or suppressing a diagnostic.
- Preserve expected diagnostic codes, source spans and safe fixes. Use an
  annotated fixture, not just a Markdown `flick fail` block, for a specific error.

## 3. Implement across the affected layers

- Fix the layer that owns the rule, using information its current callers
  already provide. Do not special-case a fixture or repair generated output.
- Match surrounding conventions. Prefer existing helpers and dependencies.
- For syntax changes, account for formatting, editor grammars, IDE spans,
  safe migration fixes, the source corpus and both documentation locales.
  Use safe fixes where possible; inspect their diff rather than blindly
  rewriting fixtures with location-sensitive annotations.
- For semantic changes, account for the reference model and formal contract.
  Update affected proofs, or explain why the existing contract is unchanged.
- For cache changes, test both reuse and invalidation after an input changes.
  For editor changes, test an edit after the initial successful analysis.

## 4. Validate what will ship

- Rerun the original reproduction and regression test after the final edit.
- Run every applicable matrix row, then the broader checks it requires.
  Execution changes must exercise emitted programs, not just IR snapshots.
- Check debug/release and supported target behavior where affected. For
  cancellation/resources, cover partial completion and overlapping work.
- Measure performance/resource changes through real entry points and fresh
  processes. Keep existing budgets; report baseline and final measurements.
- Review the complete diff, snapshots and generated files for accidental
  changes. Run `git diff --check`; leave unrelated work intact.
- Do not commit or perform outward-facing actions unless asked.

## 5. Report or hand off

Keep the report short but specific. A blocked check stays blocked.

```text
Task:
Changes (files and behavior):
Affected guarantees (including model/proof impact):
Checks (exact commands and results):
Not checked (reason and required follow-up):
Remaining risks:
```

For a handoff, also name the worktree/branch, owned files and unfinished work.
Do not report a phase complete when its required checks are still pending.
