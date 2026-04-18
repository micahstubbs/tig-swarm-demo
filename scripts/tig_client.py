"""tig_client.py — federation primitives for multi-site registration/publishing.

Reads ~/.tig-swarm/hosts.json and fans out requests across configured hosts.
"""
from __future__ import annotations

import json
import os
import socket
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

DEFAULT_HOSTS_CONFIG: dict = {
    "primary": "https://tigswarmdemo.com",
    "hosts": ["https://tigswarmdemo.com", "https://demo.discoveryatscale.com"],
    "credentials": {},
}

HOSTS_FILE: Path = Path(
    os.environ.get(
        "TIG_HOSTS_FILE",
        str(Path.home() / ".tig-swarm" / "hosts.json"),
    )
)

# Cloudflare's default WAF rules reject the built-in Python-urllib UA.
# Sending a generic browser-ish UA lets requests pass edge filtering.
USER_AGENT: str = os.environ.get(
    "TIG_USER_AGENT",
    "tig-swarm-client/1.0 (+https://github.com/SteveDiamond/tig-swarm-demo)",
)


# ---------------------------------------------------------------------------
# Config persistence
# ---------------------------------------------------------------------------

def load_hosts() -> dict:
    """Read ~/.tig-swarm/hosts.json. Create with defaults if missing."""
    if not HOSTS_FILE.exists():
        save_hosts(DEFAULT_HOSTS_CONFIG)
        return dict(DEFAULT_HOSTS_CONFIG)
    with open(HOSTS_FILE, "r", encoding="utf-8") as fh:
        return json.load(fh)


def save_hosts(cfg: dict) -> None:
    """Write back atomically (temp file + rename).

    Creates parent dir with mode 0o700 on first write.
    Writes with mode 0o600 (user-only).
    """
    parent = HOSTS_FILE.parent
    parent.mkdir(mode=0o700, parents=True, exist_ok=True)

    tmp = HOSTS_FILE.with_suffix(".tmp")
    data = json.dumps(cfg, indent=2)
    tmp.write_text(data, encoding="utf-8")
    tmp.chmod(0o600)
    os.replace(tmp, HOSTS_FILE)
    HOSTS_FILE.chmod(0o600)


# ---------------------------------------------------------------------------
# Host resolution
# ---------------------------------------------------------------------------

def resolve_hosts() -> list[str]:
    """Return hosts to contact.

    If TIG_SERVER_URL env var is set, returns [that one].
    Otherwise returns cfg['hosts'].
    """
    override = os.environ.get("TIG_SERVER_URL")
    if override:
        return [override]
    cfg = load_hosts()
    return cfg["hosts"]


def primary() -> str:
    """Return cfg['primary'] (or TIG_SERVER_URL override if set)."""
    override = os.environ.get("TIG_SERVER_URL")
    if override:
        return override
    cfg = load_hosts()
    return cfg["primary"]


# ---------------------------------------------------------------------------
# Credentials
# ---------------------------------------------------------------------------

def creds_for(host: str) -> Optional[dict]:
    """Return {'agent_id', 'agent_token', 'agent_name'} or None."""
    cfg = load_hosts()
    return cfg.get("credentials", {}).get(host)


# ---------------------------------------------------------------------------
# HTTP helpers
# ---------------------------------------------------------------------------

def post(host: str, path: str, payload: dict, timeout: int = 30) -> dict:
    """POST JSON, return parsed JSON response. Raise on non-2xx."""
    url = host.rstrip("/") + path
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        headers={
            "Content-Type": "application/json",
            "User-Agent": USER_AGENT,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"HTTP {exc.code} from {url}: {raw}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"URL error for {url}: {exc.reason}") from exc
    except socket.timeout as exc:
        raise RuntimeError(f"Timeout ({timeout}s) for {url}") from exc


def get(host: str, path: str, params: dict = None, timeout: int = 30) -> dict:
    """GET, return parsed JSON response."""
    url = host.rstrip("/") + path
    if params:
        url += "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(url, method="GET", headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"HTTP {exc.code} from {url}: {raw}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"URL error for {url}: {exc.reason}") from exc
    except socket.timeout as exc:
        raise RuntimeError(f"Timeout ({timeout}s) for {url}") from exc


# ---------------------------------------------------------------------------
# Parallel fan-out
# ---------------------------------------------------------------------------

def parallel_requests(
    hosts_and_args,
    method: str = "GET",
    path: str = "",
    timeout: int = 30,
) -> dict:
    """Fan out requests across hosts in parallel.

    hosts_and_args:
        - For GET:  list of (host, agent_id) tuples — agent_id is added as ?agent_id= param
        - For POST: list of (host, payload) tuples — payload is the JSON body

    Uses concurrent.futures.ThreadPoolExecutor.
    Returns {host: result_or_exception}.
    """
    results: dict = {}

    def _do(host: str, arg):
        if method.upper() == "GET":
            agent_id = arg
            params = {"agent_id": agent_id} if agent_id else None
            return get(host, path, params=params, timeout=timeout)
        else:
            payload = arg if isinstance(arg, dict) else {}
            return post(host, path, payload, timeout=timeout)

    with ThreadPoolExecutor(max_workers=min(len(list(hosts_and_args)), 8)) as executor:
        # hosts_and_args may be an iterator; materialise it
        items = list(hosts_and_args)
        future_to_host = {executor.submit(_do, host, arg): host for host, arg in items}
        for future in as_completed(future_to_host):
            host = future_to_host[future]
            exc = future.exception()
            results[host] = exc if exc is not None else future.result()

    return results
