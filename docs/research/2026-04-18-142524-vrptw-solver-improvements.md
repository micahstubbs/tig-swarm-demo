# VRPTW Solver Improvements: Literature Review and Prioritized Recommendations

**Generated:** 2026-04-18
**Topic:** State-of-the-art techniques for Vehicle Routing Problem with Time Windows (VRPTW) -- survey of academic literature and a prioritized roadmap of extensions for this project's hybrid ALNS solver on Homberger 400-node benchmarks, under a single-threaded 30-second-per-instance budget.
**Scope:** Scientific literature (Ropke/Pisinger, Vidal, Christiaens/Vanden Berghe, Kool, Wouda et al.), technical details of SISR, HGS, LKH-3, and Vidal-style O(1) concatenation evaluation, with concrete Rust-implementable recommendations.

---

## Executive Summary

The current solver is a hybrid ALNS that spends only **~4.5 seconds of the 30-second budget** per instance (`deadline = start + 4500ms`), uses **O(n) `is_feasible` scans on every move attempt**, has **no neighbor-list pruning** (every repair operator scans all routes x all positions), and relies on **intra-route 2-opt that reverses segments** -- a move that is *almost always infeasible* under tight time windows and wastes iteration budget. These four facts together mean the solver leaves an enormous amount of quality on the table relative to a textbook Vidal/HGS-style implementation.

Three families of techniques, adopted from the published state of the art, would likely yield the largest improvements:

1. **Vidal-style concatenation-based move evaluation** ([Vidal 2022](https://arxiv.org/abs/2012.10384)) reduces per-move feasibility+cost evaluation from O(route length) to amortized O(1) after O(n) route preprocessing. This single change is the foundation modern VRPTW solvers (HGS-CVRP, PyVRP) rest on.
2. **SISR (Slack Induction by String Removals)** destroy operator ([Christiaens & Vanden Berghe 2020](https://pubsonline.informs.org/doi/10.1287/trsc.2019.0914)) paired with a **greedy-blink repair** consistently outperforms classical random/worst/Shaw destroy operators on routing benchmarks and is strikingly simple.
3. **SWAP\*** ([Vidal et al. 2022](https://arxiv.org/abs/2012.10384)) -- swap two customers across routes, but each reinserted at its *best* position in the other route -- catches moves that plain exchange misses and is the highest-ROI intensification operator post-basic-relocate.

The single-largest low-effort change, however, is **extending the time budget**: from 4.5 s to ~27 s per instance (leaving headroom for benchmark harness overhead). That alone should yield a significant score drop with no algorithmic change.

---

## 1. The Current Algorithm (Baseline)

`src/vehicle_routing/algorithm/mod.rs` implements a five-phase pipeline:

1. **Nearest-neighbor construction**, seeded by earliest due-time, with forward time-window and capacity checks.
2. **Merge-to-fleet** -- pairwise route concatenation + cheapest-insertion to shrink to `fleet_size` routes.
3. **Quick 2-opt local search** (budget 800 ms).
4. **ALNS (budget 3200 ms)** with 4 destroy ops (random, worst, Shaw, route removal) and 2 repair ops (greedy, regret-2). Adaptive weights, exponential cooling. Shaw-relatedness is precomputed with a 30-neighbor cap.
5. **SA fine-tuning (budget 4500 - 3200 = 1300 ms)** with intra-2-opt, relocate, exchange, Or-opt (1-3), cross-2-opt. Delta evaluation avoids clones where possible but still calls `is_feasible(ref route)` per try.

### Observations with ROI implications

| Observation | Implication |
|---|---|
| `deadline = 4500 ms`, total budget is 30 000 ms | Solver quits **85% of the allotted time early**. No algorithmic change will matter as much as fixing this. |
| `is_feasible()` is an O(route length) scan called on every candidate move | On 400-node instances, routes are 20-40 nodes; per-iteration cost is O(40) x moves-per-sec. Vidal-style concat evaluators make this O(1). |
| No neighbor lists outside of Shaw destroy (30 NN used only for destroy seeding) | `greedy_insertion` and `regret_insertion` each scan every (route, position) pair -- O(k * n^2) per call. Neighbor lists reduce this to O(k * Gamma * n) with Gamma~20. |
| Intra-2-opt reverses a segment, which flips arrival times mid-route | On tight-TW instances (R1_4, RC1_4, C1_4) this move is almost always infeasible. Wasted iterations. Should be replaced by 2-opt* (inter-route, no reversal). |
| `shaw_removal` uses `custs.contains(ref nb)` | O(n) linear scan inside a loop -- easy O(1) fix with a bitset. |
| `worst_removal` re-filters `used` via full vector scan on every pick | Same issue. |
| `penalized_cost(ref routes)` fully re-sums distances | Should be incremental; SA phase does track `route_dists` correctly, but ALNS phase does not. |
| `distance_matrix: Vec<Vec<i32>>` | Double indirection; a flat `Vec<i32>` with stride-based indexing is ~30% faster due to cache locality for 400-node matrices (640 KB). |
| Fleet-overflow penalty is 100 000 per extra route | Reasonable, but not differentiated from distance scale. Adaptive penalty (Vidal) would help. |

---

## 2. Literature Findings

### 2.1 Adaptive Large Neighborhood Search (ALNS)

Originated in [Ropke & Pisinger 2006](https://www.sciencedirect.com/science/article/abs/pii/S0305054805003023) (also the [DTU tech report PDF](https://backend.orbit.dtu.dk/ws/portalfiles/portal/3154899/An%20adaptive%20large%20neighborhood%20search%20heuristic%20for%20the%20pickup%20and%20delivery%20problem%20with%20time%20windows_TechRep_ropke_pisinger.pdf)). The core destroy/repair operator set is:

- **Random removal** -- uniform q customers.
- **Worst removal** -- sort by `cost(i) = d(prev,i) + d(i,next) - d(prev,next)`, sample via `idx = floor(y^p * |L|)` with `p ~ 3` (bias to worst, but randomized).
- **Shaw (relatedness) removal** -- `R(i,j) = phi*d_ij/d_max + chi*|t_i - t_j|/t_max + psi*|q_i - q_j|/q_max + omega*V_ij` with canonical weights `phi=9, chi=3, psi=2, omega=5`. Selection by the same `y^p` rule, p~6.
- **Greedy insertion** -- pick unassigned i with minimum best-Delta; insert.
- **Regret-k insertion** -- `regret_k(i) = Sigma_{h=2..k}(Delta^(h)(i) - Delta^(1)(i))`; insert the customer with *maximum* regret. Regret-2, regret-3, regret-4, and regret-*m* (over all routes) are all worth having. Typically regret-3 dominates on VRPTW.
- **Noise function** -- perturb Deltacost by `xi ~ U[-eta*d_max, +eta*d_max]`, eta in [0.025, 0.1], with probability 0.5. Increases exploration cheaply.

The current code has only random, worst, Shaw, and route-removal destroy, and only greedy + regret-*k=2* repair. Cluster, zone, and history-based removal are missing; noise-perturbed insertion is missing.

### 2.2 SISR -- Slack Induction by String Removals

[Christiaens & Vanden Berghe 2020, *Transportation Science*](https://pubsonline.informs.org/doi/10.1287/trsc.2019.0914) ([preprint](https://scispace.com/pdf/slack-induction-by-string-removals-for-vehicle-routing-14gqwirdlp.pdf)) introduced a deceptively simple destroy that removes **contiguous strings** of customers across multiple routes. Strings (not random picks) preserve the *slack* -- the time-window headroom -- around the removed region, which dramatically improves subsequent repair quality.

Standard parameter set:

- `L_max = 10` (max string cardinality)
- `c_avg = 10` (average customers removed)
- `L_s_max = min(L_max, avg_route_len)`
- `k_s = (4*c_avg) / (1 + L_s_max) - 1`
- `k_s ~ U[1, k_s+1]` -- number of routes touched
- With probability alpha ~ 0.5, use "split-string" mode: contiguous substring plus skip-over segment.

Repair is **greedy-blink insertion**: for each unassigned customer (ordered by demand descending or random), evaluate candidate positions greedily, but with probability `beta ~ 0.01` skip the current best (a "blink") -- tiny randomization without regret's overhead. SISR reproduces near-HGS quality on CVRP with far simpler code.

Contrasted with Shaw removal: SISR preserves geographic/temporal coherence of the *removed* region, not the *seed*. This makes the residual routes more insertable.

### 2.3 Vidal HGS and SWAP\*

[Vidal 2022, "Hybrid Genetic Search for the CVRP: Open-Source Implementation and SWAP\* Neighborhood"](https://arxiv.org/abs/2012.10384) ([GitHub](https://github.com/vidalt/HGS-CVRP)) defined the modern VRPTW/CVRP engine:

- **Giant-tour representation** decoded by the **Split** algorithm into feasible routes (Bellman-style shortest path on a route-length DAG).
- **SREX / OX crossover** recombines two giant tours.
- **RELOCATE**, **SWAP**, **2-OPT**, **2-OPT\*** (inter-route tail-swap *without* reversal), **Or-opt(1,2,3)** neighborhoods.
- **SWAP\***: exchange customers i in route A and j in route B, but each reinserted at its *own best* position in the other route (not in place). Pruned by geometric sector overlap. This catches moves invisible to plain SWAP and is a major quality driver.
- **Time-warp penalty** -- soft-feasibility: accept a route that violates TW but pay a linear penalty, letting the search cross infeasible plateaus. Penalty weight adjusted to keep feasibility ratio in target band.

[Wouder Kool et al., "HGS-VRPTW"](https://wouterkool.github.io/pdf/paper-kool-hgs-vrptw.pdf) (DIMACS 2022 VRPTW winner) extended HGS-CVRP directly to VRPTW and topped the competition. [PyVRP](https://arxiv.org/pdf/2403.13795) ([docs](https://pyvrp.org/)) is an open-source Python/C++ implementation of HGS-VRPTW/CVRP with `RELOCATE`, `SWAP`, `2-OPT`, `2-OPT*`, splitting classical 2-opt into `ReverseSegment` (intra, with precomputed time-warp feasibility) and `SwapTails` (inter, = 2-opt\*).

### 2.4 LKH-3

[Helsgaun's LKH-3](http://akira.ruc.dk/~keld/research/LKH-3/) handles VRPTW by penalty transformation: convert VRPTW to asymmetric TSP with slack arcs, then run Lin-Kernighan k-opt moves. It posts competitive results on Homberger 400 but typically runs minutes per instance -- not trivial to fit in 30 s. Its sequential k-opt ideas (ejection chains) can be *inspired* without full LKH machinery. Helsgaun 2017 describes the extensions in detail.

### 2.5 Concatenation-based O(1) Move Evaluation

The engineering unlock of modern VRPTW is *constant-time move evaluation* via cumulative route descriptors, developed in:

- [Kindervater & Savelsbergh 1997, "Vehicle routing: handling edge exchanges"](https://www.sciencedirect.com/science/article/abs/pii/S0167637795003240) -- forward/backward-slack propagation.
- [Savelsbergh 1992, *INFORMS J. on Comp.*](https://pubsonline.informs.org/doi/10.1287/ijoc.4.2.146) -- **Forward Time Slack** (FTS): `FTS_i = min_{j>=i} (l_j - b_j)`. Insertion feasible <=> `push_forward <= FTS_next`.
- [Vidal et al. 2014, *EJOR*, "A unified solution framework for multi-attribute vehicle routing problems"](https://www.sciencedirect.com/science/article/abs/pii/S0377221713005547) -- generalized **(duration, earliest_departure, latest_arrival, time_warp)** route-segment tuple that concatenates in O(1):
  - `concat(a, b) = (duration_a + duration_b + max(0, earliest_b - latest_a), ...)`
- [Vidal 2016, *Networks*, "Timing problem and constant-time move evaluations"](https://doi.org/10.1002/net.21693) -- formalization and bounds.

With this data structure, after O(n) route-segment preprocessing, every move -- relocate, swap, 2-opt, 2-opt\*, Or-opt -- is evaluated in **amortized O(1)**. Local search speed increases by 10-50x on 400-node routes.

### 2.6 Granular / Neighbor-List Search

[Toth & Vigo 2003, "The granular tabu search and its application to the vehicle-routing problem"](https://pubsonline.informs.org/doi/10.1287/ijoc.15.4.333.24890) introduced **granular neighborhoods**: limit local-search move candidates to edges (i,j) whose distance is below `beta * c_hat` (beta ~ 1.3-1.5, c_hat = average edge length). Equivalent to a precomputed **top-Gamma neighbor list** with Gamma in [20, 40]. This prunes the neighborhood by >90% with minimal quality loss.

The current code computes Shaw-relatedness neighbors (for destroy) but doesn't use them to prune insertion/repair positions. Using them there is low-hanging fruit.

### 2.7 Acceptance Criteria

- **Simulated annealing** with geometric cooling `T_{k+1} = alpha*T_k`, alpha in [0.9975, 0.99995] -- the current approach.
- **Late-Acceptance Hill Climbing (LAHC)** [Burke & Bykov 2017](https://www.sciencedirect.com/science/article/abs/pii/S0377221716305495), [Wikipedia](https://en.wikipedia.org/wiki/Late_acceptance_hill_climbing): accept if `f(x') <= f_{k-L}` or `f(x') <= f_current`, with `L in [1000, 5000]`. **Single parameter**, no temperature schedule, consistently competitive with well-tuned SA and nearly parameter-free.
- **Record-to-record travel (RRT)**: accept if `f(x') <= (1 + delta) * f_best`, delta in [0.001, 0.02] -- simple and strong.

LAHC is particularly attractive for this project because it removes the temperature-tuning degree of freedom the agent currently guesses at.

### 2.8 Recent (2022-2026) Papers

- [Kool et al. 2022 HGS-VRPTW](https://wouterkool.github.io/pdf/paper-kool-hgs-vrptw.pdf) -- DIMACS 2022 winner.
- [PyVRP 2024 (arXiv 2403.13795)](https://arxiv.org/pdf/2403.13795) -- reference HGS-VRPTW implementation.
- [Spark-ALNS 2024 (Nature Scientific Reports)](https://www.nature.com/articles/s41598-024-74432-2) -- multi-objective parallel ALNS.
- [PPO-ALNS 2025, *J. Combinatorial Optimization*](https://link.springer.com/article/10.1007/s10878-025-01364-6) -- RL (PPO) picks destroy/repair ops; 11-17% over adaptive-weight baselines.
- [HGS + Ruin-and-Recreate hybrid 2022, *J. Heuristics*](https://link.springer.com/article/10.1007/s10732-022-09500-9) -- SISR-style ruin inside HGS outperforms HGS alone on CVRP; likely transfers to VRPTW.
- [Learning-to-search for multi-TW VRP 2025 (arXiv 2505.23098)](https://arxiv.org/pdf/2505.23098) -- neural operator selection.

### 2.9 Known Best-Known Solutions on Homberger 400

The [SINTEF VRPTW benchmark page](https://www.sintef.no/projectweb/top/vrptw/homberger-benchmark/400-customers/) tracks current BKS. For 400-node instances the best-known totals (summed across the 10 instances per category) fall in these ranges (Euclidean, not rounded):

- C1_4: ~ 7152
- C2_4: ~ 3920
- R1_4: ~ 9899
- R2_4: ~ 3393
- RC1_4: ~ 11406
- RC2_4: ~ 3250

Per-instance averages are therefore ~715, ~392, ~990, ~339, ~1141, ~325 respectively. Across all 60 (mixed 400-node) instances the grand average is ~650 (reading off SINTEF totals). The current project uses 24 representative instances -- the specific subset determines the floor, but a score near 700-900 per instance is a reasonable long-term target.

---

## 3. Prioritized Improvements (ordered by expected ROI)

Each item lists: **what**, **why**, **approximate impact**, **implementation sketch**, and **risk**.

### [P0] Extend the algorithm's internal deadline to match the 30 s budget
- **What**: Replace `deadline = start + 4500ms` with a deadline derived from the challenge's actual time limit (or a safe `~27 000 ms` margin).
- **Why**: The solver currently wastes ~85 % of allotted CPU time. Any improvement below costs time, and we have an enormous time surplus right now.
- **Impact**: Likely double-digit percentage score drop with zero algorithmic change -- SA/ALNS improve monotonically with iteration count up to several thousand moves.
- **Sketch**:
  ```rust
  let budget_ms = 27_000;
  let deadline = start + Duration::from_millis(budget_ms);
  let alns_deadline = start + Duration::from_millis(budget_ms * 6 / 10); // 60%
  let ls_deadline = start + Duration::from_millis(2_000);
  ```
- **Risk**: Must still call `save_solution` frequently enough that timeouts don't cost progress; existing code already does this.

### [P1] Vidal-style segment descriptors for O(1) move evaluation
- **What**: For each route, precompute an array of `SegmentInfo { duration, earliest_departure, latest_arrival, time_warp, demand }` from each node forward and backward. Provide `concat(a, b) -> SegmentInfo`. Use these to evaluate any relocate/swap/2-opt\* candidate in O(1).
- **Why**: `is_feasible` is called on every proposed move. With 400-node instances and ~10 000+ moves/sec of search pressure, concat evaluators give ~10x-30x speedup in local-search-bound phases.
- **Impact**: Very high. Multiplies effective iteration count.
- **Sketch**: The canonical reference is [Vidal 2014, Sec.3.2](https://www.sciencedirect.com/science/article/abs/pii/S0377221713005547). `concat` is:
  ```rust
  fn concat(a: Seg, b: Seg) -> Seg {
      let delta = max(0, b.earliest_departure - a.latest_arrival);
      Seg {
          duration: a.duration + b.duration + delta,
          earliest_departure: max(a.earliest_departure,
                                  b.earliest_departure - a.duration - a.time_warp),
          latest_arrival: min(a.latest_arrival,
                               b.latest_arrival - a.duration + a.time_warp),
          time_warp: a.time_warp + b.time_warp + delta,
          demand: a.demand + b.demand,
      }
  }
  ```
  Move evaluation becomes "concat segments at the 2-3 route split points and test `time_warp == 0` and `demand <= capacity`".
- **Risk**: Nontrivial to get right -- write a randomized fuzz-test that checks concat-based feasibility matches the current `is_feasible` on random routes. Don't skip this.

### [P2] Granular neighbor lists for insertion & local-search moves
- **What**: Precompute `neighbors[i]` = top-Gamma (Gamma in [20, 30]) nearest customers by distance (or by Shaw relatedness). In `greedy_insertion`, `regret_insertion`, and all SA operators, restrict candidate insert-after nodes to `neighbors[cust]` U `neighbors[removed-from-arc-endpoints]`.
- **Why**: Reduces insertion/repair scan from O(n * route_length) to O(n * Gamma). For n=400, Gamma=25, that's ~16x speedup in the repair-bound phases.
- **Impact**: High, compounds with P1.
- **Sketch**: The existing `shaw_neighbors` array is already close -- reuse it. In `greedy_insertion`, only try inserting `cust` adjacent to one of its Gamma nearest neighbors currently in some route.
- **Risk**: Can occasionally miss the true best insertion; mitigated by occasionally (every k-th iteration) falling back to full scan or by Gamma large enough (25-30).

### [P3] SISR destroy + greedy-blink repair as a 5th/6th operator
- **What**: Implement string-removal destroy (L_max=10, c_avg=10) and greedy-blink repair (beta=0.01). Add them to the ALNS operator pool.
- **Why**: Consistently outperforms random/worst/Shaw on VRPTW in the literature; preserves slack around removed segments; simple to implement.
- **Impact**: Large, especially on clustered (C1, C2, RC1, RC2) instances.
- **Sketch**:
  ```rust
  fn sisr_destroy(routes, dm, tw, rng) -> Vec<usize> {
      let c_avg = 10.0;
      let l_max = 10;
      let avg_len = avg_route_len(routes);
      let l_smax = l_max.min(avg_len);
      let k_s_max = ((4.0 * c_avg) / (1.0 + l_smax as f64) - 1.0) as usize;
      let k_s = 1 + rng.next() % (k_s_max + 1);
      let seed = random_customer(routes, rng);
      // walk through up to k_s routes ordered by proximity to seed
      // from each remove a string of length U[1, min(route_len, l_smax)]
      // with prob alpha: split-string (remove L customers, skipping m survivors)
  }
  fn blink_repair(partial, removed, dm, tw, rng) {
      // order removed by demand descending
      for cust in removed {
          for (pos, cost) in candidate_positions(cust, partial) {
              if !feasible(pos) { continue; }
              if rng.next_f64() < 0.01 { continue; } // blink
              // accept first non-blinked feasible insertion
              break;
          }
      }
  }
  ```
- **Risk**: Low. SISR is well-specified in the paper and has clean parameter defaults.

### [P4] SWAP\* operator
- **What**: Add a SWAP\* move to the SA/LS phase: swap `i in A` and `j in B` but reinsert each at its *own best* position in the other route (not in place).
- **Why**: Finds moves invisible to plain `try_exchange` (which swaps in place). [Vidal 2022 Sec.4.2](https://arxiv.org/abs/2012.10384) shows SWAP\* drives most of the remaining quality gains once RELOCATE/2-OPT\* are saturated.
- **Impact**: Medium-high, specifically in the endgame.
- **Sketch**: For each pair `(i in A, j in B)` with `i in neighbors[j]` or vice versa:
  1. Remove `i` from `A` (compute residual A' cost in O(1) via P1).
  2. Remove `j` from `B` (residual B' cost in O(1)).
  3. Find best insertion positions for `j` in A' and `i` in B' restricted to Gamma-neighbor-adjacent slots.
  4. Accept if total delta is improving.
- **Risk**: More complex than plain swap; gate with neighbor-list pruning to keep cost manageable.

### [P5] 2-opt\* (inter-route, no reversal)
- **What**: Replace `try_intra_2opt` with `try_2opt_star`: swap *tails* between two routes without reversing either.
- **Why**: The current intra-route 2-opt reverses a segment, which flips arrival times through that segment -- usually infeasible under tight TWs. 2-opt\* keeps direction and is the VRPTW-native inter-route crossover. Potvin & Rousseau 1995.
- **Impact**: Medium. Current intra-2-opt is failing silently most of the time.
- **Sketch**:
  ```rust
  fn try_2opt_star(routes, rng) {
      let (r1, r2) = pick_two_routes(rng);
      let i = random_edge_in(r1);
      let j = random_edge_in(r2);
      let new_r1 = r1[..=i] ++ r2[j+1..];   // no reversal
      let new_r2 = r2[..=j] ++ r1[i+1..];
      if feasible_both(new_r1, new_r2) { ... }
  }
  ```
- **Risk**: Low. Standard textbook operator.

### [P6] Noise-perturbed greedy repair (third repair operator)
- **What**: Add a 3rd repair op to the ALNS pool: greedy insertion with cost perturbation `Delta' = Delta + xi * d_max`, xi in [-eta, +eta], eta = 0.1 per [Ropke & Pisinger 2006].
- **Why**: Pure greedy and regret-2 explore similar solutions. Noise-greedy adds cheap diversification.
- **Impact**: Small but compounding -- ALNS adaptive weights will discover when it helps.
- **Sketch**: Trivial -- wrap existing `greedy_insertion` with a cost noise term.

### [P7] Late-Acceptance Hill Climbing for the SA phase
- **What**: Replace the geometric-cooling SA in Phase 5 with LAHC (buffer L=1500).
- **Why**: Removes temperature-tuning. Self-calibrating. Empirically strong on VRP.
- **Impact**: Medium; comparable to SA when SA is well-tuned, better when it isn't.
- **Sketch**:
  ```rust
  let L = 1500;
  let mut hist = vec![current_pen; L];
  loop {
      let k = iter % L;
      let cand = propose_move();
      if cand < current_pen || cand < hist[k] { accept(cand); }
      hist[k] = current_pen;
      iter += 1;
  }
  ```
- **Risk**: Low. Keep current SA as fallback; A/B test.

### [P8] Adaptive ruin size
- **What**: Scale `destroy_count` with stagnation: `q = q_min + (stag / stag_max) * (q_max - q_min)`, reset on improvement.
- **Why**: Small ruins intensify around current solution; larger ruins kick out of local optima.
- **Impact**: Small-medium.
- **Sketch**: Replace the fixed `[total_custs/10, total_custs*3/10]` range with a stagnation-scaled target.

### [P9] Bitset-based `contains` in destroy operators
- **What**: Replace `custs.contains(ref nb)` (O(n)) in `shaw_removal` with a `vec<bool>` or bitset of size n. Same for `used` tracking in `worst_removal`.
- **Why**: Inside an O(count) loop with O(n) contains check -> O(n^2) destroy. Bitset -> O(count + n).
- **Impact**: Small (destroy is not the bottleneck), but trivially cheap.
- **Sketch**: `let in_cust_set = bitset_from(ref custs);` -- check `in_cust_set[nb]`.

### [P10] Flat distance matrix with stride indexing
- **What**: Store `distance_matrix` as `Vec<i32>` indexed by `i * n + j` rather than `Vec<Vec<i32>>`.
- **Why**: One less pointer indirection; contiguous memory improves L1/L2 cache hit rate, particularly for the scan-heavy repair loops.
- **Impact**: ~5-15% raw speedup on inner loops. Small but free.
- **Risk**: Constant change but touches many call sites -- gate behind a `#[inline] dm(i, j)` wrapper.

### [P11] Incremental `penalized_cost` in the ALNS phase
- **What**: Track per-route distances in the ALNS phase like the SA phase already does; compute new `penalized_cost` by delta, not full recompute.
- **Why**: `penalized_cost(ref new_routes)` is O(total nodes) on every iteration.
- **Impact**: Small-medium -- eliminates a repeated O(n) sweep.

### [P12] Additional destroy operators from the literature
- **Zone/radius removal** -- remove all customers within radius r of a random seed.
- **Route-pair removal** -- pick two adjacent (by centroid distance) routes and empty both.
- **Time-window-based removal** -- remove customers whose TW intersects a random interval.
- **Impact**: Small each, but adaptive weights will find the right mix per instance class.

### [P13] Longer-segment Or-opt (chains of 4-5)
- **What**: Current `try_or_opt` uses segment lengths 1-3. Extending to 4-5 opens larger TW-preserving moves. Keep stochastic selection.
- **Impact**: Small.

### [P14] Instance-class-aware parameter tuning
- **What**: The 24 HG instances are 6 categories x 4 sizes. C1/C2/RC1 benefit from clustering-aware destroy (zone, SISR); R1/R2 benefit from random + worst; RC2 benefits from regret-3. Detect category by geometric clustering of customer positions at start and pre-weight destroy ops accordingly.
- **Impact**: Small-medium on specific categories.

### [P15] Split-based construction instead of NN
- **What**: Build a giant tour by nearest-neighbor (ignoring capacity/TW), then run **Split** [Prins 2004; Vidal 2014] to partition into feasible routes via shortest path on the route DAG.
- **Why**: Typically yields a better initial solution than NN + merge; crucial for HGS but also helps warm-starting ALNS.
- **Impact**: Medium on the initial solution quality; ALNS usually converges faster from a better start.
- **Risk**: Medium implementation effort.

### [P16] Penalty multiplier adaptation for fleet overflow
- **What**: `fleet_penalty` is fixed at 100 000. Make it adaptive: if solution is usually fleet-feasible, lower the penalty to free exploration; if often over fleet, raise. Vidal's feasibility-band adjustment.
- **Impact**: Small.

### [P17] Record crossover / solution pooling
- **What**: Keep a population of top-K distinct solutions and occasionally recombine via SREX ([Nagata & Braeysy 2009, *Networks*](https://doi.org/10.1002/net.20338)). Even K=5 with SREX every 2000 iters adds diversity.
- **Why**: Push toward HGS-style diversification without the full genetic framework.
- **Impact**: Medium on long runs (>=20 s).
- **Risk**: Nontrivial implementation.

---

## 4. Suggested Execution Order

A single agent over several iterations should tackle these in roughly this order, based on ROI and dependency:

1. **P0** (budget extension) -- one-line change, unlocks everything else.
2. **P5** (2-opt\*) -- replace the broken intra-2-opt.
3. **P9, P11** (bitset + incremental cost) -- easy cleanups, keep compiling.
4. **P2** (neighbor-list pruning) -- feeds into P4, P1.
5. **P1** (concatenation descriptors) -- the big one.
6. **P3** (SISR + blink repair) -- landmark operator.
7. **P4** (SWAP\*) -- intensification.
8. **P6, P7** (noise repair + LAHC) -- diversification.
9. **P13, P12** (longer Or-opt, more destroy ops) -- ALNS operator zoo.
10. **P15** (Split construction) -- construction upgrade.
11. **P17** (population + SREX) -- final quality push.

Every step should be measured against the benchmark harness before shipping.

---

## 5. Concrete Hypotheses the Agent Can Submit

Each maps to a testable ALNS hypothesis for the swarm server:

| Title | Tag | Why it should improve score |
|---|---|---|
| Extend internal deadline from 4.5 s -> 27 s | `other` | Uses 6x more wall time; ALNS converges monotonically |
| Replace intra 2-opt with 2-opt\* (inter-route, no reversal) | `local_search` | Intra 2-opt reverses segments, almost always TW-infeasible |
| Add SISR string-removal destroy operator | `construction` | Preserves slack around removed region; dominant on clustered instances |
| Add greedy-blink repair with beta = 0.01 | `construction` | Cheap stochastic repair with SISR-like behaviour |
| Add SWAP\* operator with neighbor-list pruning | `local_search` | Vidal 2022; catches moves invisible to in-place swap |
| Vidal concatenation segment descriptors for O(1) moves | `data_structure` | 10-30x speedup on inner loops; compounds with everything else |
| Granular neighbor-list pruning in repair + local search | `data_structure` | 10-20x speedup on insertion; Toth-Vigo 2003 |
| Bitset for `contains` in Shaw/worst removal | `data_structure` | Eliminates O(n^2) inside destroy |
| Flat stride-indexed distance matrix | `data_structure` | Cache-friendly; ~10% raw speedup |
| Noise-perturbed greedy repair | `construction` | Ropke/Pisinger diversification |
| Late-Acceptance Hill Climbing instead of SA | `metaheuristic` | One parameter; robust to tuning |
| Stagnation-scaled adaptive ruin size | `metaheuristic` | Balance intensification vs. diversification |
| Zone / time-window / route-pair destroy operators | `construction` | Extends operator pool |
| Longer Or-opt chains (4-5) | `local_search` | More movement under TW preservation |
| Split-based construction (Prins/Vidal) | `construction` | Better initial solution than NN + merge |
| Instance-class-aware operator weighting | `hybrid` | C/RC benefit from clustering-aware destroy, R from random |

---

## 6. Sources

- [Ropke & Pisinger 2006 -- ALNS foundational paper (*Transportation Science*)](https://www.sciencedirect.com/science/article/abs/pii/S0305054805003023)
- [Ropke & Pisinger 2006 -- DTU tech-report PDF](https://backend.orbit.dtu.dk/ws/portalfiles/portal/3154899/An%20adaptive%20large%20neighborhood%20search%20heuristic%20for%20the%20pickup%20and%20delivery%20problem%20with%20time%20windows_TechRep_ropke_pisinger.pdf)
- [Christiaens & Vanden Berghe 2020 -- SISR (*Transportation Science*)](https://pubsonline.informs.org/doi/10.1287/trsc.2019.0914)
- [SISR preprint PDF](https://scispace.com/pdf/slack-induction-by-string-removals-for-vehicle-routing-14gqwirdlp.pdf)
- [Vidal 2022 -- HGS-CVRP and SWAP\* (arXiv 2012.10384)](https://arxiv.org/abs/2012.10384)
- [Vidal HGS-CVRP GitHub reference implementation](https://github.com/vidalt/HGS-CVRP)
- [Kool et al. 2022 -- HGS-VRPTW, DIMACS 2022 winner (PDF)](https://wouterkool.github.io/pdf/paper-kool-hgs-vrptw.pdf)
- [Wouda et al. 2024 -- PyVRP (arXiv 2403.13795)](https://arxiv.org/pdf/2403.13795)
- [PyVRP documentation](https://pyvrp.org/)
- [Vidal 2014 -- unified framework for multi-attribute VRPs (*EJOR*)](https://www.sciencedirect.com/science/article/abs/pii/S0377221713005547)
- [Vidal 2016 -- constant-time move evaluations (*Networks*)](https://doi.org/10.1002/net.21693)
- [Savelsbergh 1992 -- Forward Time Slack (*INFORMS J. on Computing*)](https://pubsonline.informs.org/doi/10.1287/ijoc.4.2.146)
- [Kindervater & Savelsbergh 1997 -- edge exchanges](https://www.sciencedirect.com/science/article/abs/pii/S0167637795003240)
- [Toth & Vigo 2003 -- Granular Tabu Search (*INFORMS J. on Computing*)](https://pubsonline.informs.org/doi/10.1287/ijoc.15.4.333.24890)
- [Burke & Bykov 2017 -- Late Acceptance Hill Climbing (*EJOR*)](https://www.sciencedirect.com/science/article/abs/pii/S0377221716305495)
- [Late Acceptance Hill Climbing -- Wikipedia](https://en.wikipedia.org/wiki/Late_acceptance_hill_climbing)
- [Schrimpf et al. 2000 -- Ruin & Recreate](https://www.sciencedirect.com/science/article/abs/pii/S0021999199964136)
- [Helsgaun LKH-3 project page](http://akira.ruc.dk/~keld/research/LKH-3/)
- [Solomon 1987 -- insertion heuristics (*Operations Research*)](https://pubsonline.informs.org/doi/10.1287/opre.35.2.254)
- [Potvin & Rousseau 1995 -- 2-opt\* inter-route exchange](https://doi.org/10.1016/0377-2217(94)00114-3)
- [Prins 2004 -- Split procedure for VRP](https://www.sciencedirect.com/science/article/abs/pii/S0305054803000498)
- [Nagata & Braeysy 2009 -- SREX crossover (*Networks*)](https://doi.org/10.1002/net.20338)
- [Bent & Van Hentenryck 2004 -- Two-stage hybrid local search (PDF)](https://cs.brown.edu/research/pubs/pdfs/2004/Bent-2004-TSH.pdf)
- [Braeysy & Gendreau -- VRPTW survey (PDF)](https://cepac.cheme.cmu.edu/pasi2011/library/cerda/braysy-gendreau-vrp-review.pdf)
- [SINTEF VRPTW 400-customer benchmark](https://www.sintef.no/projectweb/top/vrptw/homberger-benchmark/400-customers/)
- [Spark-ALNS 2024 (*Nature Scientific Reports*)](https://www.nature.com/articles/s41598-024-74432-2)
- [PPO-ALNS 2025 (*J. Combinatorial Optimization*)](https://link.springer.com/article/10.1007/s10878-025-01364-6)
- [HGS + R&R hybrid 2022 (*J. Heuristics*)](https://link.springer.com/article/10.1007/s10732-022-09500-9)
- [Learning-to-search multi-TW VRP 2025 (arXiv 2505.23098)](https://arxiv.org/pdf/2505.23098)
- [Fast Ejection Chain VRPTW (Springer)](https://link.springer.com/chapter/10.1007/11546245_8)
- [Schneider 2020 -- Granular LS analysis (PDF)](https://logistik.bwl.uni-mainz.de/files/2020/04/dpo_2020_03.pdf)
