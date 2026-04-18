#!/usr/bin/env python3
"""Publish benchmark results to the swarm coordination server.

Usage:
    python3 scripts/benchmark.py 2>/dev/null \
      | python3 scripts/publish.py AGENT_ID AGENT_TOKEN "title" "description" strategy_tag "notes"

AGENT_TOKEN is the credential returned by /api/agents/register.
"""

import json
import os
import sys
import urllib.request
from pathlib import Path

SERVER = "https://demo.discoveryatscale.com"
ALGO_PATH = Path(__file__).parent.parent / "src/vehicle_routing/algorithm/mod.rs"


def main():
    if len(sys.argv) < 6:
        print(
            "Usage: python3 scripts/publish.py <agent_id> <agent_token> <title> <description> <strategy_tag> [notes]",
            file=sys.stderr,
        )
        sys.exit(1)

    agent_id = sys.argv[1]
    agent_token = sys.argv[2]
    title = sys.argv[3]
    description = sys.argv[4]
    strategy_tag = sys.argv[5]
    notes = sys.argv[6] if len(sys.argv) > 6 else ""

    bench = json.load(sys.stdin)
    code = ALGO_PATH.read_text()

    payload = {
        "agent_id": agent_id,
        "agent_token": agent_token,
        "title": title,
        "description": description,
        "strategy_tag": strategy_tag,
        "algorithm_code": code,
        "score": bench["score"],
        "feasible": bench["feasible"],
        "num_vehicles": bench["num_vehicles"],
        "total_distance": bench.get("total_distance", bench["score"]),
        "notes": notes,
        "route_data": bench.get("route_data"),
    }

    req = urllib.request.Request(
        f"{SERVER}/api/iterations",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )

    with urllib.request.urlopen(req) as resp:
        result = json.load(resp)
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
