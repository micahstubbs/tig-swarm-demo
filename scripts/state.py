#!/usr/bin/env python3
"""Read and merge state from every host in ~/.tig-swarm/hosts.json.

Writes merged best_algorithm_code to src/vehicle_routing/algorithm/mod.rs.
Saves per-host inspiration to /tmp/inspiration-<host-tag>.rs when provided.
Prints a human summary to stdout.
"""
import json
import sys
from pathlib import Path

# Allow running from project root or scripts/ dir
sys.path.insert(0, str(Path(__file__).resolve().parent))
import tig_client as tc

ALGO = Path(__file__).resolve().parent.parent / "src/vehicle_routing/algorithm/mod.rs"


def host_tag(host: str) -> str:
    """Map host URL to a short tag.

    https://tigswarmdemo.com         -> "tig"
    https://demo.discoveryatscale.com -> "das"
    anything else                    -> first label of the hostname
    """
    h = host.replace("https://", "").replace("http://", "").rstrip("/")
    return {
        "tigswarmdemo.com":           "tig",
        "demo.discoveryatscale.com":  "das",
    }.get(h, h.split(".")[0])


def main():
    hosts = tc.resolve_hosts()

    # Build list of (host, agent_id) for hosts that have credentials
    credentialed = [(h, tc.creds_for(h)["agent_id"]) for h in hosts if tc.creds_for(h)]

    if not credentialed:
        print("[err] no hosts have credentials — run scripts/register.py first", file=sys.stderr)
        sys.exit(1)

    results = tc.parallel_requests(
        credentialed,
        method="GET",
        path="/api/state",
    )

    ok   = {h: s for h, s in results.items() if not isinstance(s, Exception)}
    errs = {h: s for h, s in results.items() if isinstance(s, Exception)}

    for h, exc in errs.items():
        print(f"[warn] {h}: {exc}", file=sys.stderr)

    if not ok:
        print("[err] no hosts reachable", file=sys.stderr)
        sys.exit(1)

    # ------------------------------------------------------------------ #
    # Best-lineage pick: host with the lowest my_best_score               #
    # ------------------------------------------------------------------ #
    best_host, best_state = min(
        ok.items(),
        key=lambda kv: (
            kv[1].get("my_best_score")
            if kv[1].get("my_best_score") is not None
            else float("inf")
        ),
    )

    # ------------------------------------------------------------------ #
    # Union hypotheses — dedupe by (title, strategy_tag)                  #
    # ------------------------------------------------------------------ #
    seen        = set()
    hypotheses  = []
    for s in ok.values():
        for h in s.get("recent_hypotheses") or []:
            key = (h.get("title"), h.get("strategy_tag"))
            if key in seen:
                continue
            seen.add(key)
            hypotheses.append(h)
    hypotheses.sort(key=lambda h: h.get("created_at") or "", reverse=True)
    hypotheses = hypotheses[:20]

    # ------------------------------------------------------------------ #
    # Global best (min across sites)                                       #
    # ------------------------------------------------------------------ #
    scores      = [s.get("best_score") for s in ok.values() if s.get("best_score") is not None]
    global_best = min(scores) if scores else None

    # ------------------------------------------------------------------ #
    # Stagnation: take the best (min) across sites                         #
    # ------------------------------------------------------------------ #
    stag_vals  = [
        s.get("my_runs_since_improvement")
        for s in ok.values()
        if s.get("my_runs_since_improvement") is not None
    ]
    stagnation = min(stag_vals) if stag_vals else 0

    # ------------------------------------------------------------------ #
    # Activity counters: sum across sites                                  #
    # ------------------------------------------------------------------ #
    my_runs         = sum(s.get("my_runs") or 0 for s in ok.values())
    my_improvements = sum(s.get("my_improvements") or 0 for s in ok.values())

    # ------------------------------------------------------------------ #
    # Per-host inspiration files                                           #
    # ------------------------------------------------------------------ #
    inspiration_files = []
    for host, s in ok.items():
        code = s.get("inspiration_code")
        if code:
            tag  = host_tag(host)
            dest = Path(f"/tmp/inspiration-{tag}.rs")
            dest.write_text(code)
            inspiration_files.append(str(dest))

    # ------------------------------------------------------------------ #
    # Tagged leaderboard: merge all hosts, tag agent_name with @<tag>     #
    # ------------------------------------------------------------------ #
    board = []
    for host, s in ok.items():
        tag = host_tag(host)
        for entry in s.get("leaderboard") or []:
            e2 = dict(entry)
            e2["agent_name"] = f"{entry.get('agent_name')}@{tag}"
            board.append(e2)
    board.sort(key=lambda e: e.get("score") if e.get("score") is not None else float("inf"))

    # ------------------------------------------------------------------ #
    # Write algorithm code from best-lineage host                         #
    # ------------------------------------------------------------------ #
    if best_state.get("best_algorithm_code"):
        ALGO.write_text(best_state["best_algorithm_code"])

    # ------------------------------------------------------------------ #
    # Human summary                                                        #
    # ------------------------------------------------------------------ #
    print(f"best lineage: {host_tag(best_host)} (score={best_state.get('my_best_score')})")
    print(f"global best:  {global_best}")
    print(f"runs: {my_runs} · improvements: {my_improvements} · stagnation: {stagnation}")
    print(f"recent hypotheses: {len(hypotheses)}")
    if inspiration_files:
        print(f"inspiration files: {', '.join(inspiration_files)}")
    else:
        print("inspiration files: (none)")
    print("top 5 leaderboard:")
    for entry in board[:5]:
        print(f"  {entry['agent_name']}: {entry.get('score')}")


if __name__ == "__main__":
    main()
