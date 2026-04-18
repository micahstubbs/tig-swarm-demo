# VRPTW GitHub Projects and Techniques for Benchmark Optimization

**Generated:** 2026-04-18
**Topic:** Open-source projects, frameworks, and techniques on GitHub that can improve scores on the TIG VRPTW benchmark (400-node Homberger instances, 30-second single-threaded timeout)

## Executive Summary

This report catalogs GitHub repositories and state-of-the-art techniques relevant to improving scores on the TIG swarm demo VRPTW benchmark. The most immediately actionable finding is that the **TIG monorepo itself** (tig-foundation/tig-monorepo) contains 29 community-submitted Rust algorithm implementations using the identical `solve_challenge()` function signature, including a full Hybrid Genetic Search (HGS) implementation by Thibaut Vidal. Beyond direct solver implementations, a thriving ecosystem of **LLM-powered evolutionary code discovery frameworks** (OpenEvolve, SkyDiscover, CodeEvolve, ShinkaEvolve) are explicitly designed to work with TIG challenges and could automate the agent optimization loop. The **autoresearch** pattern (74K+ stars across variants) validates the core edit-benchmark-keep/discard loop our swarm agents use. On the algorithmic side, the highest-impact techniques for 30-second time-limited runs are: O(1) move evaluation via route segment concatenation, SWAP* inter-route neighborhood, SISR (Slack Induction by String Removals) destroy operators, and population-based genetic search with dual feasible/infeasible subpopulations.

## Tier 1: Directly Applicable Projects

### TIG Monorepo (tig-foundation/tig-monorepo)

- **URL:** github.com/tig-foundation/tig-monorepo
- **Stars:** 107 | **Language:** Rust | **License:** Custom TIG licenses
- **Last Updated:** 2026-04-17

The most relevant repository. Contains **29 community-submitted VRPTW algorithms** in `tig-algorithms/src/vehicle_routing/`, all using the identical Rust `solve_challenge()` function signature with `Challenge`, `Solution`, and `save_solution` types. Notable submissions:

- **`hgs_v1`**: Full Rust port of Vidal's Hybrid Genetic Search. Multi-file implementation with `constructive.rs`, `genetic.rs`, `local_search.rs`, `population.rs`, `individual.rs`, `sequence.rs`, `solver.rs`. Configurable exploration levels (0-6). This represents the state of the art for VRPTW metaheuristics in Rust.
- **`fast_lane_v2/v3/v4`**: Hybrid Genetic Algorithm with Route-Based Crossover (RBX), Or-Opt moves, diversity boosting. Multi-file with `builder.rs`, `evolution.rs`, `gene_pool.rs`, `operators.rs`.
- **Clarke-Wright variants**: `clarke_wright`, `clarke_wright_super`, `enhanced_cw`, `advanced_cw_adp`, `advanced_cw_opt`, `new_enhanced_cw_opt`
- **Other**: `vrptw_ultimate`, `vrptw_high`, `sausage`, `routing_redone`, `simple_ls_zero`

### TIG Challenges (tig-foundation/tig-challenges)

- **URL:** github.com/tig-foundation/tig-challenges
- **Language:** Rust + Python CLI

Explicitly designed as a benchmark suite for AI-driven algorithm discovery frameworks. Provides a `tig.py` CLI with `generate_dataset`, `run_algorithm`, and `evaluate_solutions` commands. The `vehicle_routing/algorithm/mod.rs` file starts as a stub, designed to be filled by discovery agents. The README directly references three frameworks: SkyDiscover, CodeEvolve, and OpenEvolve.

### HGS-CVRP (vidalt/HGS-CVRP)

- **URL:** github.com/vidalt/HGS-CVRP
- **Stars:** 427 | **Language:** C++ | **License:** MIT

The canonical reference implementation of Hybrid Genetic Search by Thibaut Vidal. Key techniques:

- **SWAP* neighborhood**: exchanges customers between routes, inserting each at its best position (not each other's slot). Uses precomputed top-3 insertion positions for O(n\textsuperscript{2}) complexity.
- **Adaptive penalty**: capacity and time window violations treated as soft constraints with dynamically adjusted penalties targeting \textasciitilde43\% feasible solution rate.
- **Dual population**: feasible + infeasible subpopulations with diversity-aware fitness.
- **Granular neighborhoods**: limits search to k=40 nearest customers.
- **SREX crossover**: Selective Route Exchange that preserves complete route structures.

### PyVRP (PyVRP/PyVRP)

- **URL:** github.com/PyVRP/PyVRP
- **Stars:** 624 | **Language:** Python/C++ | **License:** MIT

State-of-the-art HGS-based solver. Published in INFORMS Journal on Computing (2024). Achieves 0.40\% gap on 1000-customer Homberger instances. Key parameter settings from the paper:

| Parameter | Value |
|-----------|-------|
| Population min size | 25 |
| Generation size | 40 |
| Elite count | 4-5 |
| Granular neighborhood k | 40 |
| Initial time warp penalty | 6 |
| Penalty increase factor | 1.34 |
| Penalty decrease factor | 0.32 |
| Repair probability | 80\% |

### reinterpretcat/vrp

- **URL:** github.com/reinterpretcat/vrp
- **Stars:** 485 | **Language:** Rust | **License:** Apache 2.0

The most complete open-source Rust VRP solver. Modular architecture with `vrp-core`, `vrp-scientific` (Solomon/HG format), `vrp-pragmatic`. Uses ruin-and-recreate, population-based search. Available on crates.io. Good reference for local search operators and distance matrix handling patterns in Rust.

## Tier 2: Strong Reference Implementations

### VROOM (VROOM-Project/vroom)

- **URL:** github.com/VROOM-Project/vroom
- **Stars:** 1,720 | **Language:** C++20 | **License:** BSD 2-Clause

Optimized for speed (millisecond solve times). Supports CVRP, VRPTW, PDPTW. Excellent reference for fast local search under time constraints.

### VRPTW-ALNS with SISRs (TimeGone07/VRPTW-ALNS)

- **URL:** github.com/TimeGone07/VRPTW-ALNS
- **Stars:** 35 | **Language:** Python

Implements ALNS + SISRs (Slack Induction by String Removals). Achieves \textasciitilde2.49\% average gap to BKS on 100-customer Solomon instances. The SISRs destroy operator is a proven high-performer worth porting to Rust.

### ALNS\_VRPTW (zll-hust/ALNS\_VRPTW)

- **URL:** github.com/zll-hust/ALNS\_VRPTW
- **Stars:** 75 | **Language:** Java

Another ALNS reference with Homberger instance support.

### AILS-VRP Rust Solver (dguimarans/ails-vrp-rust-solver)

- **URL:** github.com/dguimarans/ails-vrp-rust-solver
- **Language:** Rust | **License:** MIT

Adaptive Iterated Local Search (AILS-II) for generic VRPs. Early stage but a clean Rust implementation.

### LNS-VRP (raviqqe/lns-vrp)

- **URL:** github.com/raviqqe/lns-vrp
- **Language:** Rust | **License:** Unlicense

Clean Large Neighborhood Search implementation with ruin-and-recreate. Good reference for Rust LNS patterns.

## Tier 3: LLM-Powered Evolutionary Discovery Frameworks

These frameworks automate the edit-benchmark-keep/discard loop and are explicitly designed to work with TIG challenges.

### OpenEvolve (algorithmicsuperintelligence/openevolve)

- **URL:** github.com/algorithmicsuperintelligence/openevolve
- **Stars:** 5,992 | **License:** Apache 2.0

Open-source implementation of AlphaEvolve. Island-based parallel evolution, multi-objective Pareto optimization. Supports Rust code evolution. Could drive our `solve_challenge` function directly.

### SkyDiscover (skydiscover-ai/skydiscover)

- **URL:** github.com/skydiscover-ai/skydiscover
- **Stars:** 441 | **License:** Apache 2.0

Introduces AdaEvolve (adapts optimization based on progress) and EvoX (evolves the optimization strategy itself using LLMs). Claims \textasciitilde34\% median improvement over OpenEvolve/GEPA/ShinkaEvolve. Natively supports TIG vehicle\_routing challenge.

### CodeEvolve (inter-co/science-codeevolve)

- **URL:** github.com/inter-co/science-codeevolve
- **Stars:** 75 | **License:** Apache 2.0

Evolutionary coding agent with "inspiration-based crossover" --- architecturally identical to our swarm's inspiration mechanism. Uses MAP-Elites quality-diversity archives.

### ShinkaEvolve (SakanaAI/ShinkaEvolve)

- **URL:** github.com/SakanaAI/ShinkaEvolve
- **Stars:** 1,083 | **License:** Apache 2.0

By Sakana AI, accepted at ICLR 2026. Ships Claude Code skills (`shinka-setup`, `shinka-run`, `shinka-inspect`). Won ICFP 2025 Programming Contest.

### FunSearch (google-deepmind/funsearch)

- **URL:** github.com/google-deepmind/funsearch
- **Stars:** 1,042

The foundational work (Nature 2024) that inspired all the above frameworks. Fork **SperanzaTY/TSP-Funsearch** applies it specifically to TSP.

### EvoControl (QuantaAlpha/EvoControl)

- **URL:** github.com/QuantaAlpha/EvoControl
- **Stars:** 117

Three innovations: diversified planning initialization, feedback-guided genetic evolution with slot-based code decomposition, hierarchical experience memory.

## Tier 4: Autoresearch Ecosystem

### Karpathy's autoresearch (karpathy/autoresearch)

- **URL:** github.com/karpathy/autoresearch
- **Stars:** 74,144

The seminal project: a 630-line setup where an AI agent edits one file, runs for 5 minutes, checks if the metric improved, keeps or discards, repeats. \textasciitilde100 experiments overnight. Our swarm demo follows exactly this pattern.

### Claude Autoresearch Skill (uditgoenka/autoresearch)

- **URL:** github.com/uditgoenka/autoresearch
- **Stars:** 3,832

Generalizes Karpathy's approach as a Claude Code skill. Could potentially be installed in our swarm agents.

### autoresearch-at-home (mutable-state-inc/autoresearch-at-home)

- **URL:** github.com/mutable-state-inc/autoresearch-at-home
- **Stars:** 474

Distributed autoresearch --- SETI@home style multi-agent swarm coordination. The most architecturally comparable project to our swarm demo.

### pi-autoresearch (davebcn87/pi-autoresearch)

- **URL:** github.com/davebcn87/pi-autoresearch
- **Stars:** 5,799

Generalized autoresearch as a Pi extension with live dashboard widget and `/autoresearch` command.

## Key Algorithmic Techniques

Based on the research, these are the highest-impact techniques for improving 30-second, single-threaded scores on 400-node Homberger instances, ordered by expected impact:

### 1. O(1) Move Evaluation via Route Segment Concatenation

The single most impactful implementation technique. In VRPTW, every local search move requires time window feasibility checking, which naively costs O(route\_length) per move. By storing three attributes per route subsequence (duration, cumulative cost, time warp), any move can be evaluated in O(1) via bounded concatenations. This transforms the bottleneck from "how many moves can I evaluate in 30s" to "how smart are my move choices."

### 2. SWAP* Inter-Route Neighborhood

For two routes R1, R2, considers all pairs (u in R1, v in R2) and evaluates swapping them, inserting each at its *best* position in the receiving route (not each other's slot). Uses precomputed top-3 best insertion positions for O(n\textsuperscript{2}) complexity.

### 3. SISR Destroy Operator

Slack Induction by String Removals. Removes multiple consecutive customer sequences from routes that are geographically near each other (Lmax=5). Paired with greedy-blink repair (randomly skipping feasibility checks for controlled randomness). Published by Christiaens and Vanden Berghe (2020), achieving state-of-the-art results without population management.

### 4. Hybrid Genetic Search Architecture

Dual population (feasible + infeasible), SREX crossover (preserves complete routes), intensive local search on every offspring, adaptive penalty management targeting \textasciitilde43\% feasible rate. This is the architecture behind all top VRPTW solvers.

### 5. Granular Neighborhood Pruning

Restrict all insertion and local search candidates to k=30-40 nearest neighbors. Essential for 400-node instances to reduce O(n\textsuperscript{2}) search cost.

### 6. Extended Time Budget

The current solver uses only 4.5 of 30 available seconds. Simply extending the deadline is a free improvement.

## Best-Known Solution Reference

Current best-known values for 400-customer Homberger instances (SINTEF benchmark database):

| Category | Vehicles | Distance Range | Notes |
|----------|----------|----------------|-------|
| C1 (4xx) | 36-40 | 6,803-7,686 | Clustered, tight windows |
| C2 (4xx) | 11-12 | 3,703-4,233 | Clustered, wide windows |
| R1 (4xx) | 36-40 | 7,257-10,372 | Random, tight windows |
| R2 (4xx) | 8 | 4,016-9,210 | Random, wide windows |
| RC1 (4xx) | 36 | 7,309-8,571 | Mixed, tight windows |
| RC2 (4xx) | 8-11 | 3,631-6,706 | Mixed, wide windows |

## Recommendations

### Immediate Actions (No-Regret)

1. **Study TIG monorepo's `hgs_v1` submission** for HGS architecture patterns in Rust with the exact same types
2. **Extend solver deadline** from 4.5s to \textasciitilde27s (free improvement)
3. **Upgrade regret-2 to regret-3** in the repair operator

### Medium-Term (Structural Improvements)

4. **Implement O(1) move evaluation** via segment concatenation
5. **Add SISR destroy** as an ALNS operator
6. **Apply granular neighborhoods** to all search operations, not just destroy seeding

### Advanced (Highest Ceiling)

7. **Implement SWAP*** with top-3 insertion caching
8. **Add population management** with feasible/infeasible subpopulations
9. **Consider integrating OpenEvolve or SkyDiscover** to automate the agent optimization loop

### Framework Integration

10. **Evaluate ShinkaEvolve** --- ships Claude Code skills and won ICFP 2025
11. **Study autoresearch-at-home** for distributed coordination patterns

## Sources

- [TIG Monorepo](https://github.com/tig-foundation/tig-monorepo)
- [TIG Challenges](https://github.com/tig-foundation/tig-challenges)
- [HGS-CVRP by Vidal](https://github.com/vidalt/HGS-CVRP)
- [PyVRP](https://github.com/PyVRP/PyVRP)
- [PyVRP Paper](https://arxiv.org/abs/2403.13795)
- [reinterpretcat/vrp](https://github.com/reinterpretcat/vrp)
- [VROOM](https://github.com/VROOM-Project/vroom)
- [VRPTW-ALNS with SISRs](https://github.com/TimeGone07/VRPTW-ALNS)
- [AILS-VRP Rust Solver](https://github.com/dguimarans/ails-vrp-rust-solver)
- [LNS-VRP Rust](https://github.com/raviqqe/lns-vrp)
- [OpenEvolve](https://github.com/algorithmicsuperintelligence/openevolve)
- [SkyDiscover](https://github.com/skydiscover-ai/skydiscover)
- [CodeEvolve](https://github.com/inter-co/science-codeevolve)
- [ShinkaEvolve](https://github.com/SakanaAI/ShinkaEvolve)
- [FunSearch](https://github.com/google-deepmind/funsearch)
- [EvoControl](https://github.com/QuantaAlpha/EvoControl)
- [Karpathy autoresearch](https://github.com/karpathy/autoresearch)
- [autoresearch-at-home](https://github.com/mutable-state-inc/autoresearch-at-home)
- [SISR Paper](https://pubsonline.informs.org/doi/10.1287/trsc.2019.0914)
- [ALNS Operator Review (2024)](https://www.sciencedirect.com/science/article/pii/S0377221724003928)
- [SINTEF Homberger Benchmark](https://www.sintef.no/projectweb/top/vrptw/homberger-benchmark/400-customers/)
- [PyVRP+ LLM-Evolved Heuristics](https://arxiv.org/html/2604.07872)
