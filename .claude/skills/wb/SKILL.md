---
name: wb
description: Implement a VRPTW optimization technique in a git worktree, benchmark it, and report the score delta back
---

# WB (Worktree Bench)

Implement an optimization technique from a beads issue in an isolated git worktree, run the full benchmark, commit results, and report the score delta compared to the baseline.

## Usage

```
/wb <issue-id>
/wb tig-swarm-demo-gk7
```

## Tmux Dispatch Rule

If a coordinator dispatches `/wb` to a tmux-backed agent, the coordinator must send the `/wb ...` text and then send `Cmd+m` to submit it. Do not rely on `Enter` alone.

## Arguments

- `issue-id`: The beads issue ID containing the optimization technique to implement

## Process

### 1. Establish Baseline

Before creating the worktree, run the benchmark on the current algorithm to get a baseline score:

```bash
BASELINE=$(python3 scripts/benchmark.py 2>/dev/null)
BASELINE_SCORE=$(echo "$BASELINE" | python3 -c "import sys,json; print(json.load(sys.stdin)['score'])")
echo "Baseline score: $BASELINE_SCORE"
```

If a recent baseline exists in `docs/benchmark-history.jsonl` (last entry less than 1 hour old and same commit), use that instead of re-running.

### 2. Read the Issue

```bash
br show <issue-id>
```

Extract the technique description and implementation details. Cross-reference with `docs/reports/2026-04-18-165200-vrptw-optimization-literature-review.md` for full technical context on each technique.

### 3. Create Worktree

```bash
BRANCH="<issue-id>/benchmark"
git worktree add ../tig-swarm-demo-<issue-id> -b $BRANCH
cd ../tig-swarm-demo-<issue-id>
```

### 4. Implement the Technique

**ONLY edit `src/vehicle_routing/algorithm/mod.rs`** -- no other source files.

Read the current algorithm file, understand its structure, and implement the optimization technique described in the issue. Follow the constraints:

- Single-threaded only (no rayon, crossbeam, async)
- Must call `save_solution()` incrementally
- 30-second timeout per instance
- Must compile with `cargo build -r --bin tig_solver --features solver,vehicle_routing`

### 5. Build and Verify

```bash
cargo build -r --bin tig_solver --features solver,vehicle_routing
cargo build -r --bin tig_evaluator --features evaluator,vehicle_routing
```

Fix any compilation errors before proceeding.

### 6. Run Benchmark

```bash
BENCH=$(python3 scripts/benchmark.py 2>/dev/null)
echo "$BENCH" | python3 -c "
import sys,json
d=json.load(sys.stdin)
print(f'Score: {d[\"score\"]}, Feasible: {d[\"feasible\"]}, Vehicles: {d[\"num_vehicles\"]}')
"
```

### 7. Save Results

```bash
mkdir -p docs/benchmark-results
echo "$BENCH" | python3 -c "
import sys,json
d=json.load(sys.stdin)
d['issue_id'] = '<issue-id>'
d['baseline_score'] = $BASELINE_SCORE
d['delta'] = d['score'] - $BASELINE_SCORE
d['delta_pct'] = (d['score'] - $BASELINE_SCORE) / $BASELINE_SCORE * 100
json.dump(d, sys.stdout, indent=2)
" > docs/benchmark-results/<issue-id>.json
```

### 8. Commit in Worktree

```bash
git add src/vehicle_routing/algorithm/mod.rs docs/benchmark-results/<issue-id>.json
git commit -m "Benchmark: <technique name> (<issue-id>)

Score: <new_score> (baseline: <baseline_score>, delta: <delta>)
Feasible: <yes/no>"
```

### 9. Report Back

Print a summary table:

```
## Benchmark Results: <issue-id>

| Metric | Baseline | New | Delta |
|--------|----------|-----|-------|
| Score | <baseline> | <new> | <delta> (<pct>%) |
| Feasible | <yes/no> | <yes/no> | - |
| Vehicles | <baseline_v> | <new_v> | <delta_v> |

Branch: <branch-name>
Worktree: ../tig-swarm-demo-<issue-id>
```

If the score improved (delta < 0), note it as a candidate for merging into main.

### 10. Update Issue

```bash
br update <issue-id> --status done
br comment <issue-id> "Benchmark complete. Score: <new> (delta: <delta>, <pct>%)"
```

## Technique Reference

The following issues map to sections in the literature review report:

| Issue | Technique | Report Section |
|-------|-----------|---------------|
| P1 (gk7) | SISRs string removal | 1.2.1 |
| P2 (rm3) | Time-warp penalties | 2.1 |
| P3 (yj7) | SWAP* neighborhood | 2.2 |
| P4 (bdu) | O(1) concatenation move eval | 3.2 |
| P5 (1yj) | Regret-3/4 insertion | 1.3.1 |
| P6 (u1p) | Granular neighborhoods | 3.1 |
| P7 (giq) | Or-opt-2/3 segments | 3.3 |
| P8 (pls) | Ensemble destroy operators | 4.3 |
| P9 (5u3) | Adaptive destroy size | 5.2 |
| P10 (79o) | Instance-adaptive params | 5.4 |
| P11 (8pv) | Multi-start construction | 5.1 |
| P12 (9q0) | Elite archive recombination | 2.3 |

## Notes

- Lower scores are better (distance-based metric)
- A score above 41,666 means at least one instance is infeasible
- Each infeasible instance adds 1,000,000 to the numerator
- The worktree is left in place for inspection; clean up manually with `git worktree remove`
- If build fails, fix compilation errors before benchmarking -- do not skip
- Always compare against the same baseline to ensure fair comparison
- Use `/rb` (the existing skill) to record benchmark results in the main worktree
