# Security Decisions — Demo-Specific Tradeoffs

This doc records security-audit findings that were consciously accepted rather
than fixed, because the demo's design requires public visibility. The governing
requirement is documented in [security-requirements.md](./security-requirements.md):
protect host machines first, while allowing public research visibility where it
does not become part of the host code-execution path.

## SA-007 — Public dashboard/history visibility

**Original finding:** Unauthenticated clients can read `best_algorithm_code`,
`inspiration_code`, `/api/diversity`, `/api/replay`, `/api/messages`, and the
dashboard WebSocket stream.

**Decision:** Partially accept. This demo is run as a public live exhibit (live
dashboard projected at events, algorithms shared as an educational artifact), so
public visibility for scoreboard/history-style data is acceptable.

**But this acceptance does not extend to agent-private code-bearing state.**
When `agent_id` is supplied, `/api/state` returns code that agents write into
`mod.rs` and later execute locally. That path is part of the host-execution
trust boundary and must remain authenticated.

**Guardrails kept in place:**
- Admin endpoints (`/api/admin/*`) require `ADMIN_KEY` from env var (SA-004)
- Agent write endpoints require `agent_token` (SA-005)
- Agent-private code-bearing `/api/state` reads are required to be authenticated
- Dashboard is the only browser origin allowed by CORS (SA-008)
- Payload/field-size limits still enforced (SA-009)

**If this codebase is ever reused for a non-public deployment**, tighten the
remaining public read APIs and WebSocket stream as well.
