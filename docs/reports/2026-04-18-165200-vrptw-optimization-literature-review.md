# VRPTW Optimization Techniques: Literature Review for Benchmark Improvement

**Generated:** 2026-04-18
**Topic:** State-of-the-art techniques from CS/AI/ML preprint literature applicable to improving scores on the 400-node Gehring-Homberger VRPTW benchmark with a 30-second single-threaded time limit

## Executive Summary

This report surveys recent (2024--2026) preprint and published literature on Vehicle Routing Problem with Time Windows (VRPTW) optimization, focusing on techniques applicable to our specific benchmark constraints: 24 Gehring-Homberger 400-node instances, 30-second single-threaded time limit, scored as average distance with massive infeasibility penalties. The current solver uses a hybrid ALNS (4 destroy / 2 repair operators) followed by SA fine-tuning with 5 local search operators.

The most promising improvement directions, ranked by expected impact within our constraints, are: (1) adding SISRs-style string removal with spatial slack induction, (2) implementing the SWAP* inter-route neighborhood, (3) adopting HGS-style time-warp penalties to search through infeasible space, (4) adding regret-k insertion for k greater than 2, and (5) incorporating granular neighborhood structures for faster move evaluation. Several LLM-driven algorithm discovery frameworks (VRPAgent, ReEvo, PyVRP+, AlphaEvolve) demonstrate that automated operator design can outperform hand-crafted heuristics, which is directly relevant to our swarm-based optimization approach.

## 1. Adaptive Large Neighborhood Search (ALNS) Improvements

### 1.1 Reinforcement Learning for Operator Selection (PPO-ALNS)

A 2025 paper integrates Proximal Policy Optimization (PPO) with ALNS for operator selection, replacing the classical roulette-wheel weight update. The RL policy observes solution state features (current cost, stagnation counter, solution structure metrics) and selects destroy/repair operator pairs. On VRPTW instances with 20--100 customers, PPO-ALNS achieves 11--17\% improvement over traditional ALNS.

**Applicability to our solver:** Our current adaptive weight scheme uses simple exponential smoothing with segment-based updates (every 80 iterations). The RL approach is too heavyweight for runtime use, but the insight that operator selection should be state-dependent is actionable. We could condition operator selection on stagnation count, current fleet excess, and solution density rather than using history-blind roulette weights.

**Source:** Reinforcement learning-guided adaptive large neighborhood search for VRPTW, Journal of Combinatorial Optimization, 2025.

### 1.2 Enhanced Destroy Operators

#### 1.2.1 SISRs: Slack Induction by String Removals

Christiaens and Vanden Berghe (2020, Transportation Science) introduced SISRs, which has become a top-performing general VRP heuristic. The key innovations:

- **String removal:** Instead of removing individual customers, remove contiguous subsequences (strings) from routes. Multiple strings from spatially proximate routes are removed together, preserving partial route structure.
- **Spatial slack:** After removing strings, the remaining route segments have "slack" --- temporal and capacity room that makes reinsertion of new customers easier.
- **Blinks in greedy repair:** During reinsertion, certain feasibility checks are probabilistically skipped ("blinks"), allowing the algorithm to explore solutions that would otherwise be pruned. This is a lightweight form of constraint relaxation.

**Applicability:** Our current destroy operators (random, worst, Shaw, route removal) all remove individual customers. Adding string removal would be a significant improvement --- it preserves route structure while creating the slack needed for profitable reinsertions. This is the single highest-impact addition based on the literature.

**Implementation sketch:** Select a random customer, find its route, extract a string of length L (2--5) centered on it. Then find k-1 additional nearby routes and extract strings from those too. Total removed = sum of string lengths.

#### 1.2.2 Historical Knowledge Removal

Track which customers appeared in the best solutions found so far and which did not. Remove customers that have been in poor positions across recent iterations. This complements worst removal by using historical signal rather than just current marginal cost.

### 1.3 Enhanced Repair Operators

#### 1.3.1 Regret-k Insertion (k = 3, 4)

Our current regret insertion uses k=2 (difference between best and second-best insertion). The literature shows regret-3 and regret-4 produce better results on instances with tight time windows (R1, RC1 categories) because they better anticipate future insertion difficulty. The computational overhead is modest --- we already compute per-route best insertions.

#### 1.3.2 Perturbation in Greedy Insertion

Adding noise to insertion cost evaluation (multiplying by a random factor in [0.8, 1.2]) introduces diversity without abandoning greedy structure. This is the "blinks" concept from SISRs applied to cost evaluation rather than feasibility checking.

## 2. Hybrid Genetic Search (HGS) Techniques

### 2.1 Time-Warp Penalty Mechanism

The HGS-VRPTW implementation by Kool et al. uses the time-warp concept: when a vehicle arrives late at a customer, it "travels back in time" to arrive on time, accumulating a penalty. This allows the search to traverse infeasible (time-window-violating) solutions, with the penalty weight dynamically adjusted to maintain a target ratio of feasible to infeasible solutions (15--25\% feasible in the local search population).

**Applicability:** Our current solver strictly enforces time window feasibility during insertion (is\_feasible check). This means many potentially good moves are rejected. Adopting time-warp penalties would dramatically expand the search space. The penalty weight starts at 1.0 and is increased by 20\% if fewer than 15\% of solutions are feasible, decreased by 15\% if more than 25\% are feasible.

**Implementation impact:** This requires modifying the insertion operators to compute time-warp cost instead of binary feasibility, and adjusting the acceptance criterion to include penalized time-warp. The payoff is large --- HGS-VRPTW is the state-of-the-art solver largely because of this mechanism.

### 2.2 SWAP* Neighborhood

SWAP* exchanges two customers between different routes, but unlike standard swap, each customer is inserted at the best position in the other route (not necessarily the position vacated by the other customer). This is a compound move that subsumes simple swap and produces better results.

The PyVRP implementation further enhances SWAP* with:
- Time window support via efficient forward/backward time-warp propagation
- Caching of insertion costs to avoid redundant computation
- Early termination when evaluating known-bad moves

**Applicability:** Our current exchange operator (try\_exchange) swaps customers in-place. SWAP* would find better moves at similar computational cost by decoupling the removal and insertion positions.

### 2.3 SREX Crossover

Selective Route Exchange (SREX) combines entire routes from two parent solutions. One parent contributes a set of routes; the other parent contributes similar routes (by customer overlap). The offspring inherits complete routes, preserving route structure better than customer-level crossover.

**Applicability:** This requires maintaining a population of solutions, which our current single-solution ALNS does not. However, a lightweight version could maintain a small elite archive (3--5 solutions) and occasionally recombine routes between the current solution and an archived one.

### 2.4 Population Diversity Management

HGS maintains separate feasible and infeasible solution pools with diversity measured by broken-pairs distance (fraction of customer adjacencies that differ between solutions). Survivors are selected based on both fitness and diversity contribution. This prevents premature convergence.

## 3. Local Search Operator Improvements

### 3.1 Granular Neighborhoods

Instead of evaluating all O(n squared) possible moves, restrict each customer's neighborhood to its k nearest neighbors (typically k=20--40). This reduces move evaluation cost by 80--90\% with minimal quality loss. The key insight from PyVRP: the granular neighborhood should consider both spatial distance and temporal proximity (time window overlap).

**Applicability:** Our SA phase already randomly selects routes and positions, which is a form of implicit neighborhood restriction. But formalizing a granular neighborhood list (which we already compute as shaw\_neighbors) for the SA operators would allow systematic best-improvement search within the time budget.

### 3.2 Efficient Move Evaluation with Concatenation

For VRPTW, move evaluation requires checking time window feasibility, which naively requires O(n) per route segment. The concatenation technique precomputes forward and backward cumulative data (earliest arrival, latest departure, cumulative demand, cumulative time warp) for each route prefix/suffix. With this, any move can be evaluated in O(1) by concatenating precomputed segments.

**Applicability:** This is a major acceleration opportunity. Our current try\_relocate, try\_exchange, and try\_or\_opt operators check feasibility by iterating over the modified route. Precomputing concatenation data would make each move evaluation constant-time, allowing many more moves to be evaluated per second.

### 3.3 Or-opt with Segment Lengths 1, 2, 3

Our current try\_or\_opt moves single customers. The literature shows that moving segments of 2 or 3 consecutive customers (or-opt-2, or-opt-3) produces significant additional improvement, especially on clustered instances (C1, C2 categories).

## 4. Neural and Learning-Based Approaches

### 4.1 Neural Deconstruction Search (NDS)

Hottung et al. (2025, TMLR) train a transformer-based policy to select which customers to remove from a solution, replacing handcrafted destroy operators. The policy is trained via REINFORCE and generates diverse deconstructions by conditioning on random seed vectors. A simple greedy insertion reconstructs the solution.

**Results:** NDS outperforms PyVRP-HGS by 2--3.5\% on VRPTW instances with 500--2000 customers.

**Applicability:** Training a neural policy is out of scope for our 30-second runtime. However, the insight that destroy operator quality is the primary bottleneck is actionable --- investing more engineering effort in destroy operators (SISRs, historical, cluster-aware) will likely yield better returns than improving repair operators.

### 4.2 Learning-Enhanced Neighborhood Selection (LENS)

A random forest model predicts which neighborhood (set of customers to destroy/repair) will yield the most improvement, based on features like route distances, time window tightness, and capacity utilization. Achieves 11.8\% improvement over random neighborhood selection after 200 iterations.

**Applicability:** The lightweight feature computation (route statistics, time window metrics) could be used to bias our destroy operator selection without ML overhead. For example, prefer Shaw removal on clustered instances, worst removal on loose-time-window instances, and route removal when fleet is exceeded.

### 4.3 VRPAgent: LLM-Discovered Operators

VRPAgent (Hottung et al., 2025) uses an LLM to generate C++ destroy and ordering operators within a fixed LNS framework. A genetic search evolves the generated operators. Key finding: the best discovered operators use ensemble approaches, randomly selecting among 5--9 component heuristics each iteration, combining distance-based, demand-based, and time-based criteria.

**Results on VRPTW:**
- 500 customers: score 47.97 (-0.24\% vs SOTA)
- 1000 customers: score 87.40 (-0.33\% vs SOTA)
- 2000 customers: score 166.96 (-0.33\% vs SOTA)

**Applicability:** This directly validates our swarm approach --- iteratively evolving algorithm code through an LLM-guided loop is state-of-the-art. The ensemble operator insight suggests our destroy operator should randomly mix strategies within a single operator call rather than selecting one operator per iteration.

### 4.4 PyVRP+: Metacognitive Heuristic Evolution

PyVRP+ (April 2026) uses GPT-4 with a structured Reason-Act-Reflect cycle to evolve three HGS components: parent selection, survivor selection, and penalty updates. Achieves up to 2.70\% improvement on VRPTW.

**Key insight:** Domain-aware initialization (telling the LLM about common pitfalls) dramatically outperforms naive code generation. This suggests our swarm agents should receive structured prompts about VRPTW-specific failure modes.

### 4.5 ReEvo: Reflective Evolution

ReEvo (NeurIPS 2024) pairs LLM code generation with evolutionary search, using "verbal gradients" --- the LLM reflects on why one heuristic outperformed another and uses that analysis to guide the next generation. Works across TSP, CVRP, bin packing, and other combinatorial problems.

### 4.6 AlphaEvolve

Google DeepMind's AlphaEvolve (May 2025) extends FunSearch to evolve entire codebases. Applied to 50+ open mathematical problems, it improved state-of-the-art in 20\% of cases. Found a matrix multiplication algorithm beating Strassen's 1969 result. Recovered 0.7\% of Google's global compute through better scheduling.

**Relevance:** Demonstrates that LLM-guided evolutionary code optimization works at scale. Our swarm is a simplified version of this paradigm.

## 5. Specific Algorithmic Techniques Worth Implementing

### 5.1 Multi-Start with Diverse Constructions

Instead of a single nearest-neighbor construction, generate 3--5 diverse initial solutions using different heuristics (nearest neighbor, savings algorithm, time-oriented nearest neighbor, random insertion). Run ALNS from the best, but keep others in an elite archive for occasional recombination.

### 5.2 Adaptive Perturbation Strength

When stagnating, gradually increase the destroy size from 10\% to 40\% of customers. When improving, reduce it back to 10--15\%. This adapts exploration intensity to search progress.

### 5.3 Fleet Minimization Phase

Before distance optimization, run a dedicated fleet minimization phase using ejection chains or route merging with time-warp relaxation. Feasibility (using at most fleet\_size vehicles) is worth 1,000,000 points per instance in our scoring --- getting all instances feasible is the first priority.

### 5.4 Instance-Adaptive Parameter Tuning

The six instance categories (R1, R2, RC1, RC2, C1, C2) have very different characteristics:
- **R1:** Random positions, narrow time windows --- local search works well
- **R2:** Random positions, wide time windows --- construction quality matters
- **C1:** Clustered positions, narrow time windows --- cluster-based operators shine
- **C2:** Clustered positions, wide time windows --- route minimization critical
- **RC1/RC2:** Mixed --- need balanced operator portfolio

Detecting instance type from position/time-window statistics and adjusting operator weights accordingly would improve performance across all 24 instances.

### 5.5 Route Ejection and Reinsertion

When the solution uses more vehicles than allowed, select the shortest route, remove all its customers, and attempt to reinsert them into remaining routes using regret insertion with time-warp relaxation. This is more targeted than generic route removal.

## 6. Prioritized Recommendations

Based on expected improvement, implementation complexity, and compatibility with our 30-second single-threaded constraint:

| Priority | Technique | Expected Impact | Implementation Effort | Section |
|----------|-----------|-----------------|----------------------|---------|
| 1 | String removal (SISRs) | High | Medium | 1.2.1 |
| 2 | Time-warp penalties | High | Medium | 2.1 |
| 3 | SWAP* neighborhood | Medium-High | Medium | 2.2 |
| 4 | Concatenation-based move eval | Medium-High | High | 3.2 |
| 5 | Regret-3/4 insertion | Medium | Low | 1.3.1 |
| 6 | Granular neighborhoods | Medium | Low | 3.1 |
| 7 | Or-opt-2/3 segments | Medium | Low | 3.3 |
| 8 | Ensemble destroy operators | Medium | Low | 4.3 |
| 9 | Adaptive destroy size | Low-Medium | Low | 5.2 |
| 10 | Instance-adaptive params | Low-Medium | Medium | 5.4 |
| 11 | Multi-start construction | Low | Low | 5.1 |
| 12 | Elite archive recombination | Low | Medium | 2.3 |

## 7. Comparison with Current Solver

Our current algorithm already implements:
- ALNS framework with adaptive weights (good)
- 4 destroy operators: random, worst, Shaw, route removal (adequate)
- 2 repair operators: greedy insertion, regret-2 (adequate)
- SA fine-tuning with 5 local search operators (good)
- Shaw relatedness with distance + time window overlap (good)
- Fleet penalty for excess vehicles (good)

**Key gaps vs. state-of-the-art:**
1. No string removal (SISRs) --- the most impactful missing destroy operator
2. Strict feasibility enforcement --- should use time-warp penalties instead
3. No SWAP* --- only in-place exchange
4. O(n) feasibility checks per move --- should use concatenation for O(1)
5. Only regret-2 --- regret-3/4 would help on tight-window instances
6. No or-opt with segments of length 2 or 3
7. No instance-type detection or parameter adaptation

## Sources

- [PPO-ALNS for VRPTW](https://link.springer.com/article/10.1007/s10878-025-01364-6) - RL-guided operator selection
- [ALNS with Deep Learning (PALNS)](https://link.springer.com/article/10.1007/s12065-025-01115-w) - Parallel ALNS with VAE integration
- [IALNS-SA for VRPTW](https://www.mdpi.com/2079-9292/14/12/2375) - Enhanced ALNS with simulated annealing
- [SISRs: Slack Induction by String Removals](https://pubsonline.informs.org/doi/10.1287/trsc.2019.0914) - String removal destroy operator
- [Neural Deconstruction Search](https://arxiv.org/abs/2501.03715) - Learned destroy policies for VRP
- [VRPAgent: LLM-Driven Heuristic Discovery](https://arxiv.org/abs/2510.07073) - LLM-evolved operators for VRPTW
- [PyVRP+: LLM-Driven Metacognitive Evolution](https://arxiv.org/abs/2604.07872) - LLM evolution of HGS components
- [ReEvo: LLMs as Hyper-Heuristics](https://arxiv.org/abs/2402.01145) - Reflective evolution framework
- [RFTHGS: RL-Finetuned LLM for HGS](https://arxiv.org/abs/2510.11121) - RL-finetuned crossover operators
- [HGS-CVRP (Vidal)](https://github.com/vidalt/HGS-CVRP) - Hybrid Genetic Search implementation
- [HGS-VRPTW (Kool et al.)](https://wouterkool.github.io/publication/hgs-vrptw/) - HGS with time windows and SWAP*
- [PyVRP Solver Package](https://arxiv.org/abs/2403.13795) - State-of-the-art VRP solver
- [LENS: Learning-Enhanced Neighborhood Selection](https://arxiv.org/abs/2403.08839) - ML-guided neighborhood selection
- [LNS + HGS for Inventory Routing](https://arxiv.org/abs/2506.03172) - Combined LNS and HGS
- [Combinatorial Optimization for All](https://arxiv.org/abs/2503.10968) - LLMs for non-expert algorithm improvement
- [AlphaEvolve (DeepMind)](https://arxiv.org/abs/2506.13131) - Gemini-powered algorithm discovery
- [FunSearch (DeepMind)](https://www.nature.com/articles/s41586-023-06924-6) - LLM program search for mathematical discovery
- [LLM Meta-Optimizers Survey](https://link.springer.com/article/10.1007/s10462-025-11470-w) - Survey of LLM-driven optimization
- [Gehring-Homberger Benchmark (SINTEF)](https://www.sintef.no/projectweb/top/vrptw/homberger-benchmark/400-customers/) - Best known solutions
- [Heuristics for VRP Survey](https://arxiv.org/abs/2303.04147) - Comprehensive VRP heuristics survey
- [Spark-ALNS for VRPTW](https://www.nature.com/articles/s41598-024-74432-2) - Parallel ALNS implementation
- [GECCO ML4VRP Competition 2024](https://github.com/ML4VRP/ML4VRP2024) - ML for VRP competition
- [Swarm Optimization for VRPTW](http://www.aimspress.com/article/doi/10.3934/era.2026116) - CSO-TS hybrid approach
- [RL-ALNS Operator Selection (ROADEF 2026)](https://roadef2026.sciencesconf.org/685226/document) - RL for ALNS at ROADEF
