# Upstream steering brief — 2026-04-18

Our swarm at `tigswarmdemo.com` is stuck at score **~7341**.
The upstream at `demo.discoveryatscale.com` reached **6861** (rank 1, silver-nova).

## Top 10 ideas that actually worked upstream (by agent)

1. **Route struct with `arr` / `lat` arrays** (fierce-owl)
   Cache arrival times and latest-feasible times. Gives **O(1) insertion feasibility** via `try_insert`. Biggest structural win.

2. **Neighbor-guided LS with K=40** (silver-nova, fierce-owl, primal-viper)
   Precompute each customer's 40 nearest neighbors from the distance matrix.
   Restrict *every* operator (relocate, or-opt, 2-opt, 2-opt*, swap*) to those candidates. Changes O(n²) passes to O(n·K).

3. **Randomized VND (RVND)** (noble-wolf, fierce-owl)
   Instead of fixed operator order, shuffle 7 operators each pass, break on first improving move. Escapes attractor basins the deterministic order falls into.

4. **ILS with full VND after perturbation** (fierce-owl from noble-wolf)
   Ruin-recreate + run VND to fixed point each iteration. Simpler than ALNS+SA, converges deeper.

5. **2-opt*** (cross-route tail swap) (silver-nova, sharp-orbit, fierce-owl)
   Swap suffixes of two routes at a cut point. Capacity + TW recomputed from new suffixes. Single-operator "major improvement".

6. **Multi-start construction** (fierce-owl, noble-wolf)
   Pick best of {Solomon insertion, deterministic regret-2, 2-3 noisy regret-2 variants}.

7. **Route-destruction destroy op** (sharp-orbit)
   Third destroy strategy alongside random and Shaw: pick one of the smallest-third of routes, remove ALL customers, greedy re-insert. Forces route count to contract.

8. **Shaw's related removal** (sharp-orbit)
   Second destroy operator: pick a seed customer, repeatedly remove most-related (distance 9, TW 3, demand 2 weights). Rank-biased pick (u^6).

9. **Variable-intensity ILS perturbation** (sharp-orbit)
   Destroy fraction ∈ [8%, 25%] (not fixed 15%). Scale destroy up 5% per stuck iter, capped at 20.

10. **Exponential SA cooling** (silver-nova)
    T0 = 1.2% of cost, T_end = 0.1%. Replaces crude acceptance; wider exploration without over-accepting late.

## Reference code available locally

- `/tmp/tig-ideas/silver-nova.rs` (711 lines, score **6861**) — rank 1
- `/tmp/tig-ideas/primal-viper.rs` (1364 lines, score **6882**) — rank 2
- `/tmp/tig-ideas/sharp-orbit.rs` (925 lines, score **6903**) — rank 3

## Rules of engagement

- These are **inspiration only** — don't copy wholesale. Adapt the ideas.
- Don't repeat ideas already in `recent_hypotheses`. Always check before editing.
- Priority order: (1) → (2) → (3) are the structural wins; start there if our code doesn't have them.
