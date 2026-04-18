# Session Summary — Init, Dependencies, Port-Conflict Resolution

## Summary
Bootstrapped project documentation for the tig-swarm-demo repo: added developer-focused sections to CLAUDE.md, wrote a dependencies doc, installed all toolchains, and resolved a port conflict (8080 was taken on this machine) by allocating 8090 via the local port-registry service. Also created a new `/pcr` skill so the port workflow is reusable on other projects.

## Completed Work

| Change | Commit |
|--------|--------|
| `/init`: prepended Build & Test Commands, Architecture, Key Constraints sections to CLAUDE.md (preserved existing Swarm Agent protocol below) | (uncommitted from prior turn, folded into later commits) |
| Added `DEPENDENCIES.md` — Rust crates, Python server deps, Node dashboard deps, Python stdlib-only scripts, system toolchain install | `9e5f4a7` (tig-swarm-demo-94t) |
| Installed all deps: `cargo build -r --bin tig_solver …` (target/release), `npm install` in `dashboard/` (29 pkgs), `python3 -m venv server/.venv` + `pip install -r requirements.txt` (20 pkgs) | (no commit — generated artifacts) |
| Per user directive, added "Do NOT use Docker" rule to CLAUDE.md Key Constraints; dropped Docker section from DEPENDENCIES.md; bumped fastapi version reference to 0.136.0 | `57e09e8` (tig-swarm-demo-vuy) |
| Created `/pcr` skill (alias `/resolve-port-conflicts`) at `~/.claude/skills/pcr/` with scanner script at `~/.claude/scripts/pcr-scan.sh` | `c287d94` (in `~/.claude`) |
| Resolved port conflict: server moved from 8080 (taken, registered as `unknown-8080`) to 8090 (`tig-swarm-demo-server`); registered 5173 as `tig-swarm-demo-dashboard`. Updated `Dockerfile`, `CLAUDE.md`, `README.md`, `DEPENDENCIES.md`. Added Port Assignments section to CLAUDE.md. | `25805e5` |
| Added `LESSONS.md` documenting the port-collision pattern on multi-project dev machines | `4efc89e` |
| Enhanced `~/.claude/skills/setup-clone/` to run `/pcr` before starting servers, and pointed "Port Already in Use" error handler to `/pcr` | `f109f5f` (in `~/.claude`) |

## Key Changes

**Files created in project**:
- `DEPENDENCIES.md` — toolchain and dep install instructions
- `LESSONS.md` — project lessons learned log
- `docs/session-summaries/2026-04-18-213327-init-deps-port-conflicts.md`

**Files modified in project**:
- `CLAUDE.md` — prepended dev-focused sections; added Port Assignments + no-Docker rule
- `README.md`, `DEPENDENCIES.md`, `Dockerfile` — port references 8080 → 8090

**Files created in `~/.claude`**:
- `skills/pcr/SKILL.md`
- `skills/resolve-port-conflicts/SKILL.md` (alias)
- `scripts/pcr-scan.sh` (port-reference scanner)

**Files modified in `~/.claude`**:
- `skills/setup-clone/SKILL.md` — added proactive port check step

**Port registry state** (`portctl list`):
- `8090/tcp` → `tig-swarm-demo-server` (dynamic, allocated)
- `5173/tcp` → `tig-swarm-demo-dashboard` (static, registered)

## Pending / Blocked

- **`server/requirements.txt` has an uncommitted change** that wasn't made in this session (fastapi 0.115.0 → 0.136.0, added starlette==1.0.0). Left alone — likely from another agent or parallel edit. Future session should decide whether to commit.
- **`docs/security-audit-pages/`** is untracked (pre-existing). Not touched this session.
- **Smoke test**: `python3 scripts/benchmark.py` ran once successfully (emitted full JSON); a second run had a stderr/stdout interleave that broke the JSON parse. The solver itself works. An end-to-end smoke test of the server at the new port 8090 was NOT done.
- **Beads issues closed this session**: `tig-swarm-demo-94t` (DEPENDENCIES.md), `tig-swarm-demo-vuy` (install deps).

## Next Session Context

1. **Port 8090 end-to-end verification**: start `cd server && .venv/bin/uvicorn server:app --port 8090`, confirm it binds and `/api/agents/register` responds. Confirm dashboard dev server on 5173 can proxy to it.
2. **Decide on `server/requirements.txt`**: either commit the uncommitted fastapi/starlette bump (if that was intentional) or revert it.
3. **Agent token auth**: CLAUDE.md was edited by the user to note that `agent_token` is now required in write requests and is only returned once at registration. If spinning up a new agent, capture this token immediately. Worth checking `server/server.py` to understand the new auth model before registering.
4. **Register with the swarm**: the actual point of the project — clone, set up, register, iterate on the VRPTW solver. Everything below `# Swarm Agent — Automated Discovery at Scale` in CLAUDE.md is the protocol.
