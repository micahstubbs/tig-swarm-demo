#!/usr/bin/env bash
set -euo pipefail

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  echo "source this script instead of executing it: source scripts/init_local_swarm_session.sh <session-tag>" >&2
  exit 2
fi

SESSION_TAG="${1:-default}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CFG_DIR="${HOME}/.tig-swarm/sessions"
CFG_PATH="${CFG_DIR}/${SESSION_TAG}.json"
MIRROR_UPSTREAM="${TIG_MIRROR_UPSTREAM:-1}"

mkdir -p "$CFG_DIR"

export TIG_HOSTS_FILE="$CFG_PATH"

if [[ "$MIRROR_UPSTREAM" == "1" ]]; then
  cat >"$CFG_PATH" <<'EOF'
{
  "primary": "http://127.0.0.1:8090",
  "hosts": [
    "http://127.0.0.1:8090",
    "https://demo.discoveryatscale.com"
  ],
  "credentials": {}
}
EOF
else
  cat >"$CFG_PATH" <<'EOF'
{
  "primary": "http://127.0.0.1:8090",
  "hosts": [
    "http://127.0.0.1:8090"
  ],
  "credentials": {}
}
EOF
fi

echo "[local-swarm] TIG_HOSTS_FILE=$TIG_HOSTS_FILE"
python3 "$ROOT_DIR/scripts/register.py" --force
python3 "$ROOT_DIR/scripts/state.py"
