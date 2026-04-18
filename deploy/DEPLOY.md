# Deploying tig-swarm-demo at tigswarmdemo.com

This is the end-to-end deploy story for tigswarmdemo.com. It differs from the
original plan (`docs/plans/2026-04-18-tigswarmdemo-deploy-plan.md`) in one
place: **DNS is managed in Cloudflare, not Spaceship.** Cloudflare Tunnel
requires the domain to be a Cloudflare zone so the edge can terminate TLS.
Spaceship is still the registrar — it delegates to Cloudflare's nameservers.

## Architecture at a glance

```
tigswarmdemo.com (registered at Spaceship, NS delegated to Cloudflare)
  └── Cloudflare zone (account fa0c1c0e...)
       └── CNAME @   → fa757484-…cfargotunnel.com (proxied)
       └── CNAME www → fa757484-…cfargotunnel.com (proxied)
       └── Cloudflare Tunnel fa757484-…
            └── ingress: http://localhost:8090  (on this machine)
                 └── uvicorn serving server:app (via server/.venv)
                      ├── FastAPI REST + WebSocket
                      └── StaticFiles mount → server/static/  ← built from dashboard/dist/
```

## Prerequisites

- Spaceship API creds at `~/keys/spaceship/spaceship.md`
- Cloudflare API token with `Zone:Edit` (account-level) at
  `~/keys/cloudflare/CLOUDFLARE_API_TOKEN_VENTURESTUDIO.md`
- Cloudflared tunnel `fa757484-8a18-46e2-b313-1ca149487613` already running
  via `opits-cloudflared.service` (user-level)
- `cloudflared`, `rsync`, `yarn`, Python 3.12+

## Stage A — Build dashboard

```bash
./scripts/build-dashboard.sh
```

Produces `server/static/` from `dashboard/dist/`.

## Stage B — Install server deps

```bash
cd server
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

## Stage C — Systemd unit (requires sudo for env file)

See `deploy/SETUP.md` for the three commands.

## Stage D — Cloudflare tunnel ingress

Already configured in `~/.cloudflared/config.yml`. See
`deploy/cloudflared-tunnel-ingress.yml` for the snippet.

## Stage E — DNS setup (one time)

### E1. Create the Cloudflare zone

```bash
TOK=$(cat ~/keys/cloudflare/CLOUDFLARE_API_TOKEN_VENTURESTUDIO.md | tr -d '\n')
curl -s -X POST https://api.cloudflare.com/client/v4/zones \
  -H "Authorization: Bearer $TOK" -H "Content-Type: application/json" \
  -d '{"name":"tigswarmdemo.com","account":{"id":"fa0c1c0e6b3f6cde0271c5301b128350"},"type":"full"}'
```

Record the returned zone ID and the two nameservers (e.g.
`aaden.ns.cloudflare.com`, `savanna.ns.cloudflare.com`).

### E2. Delegate nameservers at Spaceship

```bash
SS_KEY=$(grep '^API_KEY=' ~/keys/spaceship/spaceship.md | cut -d= -f2)
SS_SECRET=$(grep '^SECRET=' ~/keys/spaceship/spaceship.md | cut -d= -f2)
curl -X PUT "https://spaceship.dev/api/v1/domains/tigswarmdemo.com/nameservers" \
  -H "X-Api-Key: $SS_KEY" -H "X-Api-Secret: $SS_SECRET" \
  -H "Content-Type: application/json" \
  --data '{"provider":"custom","hosts":["aaden.ns.cloudflare.com","savanna.ns.cloudflare.com"]}'
```

### E3. Set DNS records in Cloudflare

```bash
ZID=<zone id from E1>
TARGET=fa757484-8a18-46e2-b313-1ca149487613.cfargotunnel.com

for name in tigswarmdemo.com www; do
  curl -s -X POST "https://api.cloudflare.com/client/v4/zones/$ZID/dns_records" \
    -H "Authorization: Bearer $TOK" -H "Content-Type: application/json" \
    -d "{\"type\":\"CNAME\",\"name\":\"$name\",\"content\":\"$TARGET\",\"ttl\":1,\"proxied\":true}"
done
```

`proxied:true` is critical — it routes traffic through Cloudflare's edge, which
is what lets the tunnel terminate TLS.

### E4. Wait for zone activation

```bash
curl -s -H "Authorization: Bearer $TOK" \
  "https://api.cloudflare.com/client/v4/zones/$ZID" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['result']['status'])"
```

Status goes from `pending` → `active` once Cloudflare confirms NS delegation
(usually 5–30 minutes; can be hours if upstream DNS caches).

### E5. Verify

```bash
curl -sI https://tigswarmdemo.com/
curl -sI https://www.tigswarmdemo.com/
```

Expect `HTTP/2 200` on both once activation completes.

## Troubleshooting

- **522 / 502 from Cloudflare:** the tunnel is up but uvicorn isn't running.
  `systemctl --user status tig-swarm-demo` or `ss -tlnp | grep 8090`.
- **Connection reset:** zone still `pending`; wait for activation.
- **Cert errors:** Cloudflare universal SSL provisions a cert when the zone
  activates. If it's been >30m and no cert, force a re-provision in the CF
  dashboard under SSL/TLS → Edge Certificates.
- **Dashboard is stale after redeploy:** re-run `./scripts/build-dashboard.sh`
  and `systemctl --user restart tig-swarm-demo`.

## Updating the dashboard after a code change

```bash
./scripts/build-dashboard.sh   # rebuilds dashboard/dist → server/static/
systemctl --user restart tig-swarm-demo   # picks up the new static files
```

## Rolling the admin key

```bash
NEW_KEY=$(openssl rand -hex 32)
sudoc sed -i "s|^ADMIN_KEY=.*|ADMIN_KEY=$NEW_KEY|" /etc/tig-swarm-demo/env
systemctl --user restart tig-swarm-demo
echo "New ADMIN_KEY: $NEW_KEY"
```
