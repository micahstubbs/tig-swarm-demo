# xt4.8 SISR Keep/Revert Handoff

## Question

Decide whether `d2e08d4` (`Benchmark: SISR string removal destroy operator (xt4.8)`) should stay on `main` or be reverted.

## Current State

- `d2e08d4` is already on `main`.
- Worker-session result in isolated worktree `tig-swarm-demo-xt4.8/benchmark`:
  - score `7363.291666666667`
  - feasible `true`
  - instances feasible `24/24`
  - vehicles `640`
  - recorded in [docs/benchmark-results/tig-swarm-demo-xt4.8.json](/home/m/wk/tig-swarm-demo/docs/benchmark-results/tig-swarm-demo-xt4.8.json)
- After cherry-picking onto `main`, local verification succeeded:
  - `cargo build -r --bin tig_solver --features solver,vehicle_routing`
  - `cargo test --features vehicle_routing`
- But a benchmark run on `main` after merge produced:
  - score `7497.583333333333`
  - feasible `true`
  - instances feasible `24/24`
  - vehicles `657`

## Why This Is Unresolved

The worker branch result says `xt4.8` is a modest improvement. The post-merge run on `main` was materially worse than that result.

Possible explanations:

- benchmark variance/noise
- comparison against the wrong baseline commit
- interaction with already-merged solver changes
- a measurement mistake in one of the runs

There is already repo-local evidence that single benchmark runs are not reliable enough for small deltas:

- `8229e56` documents benchmark noise swamping single-run optimizer deltas
- `c85a972` documents that cached baselines must match the current commit

## Evidence To Trust

- Trust the committed artifact from `tig-swarm-demo-xt4.8/benchmark`:
  - `7de85b4`
  - benchmark artifact in `docs/benchmark-results/tig-swarm-demo-xt4.8.json`
- Trust the fact that `d2e08d4` builds and tests on `main`
- Trust that the decision is still open

## Evidence Not To Trust

Do not use any comparison that:

- reuses a baseline from a different commit
- relies on a single run to justify keep/revert
- comes from the aborted temp-worktree comparison attempt that produced:
  - score `48871.583333333336`
  - feasible `false`
  - solved `23`

That result came from a bad comparison flow and is not decision-grade evidence.

## Required Investigation

Run a controlled A/B comparison between:

1. current `main`
2. `main` with `d2e08d4` reverted

Requirements:

- use isolated worktrees
- use the same benchmark harness on both arms
- collect at least `3` runs per arm
- record score, feasibility, and vehicle count for each run
- compute mean/min/max for each arm
- state whether the delta is larger than expected noise

## Decision Rule

Keep `d2e08d4` on `main` if the repeated comparison shows a real improvement or the result is inconclusive within noise.

Revert `d2e08d4` only if repeated runs show a clear regression for the SISR arm relative to the reverted arm.

## Deliverables

- a short markdown report in `docs/investigations/`
- benchmark artifacts or per-run summary data
- a clear recommendation:
  - keep
  - revert
  - inconclusive, rerun later

If the recommendation is `revert`, include the exact revert command or commit.
