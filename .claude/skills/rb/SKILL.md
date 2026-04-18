---
name: rb
description: Run benchmark and record timestamped results to docs/benchmark-history.jsonl with score, feasibility, vehicle count, commit hash, and change description
---

# RB (Record Benchmark)

Run the VRPTW solver benchmark and append a timestamped result to the history log.

## Usage

```
/rb [description of what changed]
```

If no description is provided, derive one from the most recent commit message.

## Process

### 1. Run the Benchmark

```bash
BENCH=$(python3 scripts/benchmark.py 2>/dev/null)
```

Extract the summary fields:

```bash
echo "$BENCH" | python3 -c "
import sys,json
d=json.load(sys.stdin)
print(f'Score: {d[\"score\"]}, Feasible: {d[\"feasible\"]}, Vehicles: {d[\"num_vehicles\"]}, Solved: {d[\"instances_solved\"]}')
"
```

### 2. Record the Result

Append a JSON line to `docs/benchmark-history.jsonl`:

```bash
COMMIT=$(git rev-parse --short HEAD)
BRANCH=$(git branch --show-current)
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
DESCRIPTION="<from argument or recent commit>"

echo "$BENCH" | python3 -c "
import sys, json
d = json.load(sys.stdin)
entry = {
    'timestamp': '$TIMESTAMP',
    'commit': '$COMMIT',
    'branch': '$BRANCH',
    'score': d['score'],
    'total_distance': d['total_distance'],
    'num_vehicles': d['num_vehicles'],
    'feasible': d['feasible'],
    'instances_solved': d['instances_solved'],
    'instances_feasible': d['instances_feasible'],
    'instances_infeasible': d['instances_infeasible'],
    'description': '$DESCRIPTION'
}
print(json.dumps(entry))
" >> docs/benchmark-history.jsonl
```

### 3. Report

Print a summary table comparing to the previous entry (if any):

```
Benchmark recorded:
  Score:    7414.42
  Feasible: 24/24
  Vehicles: 620
  Commit:   6ee55a9
  Delta:    -3.2% vs previous
```

## Notes

- The benchmark builds solver+evaluator and runs 24 HG 400-node instances in parallel (30s timeout each)
- Scoring: `(sum_feasible_distances + num_infeasible * 1_000_000) / num_instances` — lower is better
- Results are appended to `docs/benchmark-history.jsonl` (one JSON object per line)
- Do NOT commit the benchmark history automatically — let the user decide when to commit
