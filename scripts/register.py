#!/usr/bin/env python3
"""Register this agent at every host in ~/.tig-swarm/hosts.json.

Usage: python3 scripts/register.py [--force]

Re-running is idempotent for already-registered hosts unless --force is given.
"""
import sys
from pathlib import Path

# Allow running from project root or scripts/ dir
sys.path.insert(0, str(Path(__file__).resolve().parent))
import tig_client as tc


def main():
    force = "--force" in sys.argv
    cfg = tc.load_hosts()
    failed = {}

    for host in cfg["hosts"]:
        if not force and host in cfg.get("credentials", {}):
            cred = cfg["credentials"][host]
            print(f"[skip] {host}: already registered ({cred['agent_id']})")
            continue
        try:
            resp = tc.post(host, "/api/agents/register", {"client_version": "federation-1.0"})
            if "credentials" not in cfg:
                cfg["credentials"] = {}
            cfg["credentials"][host] = {
                "agent_id":    resp["agent_id"],
                "agent_name":  resp["agent_name"],
                # Legacy hosts do not return agent_token; newer hosts do.
                "agent_token": resp.get("agent_token"),
            }
            print(f"[ok]   {host}: {resp['agent_name']} ({resp['agent_id']})")
        except Exception as e:
            print(f"[err]  {host}: {e}", file=sys.stderr)
            failed[host] = e

    tc.save_hosts(cfg)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
