# Host Execution Risk From Server-Supplied `algorithm_code`

Timestamp: 2026-04-18 14:31 America/Los_Angeles

## Question

If an attacker can publish fake `algorithm_code`, can another host end up running it?

## Short Answer

Yes, under this project's documented agent workflow, another host can end up running attacker-controlled code.

But the answer depends on **how** the fake `algorithm_code` enters the system:

- If the fake code becomes another agent's returned `best_algorithm_code`, then **yes**, that host is expected to write it into `src/vehicle_routing/algorithm/mod.rs`, compile it, and execute it during benchmarking.
- If the fake code is only exposed as `inspiration_code`, then **not automatically** under the documented workflow, because agents are instructed to read it for ideas and not write it to `mod.rs`.
- In practice, because agents are autonomous LLM-driven workers rather than a hard-enforced runner, `inspiration_code` is still a weaker but non-zero path to execution if an agent deviates from instructions.

## Why The Answer Is Yes

The repository's documented loop explicitly tells agents to fetch server state and overwrite the local solver source with server-supplied code:

- `CLAUDE.md` instructs agents to call `GET /api/state?agent_id=YOUR_AGENT_ID` ([CLAUDE.md](/home/m/wk/tig-swarm-demo/CLAUDE.md:127))
- It says `best_algorithm_code` is "your own current best code" and to write it to `mod.rs` ([CLAUDE.md](/home/m/wk/tig-swarm-demo/CLAUDE.md:139))
- It then gives the exact command that overwrites the local source file from the server response ([CLAUDE.md](/home/m/wk/tig-swarm-demo/CLAUDE.md:154))

The server really does return `best_algorithm_code` from the database-backed `algorithm_code` field in the agent-specific `/api/state` response:

- `get_state()` loads `my_best["algorithm_code"]` and returns it as `best_algorithm_code` ([server/server.py](/home/m/wk/tig-swarm-demo/server/server.py:320), [server/server.py](/home/m/wk/tig-swarm-demo/server/server.py:377))

After that overwrite, the benchmark harness compiles and executes the local solver on the host:

- `scripts/benchmark.py` runs `cargo build` for `tig_solver` and `tig_evaluator` ([scripts/benchmark.py](/home/m/wk/tig-swarm-demo/scripts/benchmark.py:42))
- It then executes the built solver binary locally with `subprocess.run([solver, ...])` ([scripts/benchmark.py](/home/m/wk/tig-swarm-demo/scripts/benchmark.py:148))

That means the trust chain is:

1. attacker gets fake `algorithm_code` accepted by the coordination server
2. server returns that code as `best_algorithm_code`
3. another host writes it into `mod.rs`
4. another host compiles it
5. another host runs it

That is direct code execution on the recipient host, not just "data exposure."

## When Cross-Host Execution Happens

### Case 1: attacker can publish code as another agent's current best

This is the most serious case. If the attacker's fake code lands in the victim agent's `agent_bests.algorithm_code`, the victim host will pull it via `/api/state?agent_id=VICTIM`, overwrite local source, and run it on the next benchmark cycle.

This is exactly why authenticated agent-private reads and authenticated write endpoints matter, even in a hackathon setting where confidentiality is secondary.

### Case 2: attacker can only publish code under their own agent identity

In that case, the code can still become:

- visible in public APIs and dashboards
- eligible to appear as `inspiration_code` for stagnating agents

But it does **not** automatically become another agent's `best_algorithm_code` under the intended lineage model. Under the documented flow, another agent should read that code for reference only and continue editing its own `mod.rs`.

So this case is lower risk than Case 1, but still not harmless.

### Case 3: attacker-controlled code appears only as `inspiration_code`

`CLAUDE.md` explicitly says:

- save `inspiration_code` to `/tmp/inspiration.rs`
- study it
- do **not** write it to `mod.rs` ([CLAUDE.md](/home/m/wk/tig-swarm-demo/CLAUDE.md:146), [CLAUDE.md](/home/m/wk/tig-swarm-demo/CLAUDE.md:161))

So under the intended workflow, `inspiration_code` is not an automatic execution path.

However, because the consumer is an autonomous coding agent rather than a narrow sandboxed loader, there is still some residual risk that a host could end up executing code copied from inspiration if the agent follows the spirit of "borrow ideas" poorly.

## Security Conclusion

If the goal is to protect host machines, this project does need at least a **minimal integrity/authentication scheme** around code-bearing agent flows.

The minimum bar is not "protect research data." The minimum bar is:

- prevent unauthorized callers from publishing code as another agent
- prevent unauthorized callers from reading another agent's private `best_algorithm_code`
- ensure any host that writes server-supplied code into `mod.rs` is only doing so for code that belongs to that authenticated agent

Without those controls, the coordination server is effectively part of the host-code execution path.

## Practical Decision

For this hackathon project, the right security posture is:

- public read-only dashboard data is acceptable
- public research visibility is acceptable
- unauthenticated cross-host code injection is **not** acceptable

So the project can stay lightweight, but it should still keep:

- authenticated write endpoints
- authenticated agent-private `/api/state` access
- server-derived agent identity on messages/results where integrity matters

That is the smallest scheme that protects hosts while still keeping the project open and educational.
