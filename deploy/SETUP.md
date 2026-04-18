# tig-swarm-demo systemd setup

## 0. Install Python dependencies in a venv

```bash
cd server && python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
```

The systemd unit references `server/.venv/bin/uvicorn` directly.

## 1. Create the environment file (requires sudo)

Generate an admin key and write the env file:

```bash
ADMIN_KEY=$(openssl rand -hex 32)
sudoc install -d -m 0755 /etc/tig-swarm-demo
echo -e "ADMIN_KEY=$ADMIN_KEY\nCORS_ORIGINS=https://tigswarmdemo.com,https://www.tigswarmdemo.com" | sudoc tee /etc/tig-swarm-demo/env >/dev/null
sudoc chmod 0600 /etc/tig-swarm-demo/env
echo "ADMIN_KEY: $ADMIN_KEY (save this for admin calls)"
```

## 2. Install and start the user service

```bash
mkdir -p ~/.config/systemd/user
cp deploy/tig-swarm-demo.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now tig-swarm-demo.service
```

## 3. Verify

```bash
systemctl --user status tig-swarm-demo.service | head -12
curl -sI http://127.0.0.1:8090/ | head -1   # expect: HTTP/1.1 200 OK
```

## Troubleshooting

- Logs: `journalctl --user -u tig-swarm-demo -f`
- Dashboard files missing (404): run `./scripts/build-dashboard.sh` first
- Port 8090 collision: `portctl get 8090`

## Lingering (optional — lets the service run when logged out)

```bash
sudoc loginctl enable-linger $USER
```
