# Security Audit M2P Report

Timestamp: 2026-04-18 14:16 America/Los_Angeles

## Purpose

This report converts the 2026-04-18 security audit into a Beads execution plan. It maps each finding to specific epics and issues, identifies dependencies, and gives an implementation order that can be executed by engineering without re-reading the full audit.

Source audit: [2026-04-18-1409-security-audit-gpt54high.md](./2026-04-18-1409-security-audit-gpt54high.md)

## Status Update

This report is historical planning context, not the current tracker state.

- Most audit items were closed in Beads after subsequent implementation work.
- After the host-execution analysis, `tig-swarm-demo-fa5` and its parent epic
  `tig-swarm-demo-jxg` were reopened because protecting host machines requires
  authenticated agent-private `/api/state` reads, not just authenticated write
  routes.
- Public dashboard/history visibility remains an accepted tradeoff, but
  agent-private code-bearing state is no longer treated as part of that
  acceptance.

## Backlog Structure

The backlog now consists of three remediation tracks:

1. Existing epic for XSS and related admin/input-sanitization work
2. New epic for identity, authorization, and disclosure failures
3. New epic for resource limits and dependency hardening

## Epics and Issues

### Epic A: Existing frontend/admin remediation track

This track already existed in Beads before this M2P pass. At the time of this report finalization, those issues have been closed in the tracker by recent remediation work and are listed here so the audit-to-issue mapping stays complete.

- `tig-swarm-demo-h43` — Epic: Fix 4 HIGH severity security vulnerabilities from audit
- `tig-swarm-demo-ytr` — Extract shared HTML escape utility from `strategy-leaderboard.ts`
- `tig-swarm-demo-dwo` — SA-001: Fix stored XSS in `ideas-tree.ts` via `innerHTML`
- `tig-swarm-demo-w6w` — SA-002: Fix stored XSS in `feed.ts` via `innerHTML`
- `tig-swarm-demo-zkj` — SA-003: Fix stored XSS via admin broadcast message
- `tig-swarm-demo-lto` — Add server-side input validation to Pydantic models for HTML stripping
- `tig-swarm-demo-elu` — SA-004: Replace hardcoded default admin key with env var requirement

### Epic B: New identity/access-control/disclosure track

This M2P pass created the open issues in this track for the unresolved identity, authorization, and disclosure findings.

- `tig-swarm-demo-jxg` — Epic: Fix swarm identity, access control, and disclosure vulnerabilities from audit
- `tig-swarm-demo-fa5` — SA-005: Require authenticated agent credentials on all agent-scoped routes
- `tig-swarm-demo-mii` — SA-006: Enable SQLite foreign keys and reject orphan writes for unknown agents
- `tig-swarm-demo-pkt` — SA-007: Restrict solver-code and swarm-history APIs to authorized clients
- `tig-swarm-demo-s28` — SA-008: Restrict CORS and harden admin endpoint exposure

### Epic C: New resource/dependency hardening track

This M2P pass created the open issues in this track for the unresolved capacity and dependency work.

- `tig-swarm-demo-rq3` — Epic: Harden request limits and dependency exposure from audit
- `tig-swarm-demo-mlo` — SA-009: Add payload size limits and clamp unbounded list endpoints
- `tig-swarm-demo-ziu` — SA-010: Upgrade FastAPI/Starlette to patched multipart-parser versions
- `tig-swarm-demo-sqj` — SA-011: Refresh Rust dependency advisories and rerun `cargo audit`

## Finding-to-Issue Mapping

### Finding 1: Hardcoded admin secret plus exposed admin surface

- Primary issues:
  - `tig-swarm-demo-elu`
  - `tig-swarm-demo-s28`
- Notes:
  - `tig-swarm-demo-elu` addresses secret handling.
  - `tig-swarm-demo-s28` addresses wildcard CORS and overly broad admin route exposure.

### Finding 2: No agent authentication and unenforced referential integrity

- Primary issues:
  - `tig-swarm-demo-fa5`
  - `tig-swarm-demo-mii`
- Notes:
  - `tig-swarm-demo-fa5` covers agent credential issuance and verification.
  - `tig-swarm-demo-mii` covers `PRAGMA foreign_keys=ON`, existence checks, and orphan-write prevention.

### Finding 3: Stored XSS in dashboard and ideas feed

- Primary issues:
  - `tig-swarm-demo-ytr`
  - `tig-swarm-demo-dwo`
  - `tig-swarm-demo-w6w`
  - `tig-swarm-demo-zkj`
  - `tig-swarm-demo-lto`
- Notes:
  - The frontend rendering fixes depend on shared escaping work in `tig-swarm-demo-ytr`.
  - `tig-swarm-demo-lto` is supplemental server-side hardening, not a replacement for output encoding.

### Finding 4: Unauthenticated disclosure of solver code and swarm history

- Primary issues:
  - `tig-swarm-demo-pkt`
  - `tig-swarm-demo-fa5`
- Notes:
  - `tig-swarm-demo-pkt` is blocked by `tig-swarm-demo-fa5` because access control is not meaningful until client identity exists.

### Finding 5: Unbounded request and response sizes

- Primary issues:
  - `tig-swarm-demo-mlo`
- Notes:
  - This should cover both validation limits and endpoint caps.

### Dependency audit items

- Primary issues:
  - `tig-swarm-demo-ziu`
  - `tig-swarm-demo-sqj`

## Dependency Graph

- `tig-swarm-demo-h43` depends on:
  - `tig-swarm-demo-elu`
  - `tig-swarm-demo-ytr`
  - `tig-swarm-demo-dwo`
  - `tig-swarm-demo-w6w`
  - `tig-swarm-demo-zkj`
  - `tig-swarm-demo-lto`
- `tig-swarm-demo-jxg` depends on:
  - `tig-swarm-demo-fa5`
  - `tig-swarm-demo-mii`
  - `tig-swarm-demo-pkt`
  - `tig-swarm-demo-s28`
- `tig-swarm-demo-rq3` depends on:
  - `tig-swarm-demo-mlo`
  - `tig-swarm-demo-ziu`
  - `tig-swarm-demo-sqj`
- `tig-swarm-demo-pkt` depends on:
  - `tig-swarm-demo-fa5`

## Suggested Execution Order

### Phase 1: Immediate critical containment

1. `tig-swarm-demo-elu`
2. `tig-swarm-demo-s28`
3. `tig-swarm-demo-fa5`
4. `tig-swarm-demo-mii`

Rationale:
- These four items address the most direct remote compromise and integrity risks.

### Phase 2: Browser and disclosure containment

1. `tig-swarm-demo-ytr`
2. `tig-swarm-demo-dwo`
3. `tig-swarm-demo-w6w`
4. `tig-swarm-demo-zkj`
5. `tig-swarm-demo-pkt`
6. `tig-swarm-demo-lto`

Rationale:
- XSS fixes should land before broad dashboard exposure continues.
- Disclosure restrictions become meaningful once agent auth exists.

### Phase 3: Operational hardening

1. `tig-swarm-demo-mlo`
2. `tig-swarm-demo-ziu`
3. `tig-swarm-demo-sqj`

Rationale:
- These reduce abuse and maintenance risk after the control-plane vulnerabilities are contained.

## Manager-to-Programmer Notes

- Do not treat HTML stripping as a substitute for safe rendering. The browser-side `innerHTML` removals are mandatory.
- Do not treat CORS as authentication. `tig-swarm-demo-s28` is supplemental to `tig-swarm-demo-elu` and `tig-swarm-demo-fa5`.
- Do not ship partial auth that only protects write endpoints. The read endpoints currently leak solver code and agent history.
- The SQLite foreign-key fix should be verified on every connection path, not just during database initialization.

## Outcome

All audit findings are now represented in Beads:

- 3 audit epics total
- 11 audit-linked issues total
- 7 new open issues created in this M2P pass
- 1 explicit cross-issue dependency (`tig-swarm-demo-pkt` blocked by `tig-swarm-demo-fa5`)

This backlog is sufficient to drive the remaining remediation from critical containment through dependency cleanup while preserving the already-completed audit work in the tracker history.
