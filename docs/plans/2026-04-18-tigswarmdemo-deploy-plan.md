# tigswarmdemo.com Deployment Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Deploy this working tree at `https://tigswarmdemo.com` via Cloudflare Tunnel + Spaceship DNS, served continuously from this machine by a systemd user service, with a new federated multi-site agent client that defaults to fan-out publishing across tigswarmdemo.com and demo.discoveryatscale.com.

**Architecture:** FastAPI on `127.0.0.1:8090` serves the built dashboard at `/` from `server/static/` and REST + WebSocket at `/api/*` and `/ws`. Existing `cloudflared` tunnel `fa757484` gets new ingress routes for the two new hostnames. Spaceship DNS CNAMEs point to the tunnel. Agent scripts move behind a new `scripts/tig_client.py` federation layer that reads `~/.tig-swarm/hosts.json` and fans out publishes + merges reads across hosts (best-lineage switching, union hypotheses, min stagnation, per-host inspiration files, tagged leaderboard).

**Tech Stack:** Python 3 / FastAPI / uvicorn, TypeScript / Vite, SQLite, systemd user units, cloudflared, Spaceship DNS REST API (via `spaceship-dns` skill).

**Design reference:** `docs/plans/2026-04-18-tigswarmdemo-deploy-design.md`

---

## Stage A — Dashboard build pipeline

### Task A1: Verify dashboard build works end-to-end

**Files:** (no changes, verification only)
- Run: `dashboard/`

**Step 1:** Build
```bash
cd dashboard && yarn install --frozen-lockfile && yarn build
```
Expected: `dashboard/dist/index.html` and `dashboard/dist/assets/*.{js,css}` produced, no errors.

**Step 2:** Confirm output size is reasonable
```bash
du -sh dashboard/dist
ls dashboard/dist/
```
Expected: single-digit MB at most; contains `index.html`, `assets/`, maybe `ideas.html`.

### Task A2: Add a build-and-stage script

**Files:**
- Create: `scripts/build-dashboard.sh`

**Step 1:** Write the script
```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
(cd dashboard && yarn install --frozen-lockfile && yarn build)
rsync -a --delete dashboard/dist/ server/static/
echo "Staged $(find server/static -type f | wc -l) files into server/static/"
```

**Step 2:** Make executable + gitignore the staged output
```bash
chmod +x scripts/build-dashboard.sh
echo "server/static/" >> .gitignore
```

**Step 3:** Run it
```bash
./scripts/build-dashboard.sh
```
Expected: prints a file count > 0, `server/static/index.html` exists.

**Step 4:** Smoke-test locally
```bash
cd server && ADMIN_KEY=devkey uvicorn server:app --host 127.0.0.1 --port 8090 &
sleep 2
curl -sI http://127.0.0.1:8090/ | head -1
kill %1
```
Expected: `HTTP/1.1 200 OK` and the body should start with `<!DOCTYPE html>`.

**Step 5:** Commit
```bash
git add scripts/build-dashboard.sh .gitignore
git commit -m "Add scripts/build-dashboard.sh to stage dashboard into server/static"
```

---

## Stage B — Federation client

### Task B1: Write `scripts/tig_client.py` — federation primitives

**Files:**
- Create: `scripts/tig_client.py`

**Step 1:** Create the module with these public functions:
- `load_hosts() -> dict` — read `~/.tig-swarm/hosts.json`, create with defaults if missing.
- `save_hosts(cfg: dict)` — write back atomically (temp file + rename).
- `resolve_hosts() -> list[str]` — returns the list of hosts to contact. If `TIG_SERVER_URL` is set, returns `[that one]`. Else returns `cfg["hosts"]`.
- `primary() -> str` — returns `cfg["primary"]` (or `TIG_SERVER_URL` override if set).
- `creds_for(host: str) -> dict | None` — returns `{"agent_id", "agent_token"}` or `None`.
- `parallel_requests(hosts, method, path, payload=None, params=None, timeout=30) -> dict[host, result_or_exception]` — uses `concurrent.futures.ThreadPoolExecutor`, catches per-host exceptions, returns mapping.

Default `hosts.json` on first write:
```python
DEFAULT_HOSTS = {
    "primary": "https://tigswarmdemo.com",
    "hosts": ["https://tigswarmdemo.com", "https://demo.discoveryatscale.com"],
    "credentials": {},
}
```

File lives at `~/.tig-swarm/hosts.json` (create parent dir with `0700`).

**Step 2:** Run a quick sanity script inline:
```bash
python3 -c "
import sys; sys.path.insert(0, 'scripts')
import tig_client
print(tig_client.resolve_hosts())
print(tig_client.primary())
"
```
Expected: prints the two default hosts and the primary URL. Should have created `~/.tig-swarm/hosts.json`.

**Step 3:** Commit
```bash
git add scripts/tig_client.py
git commit -m "Add scripts/tig_client.py federation primitives"
```

### Task B2: Rewrite `scripts/register.py` to fan out registration

**Files:**
- Create: `scripts/register.py`
- Modify: none

**Step 1:** Write
```python
#!/usr/bin/env python3
"""Register this agent at every host in ~/.tig-swarm/hosts.json.

Usage: python3 scripts/register.py [--force]

Re-running is idempotent for already-registered hosts unless --force is given.
"""
import json, sys
import tig_client as tc

def main():
    force = "--force" in sys.argv
    cfg = tc.load_hosts()
    results = {}
    for host in cfg["hosts"]:
        if not force and host in cfg["credentials"]:
            print(f"[skip] {host}: already registered ({cfg['credentials'][host]['agent_id']})")
            continue
        try:
            resp = tc.post(host, "/api/agents/register", {"client_version": "federation-1.0"})
            cfg["credentials"][host] = {
                "agent_id":    resp["agent_id"],
                "agent_name":  resp["agent_name"],
                "agent_token": resp["agent_token"],
            }
            print(f"[ok]   {host}: {resp['agent_name']} ({resp['agent_id']})")
        except Exception as e:
            print(f"[err]  {host}: {e}", file=sys.stderr)
            results[host] = e
    tc.save_hosts(cfg)
    sys.exit(1 if results else 0)

if __name__ == "__main__":
    main()
```

Add a `post()` helper in `tig_client.py` if not already present.

**Step 2:** Make executable
```bash
chmod +x scripts/register.py
```

**Step 3:** Commit
```bash
git add scripts/register.py scripts/tig_client.py
git commit -m "Add scripts/register.py for fan-out registration"
```

### Task B3: Rewrite `scripts/publish.py` to fan out publishing

**Files:**
- Modify: `scripts/publish.py` (full rewrite)

**Step 1:** Replace the existing publish.py. New signature (keeps backward-ish compat):
```
Usage: python3 scripts/publish.py "<title>" "<description>" <strategy_tag> ["notes"]
```

(No more positional `agent_id` / `agent_token` — those come from `~/.tig-swarm/hosts.json`.)

Payload assembly stays the same. For each host in `tc.resolve_hosts()`:
- look up `creds_for(host)`; skip with a warning if missing
- POST `/api/iterations` with host-specific `agent_id` + `agent_token` and the shared title/desc/code/score payload
- collect result or exception

After fan-out:
- print per-host status line
- if primary succeeded: exit 0 (even if non-primaries failed, log warning)
- if primary failed: exit 1

**Step 2:** Test that argv parsing still errors out cleanly with too few args:
```bash
echo '{}' | python3 scripts/publish.py 2>&1 | head -3
```
Expected: usage message on stderr.

**Step 3:** Commit
```bash
git add scripts/publish.py
git commit -m "Rewrite scripts/publish.py to fan out across all configured hosts"
```

### Task B4: Write `scripts/state.py` federated read layer

**Files:**
- Create: `scripts/state.py`

**Step 1:** Write the script. Logic matches the design doc's "Federated Read Layer" table:

```python
#!/usr/bin/env python3
"""Read and merge state from every host in ~/.tig-swarm/hosts.json.

Writes merged best_algorithm_code to src/vehicle_routing/algorithm/mod.rs.
Saves per-host inspiration to /tmp/inspiration-<host>.rs when provided.
Prints a human summary to stdout.
"""
import json, sys
from pathlib import Path
import tig_client as tc

ALGO = Path(__file__).resolve().parent.parent / "src/vehicle_routing/algorithm/mod.rs"

def host_tag(host: str) -> str:
    # https://tigswarmdemo.com -> "tig"; https://demo.discoveryatscale.com -> "das"
    h = host.replace("https://", "").replace("http://", "")
    return {"tigswarmdemo.com": "tig", "demo.discoveryatscale.com": "das"}.get(h, h.split(".")[0])

def main():
    cfg = tc.load_hosts()
    hosts = tc.resolve_hosts()
    results = tc.parallel_requests(
        [(h, tc.creds_for(h)["agent_id"]) for h in hosts if tc.creds_for(h)],
        method="GET", path="/api/state",
    )
    # results: dict[host] -> state dict OR Exception
    ok = {h: s for h, s in results.items() if not isinstance(s, Exception)}
    for h, s in results.items():
        if isinstance(s, Exception):
            print(f"[warn] {h}: {s}", file=sys.stderr)

    if not ok:
        print("[err] no hosts reachable", file=sys.stderr); sys.exit(1)

    # Best-lineage pick
    best_host, best_state = min(
        ok.items(),
        key=lambda kv: kv[1].get("my_best_score") if kv[1].get("my_best_score") is not None else float("inf"),
    )

    # Union hypotheses
    seen = set(); hypotheses = []
    for s in ok.values():
        for h in s.get("recent_hypotheses") or []:
            key = (h.get("title"), h.get("strategy_tag"))
            if key in seen: continue
            seen.add(key); hypotheses.append(h)
    hypotheses.sort(key=lambda h: h.get("created_at") or "", reverse=True)
    hypotheses = hypotheses[:20]

    # Global best (min across sites)
    scores = [s.get("best_score") for s in ok.values() if s.get("best_score") is not None]
    global_best = min(scores) if scores else None

    # Stagnation: min across sites
    stag_vals = [s.get("my_runs_since_improvement") for s in ok.values() if s.get("my_runs_since_improvement") is not None]
    stagnation = min(stag_vals) if stag_vals else 0

    # Activity sums
    my_runs = sum(s.get("my_runs") or 0 for s in ok.values())
    my_improvements = sum(s.get("my_improvements") or 0 for s in ok.values())

    # Per-host inspiration files
    for host, s in ok.items():
        code = s.get("inspiration_code")
        if code:
            Path(f"/tmp/inspiration-{host_tag(host)}.rs").write_text(code)

    # Tagged leaderboard
    board = []
    for host, s in ok.items():
        tag = host_tag(host)
        for e in s.get("leaderboard") or []:
            e2 = dict(e); e2["agent_name"] = f"{e.get('agent_name')}@{tag}"
            board.append(e2)
    board.sort(key=lambda e: e.get("score") or float("inf"))

    # Write algorithm code
    if best_state.get("best_algorithm_code"):
        ALGO.write_text(best_state["best_algorithm_code"])

    # Print summary
    print(f"best lineage: {host_tag(best_host)} (score={best_state.get('my_best_score')})")
    print(f"global best:  {global_best}")
    print(f"runs: {my_runs} · improvements: {my_improvements} · stagnation: {stagnation}")
    print(f"recent hypotheses: {len(hypotheses)}")
    print(f"inspiration files: " + ", ".join(
        f"/tmp/inspiration-{host_tag(h)}.rs" for h, s in ok.items() if s.get("inspiration_code")
    ) or "(none)")
    print(f"top 5 leaderboard:")
    for e in board[:5]:
        print(f"  {e['agent_name']}: {e['score']}")

if __name__ == "__main__":
    main()
```

Add `tc.parallel_requests` overload that takes `(host, agent_id)` pairs if you didn't already.

**Step 2:** Make executable
```bash
chmod +x scripts/state.py
```

**Step 3:** Commit
```bash
git add scripts/state.py scripts/tig_client.py
git commit -m "Add scripts/state.py federated read layer"
```

### Task B5: Update `CLAUDE.md` agent section

**Files:**
- Modify: `CLAUDE.md` (sections: Quick Start, Step 1-5, Posting Messages, Rules)

**Step 1:** Replace the curl-based register/heartbeat/publish examples with the new scripts:

- **Register:** `python3 scripts/register.py` (one command, fans out)
- **Read state:** `python3 scripts/state.py` (writes mod.rs, prints summary)
- **Publish:** `python3 scripts/publish.py "title" "desc" strategy_tag "notes"`
- **Single-host override:** `TIG_SERVER_URL=https://demo.discoveryatscale.com python3 scripts/publish.py ...`

Explain the federated semantics in-line (best-lineage, union hypotheses, per-host inspiration files).

Update "Server URL" section: primary is `https://tigswarmdemo.com`, mirror is `https://demo.discoveryatscale.com`, both configured in `~/.tig-swarm/hosts.json`.

**Step 2:** Commit
```bash
git add CLAUDE.md
git commit -m "Update CLAUDE.md agent workflow for federated client"
```

---

## Stage C — Systemd user service

### Task C1: Create the environment file

**Files:**
- Create: `/etc/tig-swarm-demo/env` (not in the repo — root-owned, 0600)

**Step 1:** User provides this command (contains a secret; do NOT commit):
```bash
sudoc install -d -m 0755 /etc/tig-swarm-demo
sudoc tee /etc/tig-swarm-demo/env >/dev/null <<'EOF'
ADMIN_KEY=<generate with: openssl rand -hex 32>
CORS_ORIGINS=https://tigswarmdemo.com,https://www.tigswarmdemo.com
EOF
sudoc chmod 0600 /etc/tig-swarm-demo/env
```

Claude: print this as a code block for the user to run. Don't exec it yourself.

**Step 2:** Verify
```bash
sudo cat /etc/tig-swarm-demo/env | grep -c ADMIN_KEY
```
Expected: `1`

### Task C2: Write the systemd user unit

**Files:**
- Create: `~/.config/systemd/user/tig-swarm-demo.service`
- Create: `deploy/tig-swarm-demo.service` (checked into repo for reference)

**Step 1:** Write the checked-in copy at `deploy/tig-swarm-demo.service`:
```ini
[Unit]
Description=tig-swarm-demo FastAPI server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=/home/m/wk/tig-swarm-demo/server
EnvironmentFile=/etc/tig-swarm-demo/env
ExecStart=/usr/bin/env uvicorn server:app --host 127.0.0.1 --port 8090
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal
OOMScoreAdjust=200

[Install]
WantedBy=default.target
```

**Step 2:** Install it
```bash
mkdir -p ~/.config/systemd/user
cp deploy/tig-swarm-demo.service ~/.config/systemd/user/
systemctl --user daemon-reload
```

**Step 3:** Start + enable
```bash
systemctl --user enable --now tig-swarm-demo.service
systemctl --user status tig-swarm-demo.service | head -10
```
Expected: `active (running)`.

**Step 4:** Verify the server is listening locally
```bash
curl -sI http://127.0.0.1:8090/ | head -1
```
Expected: `HTTP/1.1 200 OK`.

**Step 5:** Commit
```bash
git add deploy/tig-swarm-demo.service
git commit -m "Add deploy/tig-swarm-demo.service systemd user unit"
```

---

## Stage D — Cloudflare tunnel ingress

### Task D1: Add ingress entries to cloudflared config

**Files:**
- Modify: `~/.cloudflared/config.yml` (outside the repo; user config)

**Step 1:** Read the current config to find the `ingress:` block and the `- service: http_status:404` catch-all (which must stay last):
```bash
cat ~/.cloudflared/config.yml
```

**Step 2:** Insert before the catch-all:
```yaml
  - hostname: tigswarmdemo.com
    service: http://localhost:8090
  - hostname: www.tigswarmdemo.com
    service: http://localhost:8090
```

**Step 3:** Validate
```bash
cloudflared tunnel ingress validate --config ~/.cloudflared/config.yml
```
Expected: `OK`.

**Step 4:** Restart cloudflared service (whichever is configured — system or user):
```bash
systemctl --user restart cloudflared 2>&1 || sudoc systemctl restart cloudflared
```

**Step 5:** Verify tunnel sees the new ingress
```bash
cloudflared tunnel info fa757484-8a18-46e2-b313-1ca149487613
```

### Task D2: Back up the config into the repo

**Files:**
- Create: `deploy/cloudflared-tunnel-ingress.yml` (snippet only, documenting what the user added)

**Step 1:** Write the snippet (document only — not the full config, which contains tunnel UUIDs for unrelated sites):
```yaml
# Added to ~/.cloudflared/config.yml under ingress:
- hostname: tigswarmdemo.com
  service: http://localhost:8090
- hostname: www.tigswarmdemo.com
  service: http://localhost:8090
```

**Step 2:** Commit
```bash
git add deploy/cloudflared-tunnel-ingress.yml
git commit -m "Document tigswarmdemo.com cloudflared ingress snippet"
```

---

## Stage E — Spaceship DNS

### Task E1: Set CNAMEs via the spaceship-dns skill

**Files:** (no code changes — external API calls)

**Step 1:** Confirm the tunnel CNAME target
```bash
echo "fa757484-8a18-46e2-b313-1ca149487613.cfargotunnel.com"
```

**Step 2:** Invoke the `spaceship-dns` skill (via Skill tool) with an instruction like:
> Set these DNS records on tigswarmdemo.com:
> - `tigswarmdemo.com` CNAME `fa757484-8a18-46e2-b313-1ca149487613.cfargotunnel.com` TTL 300
> - `www.tigswarmdemo.com` CNAME `fa757484-8a18-46e2-b313-1ca149487613.cfargotunnel.com` TTL 300
> Use the key at `~/keys/spaceship/spaceship.md`.

**Step 3:** Verify propagation (will take 1–5 minutes)
```bash
dig +short tigswarmdemo.com
dig +short www.tigswarmdemo.com
```
Expected: shows a CNAME to `cfargotunnel.com` and an IP.

**Step 4:** Full end-to-end test over HTTPS
```bash
curl -sI https://tigswarmdemo.com/ | head -3
```
Expected: `HTTP/2 200` with `content-type: text/html`.

**Step 5:** Browser check (Chrome MCP)
- Navigate to `https://tigswarmdemo.com`
- Verify dashboard renders, WebSocket connects, stats are showing
- Check dev console — no CORS errors

### Task E2: Register a test agent end-to-end

**Files:** none

**Step 1:** Wipe local client state (back up first)
```bash
cp ~/.tig-swarm/hosts.json ~/.tig-swarm/hosts.json.bak 2>/dev/null || true
rm -f ~/.tig-swarm/hosts.json
```

**Step 2:** Register
```bash
python3 scripts/register.py
```
Expected: two `[ok]` lines, one per host. `~/.tig-swarm/hosts.json` now has creds for both.

**Step 3:** Read state
```bash
python3 scripts/state.py
```
Expected: prints a summary, `src/vehicle_routing/algorithm/mod.rs` contains the Solomon seed (since we have no best yet).

**Step 4:** Publish a dummy iteration
```bash
python3 scripts/benchmark.py 2>/dev/null | python3 scripts/publish.py "federation test" "verifying dual publish" other "deploy smoke test"
```
Expected: two `[ok]` lines (one per host), primary exits 0.

### Task E3: Final session summary

**Files:**
- Create: `docs/session-summaries/2026-04-18-HHMMSS-tigswarmdemo-deploy.md`

Summarize what was built, commit hashes, and any follow-up items (e.g., cloudflared running mode, DNS target-IP changes, failed registrations).

Commit the summary, push.

---

## Rollback Guide

| Stage | Rollback |
|---|---|
| A (build script) | `git revert` the commit; delete `server/static/`. |
| B (federation client) | Revert commits; the old single-host `publish.py` is in git history at `aeef92a` and earlier. |
| C (systemd) | `systemctl --user disable --now tig-swarm-demo && rm ~/.config/systemd/user/tig-swarm-demo.service && systemctl --user daemon-reload`. |
| D (cloudflared) | Remove the two ingress lines; restart cloudflared. |
| E (DNS) | Delete the two CNAME records via the spaceship-dns skill. |

Steps A–C are local-only; D makes the site reachable internally via direct IP lookup (edge won't route); only E makes the site publicly discoverable. You can stop at any stage and the previous ones stay functional.

---

## Appendix — Related Skills

- `@spaceship-dns` for the DNS step
- `@ncd` (new-cf-domain) covers a similar end-to-end for reference
- `@nus` (node-user-systemd) is the Node version of stage C — pattern is identical
- `@superpowers:executing-plans` to run this plan
