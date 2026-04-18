#!/usr/bin/env python3
"""Publish benchmark results to every host in ~/.tig-swarm/hosts.json.

Usage:
    python3 scripts/benchmark.py 2>/dev/null \
      | python3 scripts/publish.py "<title>" "<description>" <strategy_tag> ["notes"]

Credentials come from ~/.tig-swarm/hosts.json (populated by scripts/register.py).

Single-host override:
    TIG_SERVER_URL=https://demo.discoveryatscale.com python3 scripts/publish.py ...
"""
import json
import sys
from pathlib import Path

# Allow running from project root or scripts/ dir
sys.path.insert(0, str(Path(__file__).resolve().parent))
import tig_client as tc

ALGO_PATH = Path(__file__).resolve().parent.parent / "src/vehicle_routing/algorithm/mod.rs"


def main():
    if len(sys.argv) < 4:
        print(
            "Usage: python3 scripts/publish.py <title> <description> <strategy_tag> [notes]",
            file=sys.stderr,
        )
        print(
            "  Reads benchmark JSON from stdin.",
            file=sys.stderr,
        )
        sys.exit(1)

    title        = sys.argv[1]
    description  = sys.argv[2]
    strategy_tag = sys.argv[3]
    notes        = sys.argv[4] if len(sys.argv) > 4 else ""

    bench = json.load(sys.stdin)
    code  = ALGO_PATH.read_text()

    # Shared payload (without agent creds — added per-host below)
    shared = {
        "title":          title,
        "description":    description,
        "strategy_tag":   strategy_tag,
        "algorithm_code": code,
        "score":          bench["score"],
        "feasible":       bench["feasible"],
        "num_vehicles":   bench["num_vehicles"],
        "total_distance": bench.get("total_distance", bench["score"]),
        "notes":          notes,
        "route_data":     bench.get("route_data"),
    }

    hosts      = tc.resolve_hosts()
    primary    = tc.primary()
    errors     = {}
    successes  = {}

    for host in hosts:
        creds = tc.creds_for(host)
        if not creds:
            print(f"[warn] {host}: no credentials — run scripts/register.py first", file=sys.stderr)
            errors[host] = RuntimeError("missing credentials")
            continue

        payload = dict(shared)
        payload["agent_id"]    = creds["agent_id"]
        payload["agent_token"] = creds["agent_token"]

        try:
            result = tc.post(host, "/api/iterations", payload)
            iteration_id = result.get("iteration_id", result.get("id", "?"))
            improved     = result.get("improved", result.get("is_improvement", "?"))
            print(f"[ok]   {host}: iteration {iteration_id}, improved={improved}")
            successes[host] = result
        except Exception as e:
            print(f"[warn] {host}: {e}", file=sys.stderr)
            errors[host] = e

    # Exit policy
    if primary not in successes:
        if primary in errors:
            print(f"[err]  primary host {primary} failed — exiting 1", file=sys.stderr)
        else:
            print(f"[err]  primary host {primary} had no credentials — exiting 1", file=sys.stderr)
        sys.exit(1)

    if errors:
        non_primary_errors = {h: e for h, e in errors.items() if h != primary}
        if non_primary_errors:
            print(
                f"[warn] {len(non_primary_errors)} non-primary host(s) failed; primary succeeded",
                file=sys.stderr,
            )

    sys.exit(0)


if __name__ == "__main__":
    main()
