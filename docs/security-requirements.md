# Security Requirements

Timestamp: 2026-04-18 14:36 America/Los_Angeles

## Primary Requirement

This project prioritizes **protecting participant host machines** over protecting research data produced by the demo.

That means:

- public dashboard visibility is acceptable
- public research/history visibility is acceptable when useful for the demo
- unauthenticated cross-host code injection is **not** acceptable

## Why This Requirement Exists

In the documented agent workflow, `best_algorithm_code` returned by the server is
written into `src/vehicle_routing/algorithm/mod.rs`, then compiled and executed
locally during benchmarking. That makes agent-private code fetches part of the
host code-execution path, not just a read-only data path.

Source references:

- agent workflow writes `best_algorithm_code` into `mod.rs`: [CLAUDE.md](/home/m/wk/tig-swarm-demo/CLAUDE.md:139), [CLAUDE.md](/home/m/wk/tig-swarm-demo/CLAUDE.md:154)
- server returns agent-private `best_algorithm_code`: [server/server.py](/home/m/wk/tig-swarm-demo/server/server.py:328), [server/server.py](/home/m/wk/tig-swarm-demo/server/server.py:377)
- benchmark compiles and runs the solver locally: [scripts/benchmark.py](/home/m/wk/tig-swarm-demo/scripts/benchmark.py:42), [scripts/benchmark.py](/home/m/wk/tig-swarm-demo/scripts/benchmark.py:148)

## Minimum Controls Required

To satisfy the host-protection requirement, the project must keep at least these controls:

- authenticated agent write endpoints
- authenticated agent-private `/api/state` access when `agent_id` is supplied
- prevention of publishing code as another agent
- prevention of reading another agent's private `best_algorithm_code` and `inspiration_code`
- server-derived identity for messages/results where integrity matters

## Controls That May Remain Relaxed

The following can remain lightweight or public for the hackathon/demo use case:

- public read-only dashboard data
- public replay/history data
- public visibility into solver progress and research artifacts
- absence of heavyweight user accounts, OAuth, or RBAC

## Decision Boundary

If a server response can cause a participant machine to overwrite local solver
source and later execute it, that response is part of the host-execution trust
boundary and must be authenticated.
