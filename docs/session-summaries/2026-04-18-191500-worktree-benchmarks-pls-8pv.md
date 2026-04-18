# Session Summary: Worktree Benchmarks (P8 pls, P11 8pv)

## Summary

Benchmarked two VRPTW optimization techniques in isolated worktrees: ensemble destroy operators (P8/pls) and multi-start diverse construction (P11/8pv). Also completed O(1) relocate move evaluation (xt4.6) in worktree wt18.

## Completed Work

- **xt4.6 (wt18):** O(1) relocate move evaluation with RouteMeta scaffolding — commit `f3bd600` on branch `tmux18-xt4-6`
- **P8 (pls):** Ensemble destroy operator mixing — commit `34a474d` on branch `tig-swarm-demo-pls/benchmark`
  - Score: 7410.25 (baseline 7461.79, delta -51.54, **-0.69% improvement**)
  - Vehicles: 646 (baseline 662, delta -16)
- **P11 (8pv):** Multi-start diverse construction — commit `628fce4` on branch `tig-swarm-demo-8pv/benchmark`
  - Score: 7512.08 (baseline 7460.79, delta +51.29, **+0.69% regression**)
  - Vehicles: 640 (baseline 662, delta -22)

## Key Changes

- `tig-swarm-demo-pls/src/vehicle_routing/algorithm/mod.rs` — Added `ensemble_removal()` as 5th destroy operator mixing 5 criteria per step
- `tig-swarm-demo-8pv/src/vehicle_routing/algorithm/mod.rs` — Added 3 construction heuristics (tight-TW NN, random-seed NN, Clarke-Wright savings) with best-of selection
- `tig-swarm-demo-wt18/src/vehicle_routing/algorithm/mod.rs` — Added `RouteMeta` struct with O(1) feasibility checks, `try_relocate_fast()`, 6 unit tests

## Pending/Blocked

- **P8 (pls)** is a candidate for merging to main (score improved)
- **P11 (8pv)** regressed — penalized-cost selector biases toward fleet consolidation at expense of distance; needs tuning before merge
- **xt4.6 (wt18)** needs integration testing with full benchmark before merge

## Next Session Context

- Consider merging P8 (pls) ensemble destroy into main since it showed improvement
- For P11 (8pv), try adjusting the fleet penalty weight or using distance-only selection to see if multi-start construction can improve without regression
- RouteMeta (xt4.6) provides foundation for O(1) evaluation of other move types (exchange, or-opt) in future work
