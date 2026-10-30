# Branches

This fork keeps two lines of work.

| Branch | What it is |
|---|---|
| `upstream-main` | An exact mirror of `eriknihlen/openac-rust` `main`. Only ever fast-forwarded; never committed to. |
| `main` | Our client: upstream plus our changes, including local-only ones (the port tracker, ignore rules) and work waiting on upstream review. |

## Merge direction

Merges only flow down: `upstream-main` → `main`. Never merge `main` into `upstream-main`.
Use merges, not cherry-picks, so `git log main ^upstream-main` shows exactly what we carry that
upstream does not have.

## Where a change goes

- **Anything meant for Erik** (features, fixes, items in `docs/csharp-fork-port-tracker.md`):
  branch from `upstream-main`, open the upstream PR from that branch, and merge the same branch
  into `main`. Never cut a PR branch from `main`, or our local-only commits ride along.
- **Local-only changes** (tooling, notes, ignore rules): commit on `main`.
- Large features (the Modern HUD decks) go upstream as several reviewable PRs, each its own branch
  off `upstream-main`.

## Syncing upstream

1. `git fetch upstream`, fast-forward `upstream-main` to `upstream/main`
   (`git switch upstream-main && git merge --ff-only upstream/main`), and push it to `origin`.
2. Merge `upstream-main` into `main`, resolve, build and test; push.
3. Once an upstream PR is merged, its branch can be deleted; `main` already has its commits.

## Rules that stay from the C# fork

- One client, no separate branding. Every non-retail feature sits behind an option that is off
  by default.
- `ac-client-src/` (the decompiled retail client reference) is local only and never committed.
