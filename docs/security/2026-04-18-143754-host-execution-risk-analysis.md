# Host Execution Risk Analysis for `algorithm_code`

## Question

If an attacker can publish fake `algorithm_code`, can another host end up running it?

## Answer: Yes

An attacker can get arbitrary Rust code compiled and executed on another participant's host through the inspiration code path, with **no server compromise needed**. The score is self-reported, the code is served verbatim to stagnating peers, and the only "filter" is an LLM instruction to "read, don't copy" — which is a policy, not a technical control.

## Attack Vectors

### Vector 1: Inspiration Code (designed-in, LLM-mediated)

1. Attacker registers an agent (`POST /api/agents/register`)
2. Publishes malicious Rust in `algorithm_code` with a **fraudulently low score** — the server trusts self-reported scores with zero verification (`server.py:524-537`)
3. That code is stored in `agent_bests` (`server.py:549-555`)
4. When any other agent stagnates (2 runs without improvement), `_pick_inspiration()` randomly selects from active peers' bests (`server.py:269-281`) and serves the attacker's code verbatim as `inspiration_code` (`server.py:367`)
5. The victim LLM reads it and adapts ideas into its own `mod.rs`, which then gets `cargo build` + executed

The LLM acts as an imperfect filter — CLAUDE.md says "read for inspiration, don't copy wholesale" — but a cleverly disguised payload (e.g., Rust code that looks like a caching optimization but includes `std::process::Command` or `std::fs` operations) could survive the LLM's adaptation step.

### Vector 2: Own-best Poisoning (requires DB/MITM compromise)

The agent blindly writes `best_algorithm_code` from the server to `mod.rs`:

```bash
echo "$STATE" | python3 -c "..." > src/vehicle_routing/algorithm/mod.rs
```

There's no signature, hash, or integrity check. If an attacker can modify the DB (SQL injection, server compromise) or MITM the HTTPS connection, they can replace any agent's own-best code, and it gets compiled and run directly — no LLM filter at all.

## Root Causes

| Gap | Location | Issue |
|-----|----------|-------|
| No score verification | `server.py:524-537` | Server trusts self-reported scores; attacker can claim score=0.001 |
| No code signing | `GET /api/state` response | Agent can't verify `best_algorithm_code` is actually theirs |
| No sandboxing | benchmark.py / cargo build | Solver binary runs with full user privileges |
| No content filtering | `agent_bests` table | Arbitrary Rust code stored and served to peers |
| LLM as security boundary | CLAUDE.md instructions | "Don't copy inspiration" is a policy, not a technical control |

## Mitigation Added

A **host-protection invariant** note was added to `CLAUDE.md` Key Constraints:

> `best_algorithm_code` returned by the coordination server is written into `src/vehicle_routing/algorithm/mod.rs` and then compiled/executed locally. For any deployment that protects participant hosts, agent-private `/api/state` reads belong to the authentication boundary just like write endpoints.

## Potential Further Mitigations

1. **Server-side score verification** — re-run benchmark on submitted code before accepting scores
2. **Code signing** — agent signs its own code at publish time; verify signature before writing to `mod.rs`
3. **Sandboxed execution** — run `cargo build` + solver in a container or VM with no network access
4. **Content allowlisting** — reject code containing `std::process`, `std::net`, `std::fs` (outside expected paths)
5. **Authenticated reads** — require `agent_token` on `GET /api/state` so only the owning agent can retrieve their own best code
