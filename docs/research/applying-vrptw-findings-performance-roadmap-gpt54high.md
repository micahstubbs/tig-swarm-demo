# Applying the VRPTW Findings to This Project

## Purpose

This document translates the recommendations in [2026-04-18-142524-vrptw-solver-improvements.md](./2026-04-18-142524-vrptw-solver-improvements.md) into a project-specific performance roadmap for `tig-swarm-demo`.

The goal here is not to restate the literature. It is to decide what this repository should actually do next to improve benchmark score on the 24 Homberger 400-node instances under the current constraints:

- single-threaded solver logic in [`src/vehicle_routing/algorithm/mod.rs`](../../src/vehicle_routing/algorithm/mod.rs)
- 30-second per-instance timeout in [`scripts/benchmark.py`](../../scripts/benchmark.py)
- swarm workflow where agents repeatedly overwrite `mod.rs`, benchmark, and publish results
- strong penalty for infeasible instances, so anytime feasible solutions still matter

## Bottom Line

The research report's main conclusion is correct for this codebase: the project should not aim for a full HGS-VRPTW or LKH-style rewrite first. It should aim for a **mini-HGS trajectory** built from a few high-leverage ideas that fit the current architecture:

1. use almost all of the 30-second budget
2. stop spending O(route length) work on every move attempt
3. prune candidate neighborhoods aggressively
4. replace weak VRPTW-hostile operators with VRPTW-native ones
5. add one or two destroy/repair operators that materially change search behavior

In practical terms, that means:

- first: `P0 + P5 + P9/P11`
- next: `P2`
- then: `P1`
- then: `P3 + P4`

That sequence gives the best score-per-engineering-hour for this repository.

## Why The Findings Matter Here

The research report identified four current bottlenecks, and they all map directly to live code:

### 1. The solver is quitting far too early

[`solve_challenge()`](../../src/vehicle_routing/algorithm/mod.rs) sets:

- `deadline = start + 4500 ms`
- `alns_deadline = start + 3200 ms`
- `ls_deadline = start + 800 ms`

even though the benchmark harness gives each solver process **30 seconds** before timeout in [`scripts/benchmark.py`](../../scripts/benchmark.py).

This is unusually important in this project because the harness already runs all 24 instances in parallel. Extending internal search from 4.5 seconds to roughly 27 seconds does **not** make the total benchmark wall clock 6x slower. It mostly means each per-instance worker uses the time it already has.

That makes the deadline change the highest-ROI improvement in the entire repo.

### 2. The inner loop is dominated by repeated full feasibility scans

The solver still evaluates candidate moves by cloning routes and calling [`is_feasible()`](../../src/vehicle_routing/algorithm/mod.rs), which scans the whole route forward. This affects:

- greedy insertion
- regret insertion
- intra-route 2-opt
- relocate
- exchange
- Or-opt
- cross-2-opt
- merge-to-fleet

That is exactly the kind of O(n)-per-move overhead the research report flags. On 400-node instances, this is not a minor inefficiency. It is the difference between thousands and tens of thousands of useful move evaluations.

### 3. The current neighborhood search is much too broad

The code already computes `shaw_neighbors`, but only uses them for destroy seeding. It does **not** use them to prune:

- insertion positions in `greedy_insertion()`
- route choices in `regret_insertion()`
- local-search move candidates in the SA phase

So the solver pays for broad O(routes x positions) scans while already holding a structure that could cut most of that work.

### 4. One major local-search operator is a bad fit for VRPTW

The current early local search and SA phase both rely on segment reversal:

- `two_opt_route()`
- `try_intra_2opt()`

For VRPTW, reversing a segment often destroys arrival-time structure. The report is right that this is usually a poor trade. This project should prefer `2-opt*` tail swapping between routes over classical intra-route reversal.

## Phase 1: Immediate no-regret gains

### A. Extend the internal deadline to approximately 27 seconds

This should be the first change shipped.

Why it fits this repo:

- trivial code change
- no architectural risk
- no need to change the swarm protocol
- directly exploits the benchmark harness as written

Guardrail:

- keep early `save_solution()` calls after construction and every new best solution
- do not move to a late-save strategy, because infeasible or timeout runs are still catastrophic in this scoring setup

Expected effect:

- materially lower score even before any sophisticated operator work

### B. Replace intra-route 2-opt with 2-opt*

This is the second change that should happen, not because it is the biggest possible gain, but because it removes a structurally weak operator from both the quick local-search phase and the SA phase.

Why it fits this repo:

- small surface area
- local change inside `mod.rs`
- directly addresses a move that is likely burning iterations for little return

Expected effect:

- better use of search time
- more feasible improving moves late in the run

### C. Fix the obvious bookkeeping inefficiencies

The report's "small but cheap" recommendations belong here:

- replace linear membership checks in `shaw_removal()` with a bitset
- remove the repeated "build remaining indices" pattern in `worst_removal()`
- track ALNS route distances incrementally instead of recomputing `penalized_cost()` from scratch

These will not transform quality on their own, but they are cheap and reduce wasted work before the bigger refactor.

## Phase 2: Prune the search before reinventing it

### D. Reuse `shaw_neighbors` as a general candidate list

This should happen before any complex data-structure work.

Reasoning:

- it is already present in the code
- it gives a large fraction of the benefit of granular search with much lower implementation risk
- it constrains later features like SWAP* and SISR repair in a natural way

Concretely, this project should:

- only try insertions adjacent to a customer's nearest-neighbor set
- only consider route targets containing one of those neighbors
- restrict inter-route local-search pair selection to routes/customers that are geographically plausible partners

This is the point where the solver starts to look less like a generic ALNS toy and more like a practical VRPTW engine.

### E. Upgrade regret-2 to regret-3 before adding many new operators

The report notes that regret-3 is often stronger on VRPTW. In this project, regret-3 is a good intermediate step because:

- it improves repair quality without large conceptual change
- it helps immediately once neighborhood pruning exists
- it gives a stronger baseline before SISR/blink are added

I would do this before adding a large operator zoo.

## Phase 3: Structural speedup in the hot path

### F. Add concatenation-based O(1) route-segment evaluation

This is the most important "real engineering" change in the roadmap.

It is also the point where the codebase stops being a small heuristic script and becomes a proper solver core.

Why it matters here:

- nearly every meaningful operation in the current code is bottlenecked by `is_feasible()`
- the benchmark instances are large enough that route-level scans dominate runtime
- once the solver uses the full time budget, this bottleneck becomes even more costly

What "applying the finding" means in this repo:

- keep the external swarm workflow unchanged
- keep the solver single-threaded
- keep everything in `mod.rs` initially if needed
- introduce route/segment helper structs solely to eliminate repeated scan-based move testing

This should not be attempted as a full architecture rewrite. The right approach is narrower:

1. introduce route metadata and concatenation helpers
2. port one operator first, likely relocate
3. fuzz-check O(1) evaluation against `is_feasible()`
4. port exchange / Or-opt / 2-opt* after the invariant is trusted

That staged migration matters because a silent feasibility bug will poison all benchmark results.

## Phase 4: Add the operators that change search behavior, not just speed

### G. Add SISR destroy plus greedy-blink repair

Once the solver uses the full deadline and the search is pruned, SISR is the best next operator addition.

Why it fits this project especially well:

- the benchmark mix includes clustered categories where slack-preserving removal should help
- it is much smaller than adding a full population-based HGS layer
- it gives qualitatively different search trajectories than the current destroy set

Important implementation note for this repo:

SISR should be added as a selective ALNS operator, not as a wholesale replacement for current destroy operators. The adaptive-weight framework already exists, so the project can let the benchmark tell it when SISR is paying off.

### H. Add SWAP*

SWAP* belongs after neighborhood pruning and ideally after O(1) evaluation.

Why:

- in the current scan-heavy implementation, SWAP* would likely be too expensive
- with candidate lists, it becomes an efficient endgame intensification move
- it directly addresses the gap between "simple exchange" and the stronger cross-route moves used by stronger solvers

This is one of the clearest examples of a research finding that should be applied here only after the enabling infrastructure is in place.

## What Should Not Be Prioritized Yet

### Full HGS population management

The report is right that HGS-VRPTW is state of the art, but this repository is not ready for a first-principles HGS rewrite. It would add:

- giant-tour representation
- split decoder
- crossover
- population diversity tracking
- feasibility-penalty adaptation

That is a lot of surface area for a swarm project whose agents currently only modify one solver file.

The project should instead take the HGS pieces with the highest standalone ROI:

- candidate pruning
- O(1) move evaluation
- SWAP*
- better ruin-and-recreate

### LKH-style machinery

This is even less appropriate for the current project. LKH ideas are useful as inspiration, but the implementation burden is far too high for the likely payoff under the existing workflow.

### RL operator selection

Not yet. The current operator set and evaluation engine are still too weak for PPO-style selection to be the bottleneck. Better search mechanics will dominate any gains from smarter operator scheduling.

## Recommended Order For This Repository

If the goal is to improve score quickly while keeping the project stable, the implementation order should be:

1. extend solver deadline to approximately 27 seconds
2. replace intra-route 2-opt with 2-opt*
3. bitset and incremental-cost cleanups
4. use neighbor lists to prune insertion and local search
5. upgrade regret-2 to regret-3
6. implement concatenation-based O(1) evaluation for relocate first
7. extend O(1) evaluation to exchange, Or-opt, and 2-opt*
8. add SISR destroy
9. add greedy-blink repair
10. add SWAP*
11. only then consider LAHC, Split construction, or population ideas

This order matters because it compounds correctly:

- more time only helps if moves are not wasted
- richer operators only help if they can be evaluated cheaply
- fancy metaheuristics only help after the move engine is strong

## How To Measure Progress In This Project

The benchmark design changes what "good progress" means.

Because [`scripts/benchmark.py`](../../scripts/benchmark.py) averages score across 24 fixed instances and applies a massive infeasibility penalty, the project should treat improvements in this order:

1. keep `instances_feasible == 24`
2. lower `total_distance`
3. lower average score by category, not just overall
4. only after that worry about elegance or architectural purity

For this repo, every serious change should be evaluated with an ablation mindset:

- benchmark before
- change one core behavior
- benchmark after
- record which instance families improved or regressed

That is especially important because some recommendations in the literature are category-sensitive:

- clustered instances should benefit more from SISR and zone-like destroy
- random instances may benefit more from broader diversify-and-repair behavior
- neighbor pruning that is too aggressive can hurt insertion quality on sparse cases

## The Real Target: A Mini-HGS Solver Inside The Existing Workflow

The best way to apply the report is not "implement HGS-VRPTW." It is:

> Turn the current hybrid ALNS into a mini-HGS-style engine using the same benchmark harness and same swarm workflow.

That target solver would look like this:

- feasible construction plus early save
- nearly full 30-second search budget
- granular candidate lists
- O(1) route-segment evaluation
- regret-3 plus greedy-blink repair
- SISR destroy
- relocate, Or-opt, 2-opt*, exchange, and SWAP* as the main neighborhoods
- optional LAHC or lightly tuned SA on top

That is realistic for this project. A full HGS or LKH port is not.

## Recommended Next Milestone

If the team wants the highest-confidence performance milestone for the next round of work, it should be:

1. extend the deadline
2. remove intra-route reversal as a central operator
3. add granular candidate pruning
4. benchmark

If that lands cleanly, the next milestone should be:

1. port relocate to O(1) evaluation
2. benchmark
3. port the rest of local search
4. add SISR plus blink repair

That path matches both the literature and the actual structure of this repository.
