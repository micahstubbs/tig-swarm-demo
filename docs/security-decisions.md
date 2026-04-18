# Security Decisions — Demo-Specific Tradeoffs

This doc records security-audit findings that were consciously accepted rather
than fixed, because the demo's design requires public visibility.

## SA-007 — Public solver-code and swarm-history APIs

**Finding:** Unauthenticated clients can read `best_algorithm_code`,
`inspiration_code`, `/api/diversity`, `/api/replay`, `/api/messages`, and the
dashboard WebSocket stream.

**Decision:** Accept. This demo is run as a public live exhibit (live dashboard
projected at events, algorithms shared as an educational artifact). Restricting
those endpoints would defeat the purpose.

**Guardrails kept in place:**
- Admin endpoints (`/api/admin/*`) require `ADMIN_KEY` from env var (SA-004)
- Agent write endpoints require `agent_token` (SA-005)
- Dashboard is the only browser origin allowed by CORS (SA-008)
- Payload/field-size limits still enforced (SA-009)

**If this codebase is ever reused for a non-public deployment**, reopen SA-007
and add an auth gate to the WebSocket stream, the `/api/state` algorithm code
fields, `/api/messages` GET, and `/api/diversity`.
