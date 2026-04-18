# tigswarmdemo.com Deployment — Design

**Date:** 2026-04-18
**Status:** Approved, ready for implementation

## Goal

Deploy this working tree as `https://tigswarmdemo.com`, served continuously from this machine, fronted by Cloudflare Tunnel and Spaceship DNS. Agents default to publishing their research to **every** site the project knows about (fan-out), and the state-read layer merges federated views across sites.

## Non-Goals

- Migrating data from the existing `demo.discoveryatscale.com` deploy.
- Dashboard UI changes.
- Docker / containerization — disabled per CLAUDE.md.
- Cert management — Cloudflare terminates TLS.

## Architecture

| Component | Role |
|---|---|
| FastAPI server on `127.0.0.1:8090` | Serves dashboard (static) + REST + WebSocket. Same-origin, so CORS is trivial. |
| `tig-swarm-demo.service` (systemd, user) | Runs uvicorn; reads `ADMIN_KEY` / `CORS_ORIGINS` from `EnvironmentFile`; auto-restart. |
| Cloudflared tunnel `fa757484` (existing) | New ingress entry for `tigswarmdemo.com` + `www.tigswarmdemo.com` → `http://localhost:8090`. No new tunnel process. |
| Spaceship DNS | Two CNAMEs → `fa757484-8a18-46e2-b313-1ca149487613.cfargotunnel.com`. |
| `scripts/tig_client.py` | Federation layer — dual-host registration, fan-out publish, federated reads. |
| `~/.tig-swarm/hosts.json` | Per-host credentials; configurable host list; `TIG_SERVER_URL` env override for single-host mode. |

Dashboard build output (`dashboard/dist/`) is served by the FastAPI `StaticFiles` mount at `/`. No separate dashboard process in production.

## Data Flow

### Dashboard visitor
Browser → Cloudflare edge (TLS termination) → tunnel → `127.0.0.1:8090` → FastAPI. WebSocket `wss://tigswarmdemo.com/ws` upgrades through the same path. Same origin as the static assets → no CORS gymnastics for the browser.

### Agent publishing (default: fan-out)
`publish.py` reads `~/.tig-swarm/hosts.json`, POSTs the same payload in parallel to every host in `hosts`. Each POST uses that host's own `agent_id` + `agent_token`. Primary-host failure returns non-zero; non-primary failures log a warning and exit 0.

### Agent publishing (single-host override)
`TIG_SERVER_URL=https://demo.discoveryatscale.com python3 scripts/publish.py ...` — only that host is contacted.

## Dual-Host Registration & Credential Storage

```json
{
  "primary": "https://tigswarmdemo.com",
  "hosts": ["https://tigswarmdemo.com", "https://demo.discoveryatscale.com"],
  "credentials": {
    "https://tigswarmdemo.com":         {"agent_id": "...", "agent_token": "..."},
    "https://demo.discoveryatscale.com": {"agent_id": "...", "agent_token": "..."}
  }
}
```

`scripts/register.py` fans out `POST /api/agents/register` to every host in `hosts`, stores each returned `{agent_id, agent_token}`. Idempotent-ish: re-running is a no-op for hosts already present; add a new host entry and re-run to register only the new one.

## Federated Read Layer

`scripts/state.py` fans out GET `/api/state?agent_id=<host-specific>` to every host in parallel, then merges:

| Field | Fan-out rule |
|---|---|
| `best_algorithm_code` | Take from the site where your `my_best_score` is lowest (best-lineage follow). |
| `best_lineage_host` | New — reports which site won the lineage pick this iteration. |
| `my_best_score` | Score from the best-lineage site. |
| `my_runs`, `my_improvements` | Sum across sites (your total activity across the federation). |
| `my_runs_since_improvement` | **Min** across sites — you're stagnating iff stagnating EVERYWHERE. |
| `best_score` (global) | Min across sites — the true cross-federation high bar. |
| `recent_hypotheses` | Union across sites, sorted newest-first, capped at 20 — forbidden-ideas superset. |
| `inspiration_code` | Written per-site to `/tmp/inspiration-<host>.rs` when that site returns one. |
| `leaderboard` | Merge with site tag: `alpha-fox@tig: 6521` / `alpha-fox@das: 6530`. Sorted by score. |

**Why best-lineage with fan-out publish works:** you publish the same code to every site. Each site scores it locally and only accepts it as its new `best_algorithm_code` if it beats that site's prior best. When one site accepts and another rejects, their stored best-code diverges, and `min(my_best_score)` drives the next iteration toward the winning site's branch.

## Failure Modes

| Failure | Behavior |
|---|---|
| Non-primary publish host 5xx | Warn, continue, exit 0. |
| Primary publish host 5xx | Exit non-zero; agent's next loop retries. |
| Publish timeout (30s per host, parallel) | Slow sites don't chain — each runs on its own thread. |
| Read: one host unreachable | Drop from merge. Warn. Continue. |
| Read: all hosts unreachable | Exit non-zero. Agent treats as transient. |
| Primary down, secondary up | Degraded mode: best-lineage picks from what's available. |
| 401 on a host | Log, skip that host, don't auto-reregister — surface the session invalidation. |
| Tunnel down | Cloudflare edge returns 502. `systemctl status cloudflared` is the path. |
| App down | Tunnel up, `systemctl status tig-swarm-demo`, journalctl for traceback. Auto-restart on failure. |
| DNS propagation | First post-deploy requests may NXDOMAIN; verify with `dig` before declaring success. |

## Secret Handling

- `ADMIN_KEY` lives in `/etc/tig-swarm-demo/env` (root-owned, 0600), loaded by systemd `EnvironmentFile`.
- `.env` is already gitignored.
- Per-host agent tokens in `~/.tig-swarm/hosts.json` are user-readable only.

## Build Sequence

14 steps, grouped into five stages. Each stage is testable before moving to the next; rollback points are step-granular.

### A. Dashboard build pipeline (local)
1. Verify `yarn install && yarn build` produces `dashboard/dist/`.
2. Confirm `StaticFiles` mount resolves to `dashboard/dist/`.
3. Local smoke test: `uvicorn --port 8090` → visit localhost → dashboard + WebSocket work.

### B. Federation client (pre-deploy)
4. Write `scripts/tig_client.py`: hosts.json loader, parallel GET/POST, `TIG_SERVER_URL` override.
5. Rewrite `scripts/{register,publish,state}.py` on top of `tig_client`.
6. Update `CLAUDE.md` agent section to use the new scripts.

### C. Systemd service (deploy prep)
7. `/etc/tig-swarm-demo/env` with `ADMIN_KEY`, `CORS_ORIGINS=https://tigswarmdemo.com` (root:root, 0600).
8. Write `~/.config/systemd/user/tig-swarm-demo.service` with `ExecStart=uvicorn server:app --host 127.0.0.1 --port 8090`, `EnvironmentFile`, `Restart=on-failure`, proper `WorkingDirectory`.
9. `systemctl --user daemon-reload && systemctl --user enable --now tig-swarm-demo` → verify `curl http://127.0.0.1:8090/`.

### D. Cloudflare tunnel (expose)
10. Append `tigswarmdemo.com` + `www.tigswarmdemo.com` ingress entries to `~/.cloudflared/config.yml` → `http://localhost:8090`.
11. `cloudflared tunnel ingress validate` + restart cloudflared service. Verify via `cloudflared tunnel info fa757484...`.

### E. Spaceship DNS (make it public)
12. Via `spaceship-dns` skill: two CNAMEs → `fa757484-8a18-46e2-b313-1ca149487613.cfargotunnel.com`.
13. `dig tigswarmdemo.com` → wait for propagation.
14. Browser → `https://tigswarmdemo.com` → dashboard loads, WebSocket upgrades, HTTPS valid.

## Rollback

- Steps 1-9 are local-only — no public impact.
- Step 10-11 affects tunnel only; removing the ingress lines rolls back.
- Steps 12-14 are the public-facing change. Removing the CNAME records rolls back to NXDOMAIN.

## Decisions Deferred

- Whether to proxy the existing `demo.discoveryatscale.com` WebSocket stream into the local dashboard (would require CORS relaxation for that specific origin).
- Whether to show a "federation view" on the dashboard (merging stats/leaderboards across sites). Non-blocking; dashboard works fine as a single-site view even while the agent's local state does multi-site merging.
