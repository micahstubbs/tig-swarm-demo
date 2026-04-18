# Security Audit: `tig-swarm-demo`

Timestamp: 2026-04-18 14:09 America/Los_Angeles

## Scope and Method

I reviewed the Rust solver/evaluator/generator entrypoints, the FastAPI coordination server, the dashboard frontend, build/runtime configuration, and the project dependencies. I combined manual source review with local verification runs:

- `cargo test`
- `npm audit --prefix dashboard --omit=dev --json`
- `pip-audit -r server/requirements.txt -f json` in a temporary virtualenv
- `cargo audit --json`

The highest-risk issues are in the coordination server and dashboard, not in the Rust challenge binaries.

## Executive Summary

The project has multiple remotely exploitable application-layer security failures. The most serious problems are:

1. A hardcoded/default admin key combined with publicly reachable admin endpoints and wildcard CORS.
2. No agent authentication, allowing arbitrary agent impersonation and direct tampering with swarm state.
3. Stored XSS in the dashboard and ideas feed through unsanitized `innerHTML` rendering of user-controlled content.

In its current form, a network client that can reach the server can reset the swarm, spoof agents, falsify leaderboard results, poison stored state, and execute script in other users' browsers.

## Findings

### 1. Critical: Hardcoded admin secret enables full remote takeover

**Evidence**

- Default admin key is seeded as a literal string in [server/db.py](../../server/db.py#L107) through [server/db.py](../../server/db.py#L110).
- Admin verification falls back to the same literal value in [server/server.py](../../server/server.py#L80) through [server/server.py](../../server/server.py#L83).
- The bootstrap script also contains the same literal secret in [scripts/bootstrap_seed.py](../../scripts/bootstrap_seed.py#L25) through [scripts/bootstrap_seed.py](../../scripts/bootstrap_seed.py#L30).
- Destructive admin routes are exposed at [server/server.py](../../server/server.py#L1072), [server/server.py](../../server/server.py#L1084), and [server/server.py](../../server/server.py#L1107).
- Cross-origin requests are broadly allowed in [server/server.py](../../server/server.py#L137) through [server/server.py](../../server/server.py#L142).

**Impact**

Anyone with repository access, image access, or leaked deployment config knowledge can use the default key to:

- reset the entire swarm state,
- broadcast privileged messages,
- change runtime config, including rotating the admin key.

Because `allow_origins=["*"]` and `allow_methods=["*"]` are enabled, any malicious website can send browser-based requests to these endpoints if it knows the key. This is effectively full administrative compromise.

**Recommendation**

- Remove the hardcoded secret from source control.
- Require `ADMIN_KEY` from environment or a secrets manager at startup.
- Fail closed if the key is unset.
- Restrict admin endpoints behind real authentication, not a shared static body field.
- Tighten CORS to trusted origins only.

### 2. Critical: No agent authentication allows impersonation, tampering, and orphan writes

**Evidence**

- Clients receive an `agent_id` at registration, but no credential, signature, or token is ever issued in [server/server.py](../../server/server.py#L200) through [server/server.py](../../server/server.py#L229).
- Heartbeats trust the path `agent_id` entirely in [server/server.py](../../server/server.py#L232) through [server/server.py](../../server/server.py#L241).
- Agent-scoped state trusts the query parameter and also mutates server state by updating `last_heartbeat` in [server/server.py](../../server/server.py#L294) through [server/server.py](../../server/server.py#L299).
- Iteration and experiment submission trust caller-supplied `agent_id` in [server/server.py](../../server/server.py#L457) through [server/server.py](../../server/server.py#L625) and [server/server.py](../../server/server.py#L695) through [server/server.py](../../server/server.py#L889).
- Message posting also trusts caller-supplied `agent_name` and `agent_id` in [server/server.py](../../server/server.py#L903) through [server/server.py](../../server/server.py#L924).
- Request models impose no identity proof at all in [server/models.py](../../server/models.py#L27) through [server/models.py](../../server/models.py#L93).

**Amplifying issue**

- The schema defines foreign keys, but connections never enable `PRAGMA foreign_keys=ON`; see [server/db.py](../../server/db.py#L113) through [server/db.py](../../server/db.py#L189). SQLite defaults to `0` unless explicitly enabled. I confirmed the default locally with `PRAGMA foreign_keys`, which returned `0`.

**Impact**

An unauthenticated client can:

- send heartbeats for any agent,
- keep another agent marked active,
- submit experiments or hypotheses as another agent,
- overwrite leaderboard history,
- create orphaned experiment rows for nonexistent agents because foreign keys are not enforced,
- force inconsistent behavior and likely 500s in `create_iteration` when `agent_info` is missing after commit.

This is a complete integrity failure for the swarm protocol.

**Recommendation**

- Bind each registered agent to a server-issued secret or signed token.
- Require that token on every agent-scoped route.
- Reject unknown agents before writes.
- Enable SQLite foreign key enforcement on every connection.
- Treat `/api/state?agent_id=...` as authenticated and non-mutating unless identity is verified.

### 3. High: Stored XSS in dashboard and ideas feed

**Evidence**

- The server accepts arbitrary `agent_name`, `content`, `title`, `description`, `notes`, and `message` values from clients in [server/models.py](../../server/models.py#L27) through [server/models.py](../../server/models.py#L93), then stores and rebroadcasts them in [server/server.py](../../server/server.py#L576) through [server/server.py](../../server/server.py#L594), [server/server.py](../../server/server.py#L662) through [server/server.py](../../server/server.py#L672), and [server/server.py](../../server/server.py#L914) through [server/server.py](../../server/server.py#L922).
- The live feed renders interpolated HTML directly with `innerHTML` in [dashboard/src/panels/feed.ts](../../dashboard/src/panels/feed.ts#L40) through [dashboard/src/panels/feed.ts](../../dashboard/src/panels/feed.ts#L109).
- The ideas page does the same in [dashboard/src/panels/ideas-tree.ts](../../dashboard/src/panels/ideas-tree.ts#L68) through [dashboard/src/panels/ideas-tree.ts](../../dashboard/src/panels/ideas-tree.ts#L170).

**Impact**

Any participant or unauthenticated caller that can hit the API can inject markup or script into the dashboard viewed by operators or observers. A successful payload could:

- hijack browser sessions,
- rewrite the displayed leaderboard/history,
- exfiltrate data visible to the page,
- pivot into any browser-accessible admin workflow.

**Recommendation**

- Stop rendering user-controlled fields with `innerHTML`.
- Use `textContent` or DOM node creation for all untrusted values.
- Sanitize any HTML that must remain rich text with a vetted sanitizer such as DOMPurify.
- Apply output encoding consistently across both dashboard entrypoints.

### 4. Medium: Full algorithm source and swarm history are exposed without authentication

**Evidence**

- Dashboard state returns `best_algorithm_code` to unauthenticated callers in [server/server.py](../../server/server.py#L412) through [server/server.py](../../server/server.py#L418).
- Agent-specific state returns per-agent best code and peer `inspiration_code` in [server/server.py](../../server/server.py#L358) through [server/server.py](../../server/server.py#L379).
- `/api/diversity` loads every feasible agent best `algorithm_code` in [server/server.py](../../server/server.py#L940) through [server/server.py](../../server/server.py#L982).
- `/api/replay`, `/api/top_scores`, `/api/agent_experiments`, and `/api/messages` are all publicly readable in [server/server.py](../../server/server.py#L927) through [server/server.py](../../server/server.py#L1067).
- The WebSocket endpoint accepts any client in [server/server.py](../../server/server.py#L1124) through [server/server.py](../../server/server.py#L1131).

**Impact**

Any unauthenticated client can scrape:

- the best current solver implementation,
- agent names and performance history,
- global-best route data,
- message feed content,
- branch diversity characteristics.

If solver code is meant to be confidential or rate-limited to swarm members, the current API does not provide that boundary.

**Recommendation**

- Decide explicitly whether this system is public-by-design.
- If not, require auth for read APIs and WebSocket subscriptions.
- Separate public scoreboard data from private algorithm/code distribution.
- Never return raw solver source to anonymous dashboard clients.

### 5. Medium: Unbounded request and response sizes enable easy storage and memory abuse

**Evidence**

- Pydantic models do not constrain length or structure for large text-bearing fields such as `algorithm_code`, `notes`, `content`, `title`, and `description`; see [server/models.py](../../server/models.py#L27) through [server/models.py](../../server/models.py#L93).
- These values are stored directly in SQLite and rebroadcast to all WebSocket listeners in [server/server.py](../../server/server.py#L504) through [server/server.py](../../server/server.py#L517), [server/server.py](../../server/server.py#L741) through [server/server.py](../../server/server.py#L754), and [server/server.py](../../server/server.py#L903) through [server/server.py](../../server/server.py#L924).
- `/api/messages` accepts an arbitrary `limit` without clamping in [server/server.py](../../server/server.py#L927) through [server/server.py](../../server/server.py#L934).

**Impact**

An attacker can submit oversized payloads that:

- bloat the SQLite database,
- amplify WebSocket broadcast cost,
- increase page render cost in the browser,
- trigger expensive large-result reads from unbounded list endpoints.

This is primarily a denial-of-service and operational stability risk.

**Recommendation**

- Add maximum lengths for all text fields.
- Cap `route_data` size and validate schema shape.
- Clamp list endpoint limits.
- Enforce request body size limits at the ASGI server or reverse proxy.

## Dependency and Tooling Audit

### Python

`pip-audit` found 2 known vulnerabilities in transitive `starlette 0.38.6`:

- `CVE-2024-47874` / `GHSA-f96h-pmfr-66vw`, fixed in `starlette>=0.40.0`
- `CVE-2025-54121` / `GHSA-2c2j-9gv5-cj73`, fixed in `starlette>=0.47.2`

Current code does not expose multipart form upload handlers, so these do not appear directly reachable from the reviewed endpoints. They are still worth fixing by upgrading FastAPI and Starlette together.

### Node

`npm audit --prefix dashboard --omit=dev` reported no production dependency vulnerabilities.

### Rust

`cargo audit` reported no RustSec vulnerabilities, but it did emit informational warnings:

- `paste 1.0.15` is unmaintained (`RUSTSEC-2024-0436`)
- `rand 0.8.5` has an unsoundness advisory (`RUSTSEC-2026-0097`) under specific feature/logger conditions

In this repository, `rand` is configured in [Cargo.toml](../../Cargo.toml#L12) through [Cargo.toml](../../Cargo.toml#L18) with `default-features = false`, and the reported `rand` advisory appears conditional. I would still plan an upgrade.

## Verification Notes

- `cargo test` completed successfully, but the crate has no actual tests: `0 passed; 0 failed`.
- I did not run a live attack simulation against a running server instance.
- Findings 1 through 5 are based on direct source inspection and are sufficient to justify remediation even without a live exploit harness.

## Remediation Order

1. Remove the hardcoded admin key and add real admin authentication.
2. Add per-agent authentication and enable SQLite foreign key enforcement.
3. Eliminate all `innerHTML` rendering of untrusted data.
4. Restrict or authenticate read APIs and WebSocket subscriptions.
5. Add request size limits, field length validation, and endpoint result caps.
6. Upgrade FastAPI/Starlette and refresh Rust dependencies.

## Bottom Line

This codebase is not safe to expose to untrusted networks in its current form. The top three issues are independently serious and, in combination, allow complete compromise of both control plane integrity and dashboard viewers.
