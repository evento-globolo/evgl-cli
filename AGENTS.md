# AGENTS.md

Owner: `evento-globolo`  
Tracking: `DEN-1889`

Use focused pull requests, preserve interface compatibility, add tests with behavior changes, never commit credentials or customer data, and resolve conflicts semantically using both sides and relevant history.

## Code style and coding patterns

remember to modularize the rust, typescript and dart - not everything belongs in main.rs, main.ts and main.dart; also follow functional coding principles - fewer side-effects (use pure functions more), more immutability (immutable variables); but for stateful apps like the client or stateful servers like websockets or tcp connections, sometimes classes and oop make more sense than functional programming perse, but we can still adhere to functional programming more than usual. Favor exhaustive pattern matching and use formal methods checking too. Favor composability and re-use , so basically create more utility functions and routines for shared use. You can follow a medium level of D.R.Y. (don't repeat yourself) - in other words you can repeat yourself at medium amount (not too much not too little). Some chaining is totally fine, so either method-chaining (immutable sometimes although with classes can be mutable too for performance), and chaining via the pipe operator is ok in languages like gleamlang.

Functional programming is mostly the following:

+ explicit inputs
+ explicit outputs
+ immutable values
+ pure transformations
+ typed errors
+ explicit state transitions
+ composition
+ effects pushed outward
+ illegal states excluded by types

## Functional programming conformance

This repository carries an FP conformance ratchet. Before you land a change:

```sh
python3 tools/fp-conformance/fp_conformance.py .
```

CI compares your findings against `tools/fp-conformance/budget.json` and fails
only when a rule's count *increases*. Do not raise the budget to get green — fix
the new violations. When you clear a class of violation, lower the budget in the
same commit with `--write-budget`.

The principles, the rule codes and the remedy for each are in `FP-GUIDELINES.md`.

## Repository-local Git worktrees

- Create or use a Git worktree only when the human operator explicitly authorizes it for the current task. Concurrency or a dirty checkout is not permission by itself.
- Put every authorized worktree at `<repository-root>/tmp/worktrees/<name>`; from the repository root, use `./tmp/worktrees/<name>`. Never place worktrees beside repositories or organization directories.
- Keep `tmp`, `temp`, `tmp/worktrees`, and `temp/worktrees` ignored in the repository-root `.gitignore`. Do not commit files from those directories.
- Relocate or remove a worktree only when the operator explicitly requests it. Before removal, preserve and publish intended changes, verify its commit is represented on the target branch, and confirm there are no tracked, untracked, ignored-sensitive, or in-use files that must survive. Remove it with `git worktree remove <path>` without `--force`; never delete a worktree directory with `rm`.
