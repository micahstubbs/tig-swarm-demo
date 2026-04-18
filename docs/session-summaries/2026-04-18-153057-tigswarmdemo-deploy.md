# Session Summary — tigswarmdemo.com deploy + federated multi-site client

**Date:** 2026-04-18
**Duration:** ~3h (continuation of earlier security audit session)

## Summary

Shipped the tig-swarm-demo project live at `https://tigswarmdemo.com` (and `https://www.tigswarmdemo.com`) fronted by Cloudflare Tunnel. Added a federated multi-site agent client so an agent's research work fans out to both `tigswarmdemo.com` and the upstream `demo.discoveryatscale.com` by default, with a `TIG_SERVER_URL` env var for single-host override.

## Completed Work

### Design + plan

- `docs/plans/2026-04-18-tigswarmdemo-deploy-design.md` — architecture doc (commit `fcebc42`)
- `docs/plans/2026-04-18-tigswarmdemo-deploy-plan.md` — 14-step implementation plan (commit `68830b9`)

### Federation client (Stages A, B1, B2, B3)

Commit `3471491`, `bc6be23`:
- `scripts/build-dashboard.sh` — builds dashboard, rsyncs to `server/static/`
- `scripts/tig_client.py` — federation primitives (hosts.json loader, parallel GET/POST, TIG_SERVER_URL override)
- `scripts/register.py` — fan-out `POST /api/agents/register` to every host, stores creds per-host
- `scripts/publish.py` (rewritten) — fan-out publish to all hosts, primary-success semantics
- `scripts/state.py` — fans out `GET /api/state`, merges: best-lineage code selection, union `recent_hypotheses`, min `global_best`, min stagnation, sum runs/improvements, per-host `/tmp/inspiration-<tag>.rs`, tagged leaderboard
- `CLAUDE.md` — agent workflow updated for federated client, curl commands replaced with scripts

### Systemd service (Stage C)

Commit `bc6be23`:
- `deploy/tig-swarm-demo.service` — user-level unit with `Restart=on-failure`
- `deploy/SETUP.md` — manual sudo steps to create `/etc/tig-swarm-demo/env` and enable the service
- `server/.venv/` — created locally; unit uses `server/.venv/bin/uvicorn` directly (commit `6c6321d`)

### Cloudflare tunnel (Stage D)

Commit `56f3993`:
- Added `tigswarmdemo.com` and `www.tigswarmdemo.com` ingress to `~/.cloudflared/config.yml` (tunnel `fa757484-8a18-46e2-b313-1ca149487613`)
- Validated via `cloudflared tunnel ingress validate`
- Restarted `opits-cloudflared.service`
- `deploy/cloudflared-tunnel-ingress.yml` — documented snippet

### DNS + activation (Stage E)

Commit `6c6321d`:
- Created Cloudflare zone for `tigswarmdemo.com` (zone ID `74db8bd3b3808bad3b0a9634f5894ba9`, account `fa0c1c0e…`)
- Delegated Spaceship nameservers to `aaden.ns.cloudflare.com` + `savanna.ns.cloudflare.com`
- Added proxied CNAME records for apex + www → `fa757484-…cfargotunnel.com`
- Forced Universal SSL re-issuance via toggle
- Zone activated; Edge cert provisioned within ~30s
- `deploy/DEPLOY.md` — end-to-end runbook

### Verification

- `curl -sI https://tigswarmdemo.com/` → `HTTP/2 200`
- `curl -sI https://www.tigswarmdemo.com/` → `HTTP/2 200`
- `curl -s https://www.tigswarmdemo.com/api/state` → returns valid JSON with Solomon seed algorithm

## Key Changes

| Change | Why |
|---|---|
| DNS in Cloudflare (not Spaceship) | Cloudflare Tunnel requires the domain to be a CF zone — TLS termination at the edge only works for zones in the CF account. The original plan's "Spaceship DNS + Cloudflare Tunnel" combo was architecturally impossible; Spaceship remains the registrar, CF handles DNS. |
| Systemd `ExecStart=server/.venv/bin/uvicorn` (not `/usr/bin/env uvicorn`) | uvicorn isn't on system PATH. The project owns its venv explicitly. |
| `CLOUDFLARE_API_TOKEN_VENTURESTUDIO` is the only token with `com.cloudflare.api.account.zone.create` permission | The other two tokens (`EDIT_DNS_AND_DNS_SETTINGS`, `EDIT_CLOUDFLARE_WORKERS`) are zone-scoped or resource-scoped and can't create new zones. |
| Federation with fan-out publish + best-lineage reads | User chose "submit to all hosts that the project knows about" as default. Best-lineage means mod.rs follows whichever site scored the agent's work better; union hypotheses prevents re-proposing ideas across sites. |

## Pending / Blocked

### One manual step remains: systemd service

The server is currently running via a **temporary manual uvicorn process** (backgrounded shell job, not systemd). To make it persistent and auto-restart on reboot, run the three commands in `deploy/SETUP.md`:

```bash
# 0. venv already created
# 1. Create env file (requires sudo)
ADMIN_KEY=$(openssl rand -hex 32)
sudoc install -d -m 0755 /etc/tig-swarm-demo
echo -e "ADMIN_KEY=$ADMIN_KEY\nCORS_ORIGINS=https://tigswarmdemo.com,https://www.tigswarmdemo.com" | sudoc tee /etc/tig-swarm-demo/env >/dev/null
sudoc chmod 0600 /etc/tig-swarm-demo/env

# 2. Install + start service
mkdir -p ~/.config/systemd/user
cp deploy/tig-swarm-demo.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now tig-swarm-demo.service

# 3. Kill the temp process
# (find via: ps -ef | grep "[u]vicorn server:app")
```

Until then, the site goes down if the temp process dies or the shell closes.

## Next Session Context

- **All beads issues closed.** Epic `tig-swarm-demo-qoq` + 7 sub-issues resolved.
- **Agents can now register via `python3 scripts/register.py`.** This fans out to both sites and stores creds in `~/.tig-swarm/hosts.json`.
- **Dashboard redeploy flow:** edit → `./scripts/build-dashboard.sh` → `systemctl --user restart tig-swarm-demo` (once Stage C is fully installed).
- **If `tigswarmdemo.com` returns 522 later:** the temp uvicorn died. Run the SETUP.md commands to make it permanent.
- **Spaceship DNS is effectively inactive for this domain** — NS is delegated. All future DNS changes go through the Cloudflare API (token `CLOUDFLARE_API_TOKEN_VENTURESTUDIO`, zone ID `74db8bd3b3808bad3b0a9634f5894ba9`).
