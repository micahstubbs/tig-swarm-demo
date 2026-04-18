Timestamp: 2026-04-18 14:24 America/Los_Angeles

# Quick Review: Recent Security Changes

## Findings

### 1. High: the admin-key migration does not actually rotate existing deployments off the old database key

`server/db.py` now sources `DEFAULT_CONFIG["admin_key"]` from `ADMIN_KEY`, but it still persists config rows with `INSERT OR IGNORE` ([server/db.py](/home/m/wk/tig-swarm-demo/server/db.py:179)). On any existing deployment that already has a `config.admin_key` row, this migration is a no-op, and `verify_admin()` will continue accepting the legacy database value ([server/server.py](/home/m/wk/tig-swarm-demo/server/server.py:81)). In other words, this closes the issue only for fresh databases; upgraded environments can keep the previously compromised shared key indefinitely.

Suggested fix: explicitly overwrite `config.admin_key` from the environment during startup, or fail startup when the persisted value does not match `ADMIN_KEY`.

### 2. High: the new agent-token scheme still leaves `/api/state?agent_id=...` unauthenticated

The patch adds `verify_agent()` and wires it into several POST routes, but the agent-specific branch of `GET /api/state` still trusts the querystring `agent_id` and returns `best_algorithm_code`, private hypothesis history, and peer `inspiration_code` without any token check ([server/server.py](/home/m/wk/tig-swarm-demo/server/server.py:313)). That means any caller can still impersonate an agent for the most sensitive read path, which materially defeats the new auth model.

Suggested fix: require an agent credential on agent-scoped reads as well, or split the endpoint so private agent state is no longer served from an unauthenticated GET path.

### 3. Medium: message spoofing is still possible by omitting `agent_id`

`POST /api/messages` only calls `verify_agent()` when `req.agent_id` is present ([server/server.py](/home/m/wk/tig-swarm-demo/server/server.py:931)), while `agent_name` remains client-controlled ([server/models.py](/home/m/wk/tig-swarm-demo/server/models.py:112)). An attacker can therefore post arbitrary chat messages under any displayed name by sending `agent_id = null` and a forged `agent_name`, bypassing the intended trust boundary for the feed.

Suggested fix: either require authenticated agent identity for all non-admin messages or derive `agent_name` exclusively from the authenticated `agent_id` on the server side.

## Verification Notes

- Reviewed the committed security fix in `78b2260` and the current uncommitted server changes.
- Verified the new dependency pins install cleanly in a throwaway virtualenv.
- Ran `python3 -m py_compile server/*.py`; no syntax errors.
