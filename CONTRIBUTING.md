# Contributing

## Repository responsibilities

`ternary-contours` is the authoritative numerical crate. Numerical milestones must be developed, reviewed, and merged here before any companion work begins in `plotters-ternary`.

`plotters-ternary` owns rendering integration only: chart projection, display clipping, styling, labels, legends, and backend output. Do not open a parallel Plotters pull request merely to follow an in-progress numerical branch.

## Milestone workflow

1. Create one feature branch from the latest `ternary-contours/master`.
2. Keep the milestone confined to `ternary-contours` unless an integration change is impossible to defer.
3. Rebase the feature branch onto `master` before opening or updating the pull request. Do not merge `master` into the feature branch.
4. Merge the core pull request first.
5. After the core merge, inspect `plotters-ternary` against the released or merged core API.
6. Open a separate Plotters pull request only for required rendering integration, dependency metadata, re-exports, examples, or documentation.
7. Prefer squash merge for milestone branches that have accumulated corrective commits. Use rebase merge only when the branch history is already linear.

## Conflict prevention

- Never maintain matching long-lived milestone branches in both repositories.
- Never make a Plotters branch depend on an unmerged core branch unless performing a disposable local integration test.
- Before pushing milestone updates, fetch the latest `master` and rebase locally.
- If GitHub reports that a branch cannot be rebased because it contains a merge commit, reconstruct or squash the branch rather than merging `master` into it again.

## Validation boundary

Core numerical tests belong in `ternary-contours`. Once the core milestone is merged, run `plotters-ternary` regression tests against the updated dependency before deciding whether a renderer change is needed.

## Adding a GUI element

Viewer controls are contract-driven. A new button, selector, edit box, panel,
canvas interaction, dialog, table, menu item, shortcut, or status indicator is
incomplete until it has:

- a stable `UiElementId` and one registry entry;
- a typed `UiAction`, reducer transition, declared effects, and invalidation;
- public-state rationale and hazard documentation;
- an explicit layout and overflow policy;
- a contract-aware widget wrapper and behavioral tests; and
- regenerated `docs/gui` inventories.

Run the checked-in documentation guard before submitting a viewer change:

```text
cargo run -p ternary-contours-gui-core --bin generate-gui-contract-docs -- --check
```
