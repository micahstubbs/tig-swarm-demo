# Lessons Learned

Append-only log of debugging insights and non-obvious patterns discovered while working on this project.

---

## 2026-04-18T21:31 - Conventional dev ports collide on multi-project machines

**Problem**: Project docs and Dockerfile advertised `uvicorn server:app --port 8080`, but port 8080 was already bound by another process on the dev machine (registered in the local port-registry as `unknown-8080`, PID unknown). Starting the server would have failed silently or taken over a conflicting service.

**Root Cause**: 8080 is the most-reused "alternative HTTP" port in dev tooling — FastAPI/Express/Tomcat/Jenkins/countless demos all default to it. On a single-user dev machine that hosts many projects, collisions are the rule, not the exception. The `Dockerfile` hardcoded `EXPOSE 8080` and the CMD defaulted `${PORT:-8080}`, and the README/CLAUDE.md/DEPENDENCIES.md followed the Dockerfile without checking the local machine's actual port map.

**Lesson**: On a multi-project dev machine, **never copy a conventional default port from upstream docs without checking the local registry**. The registry (`portctl list` / `portctl get <port>`) is the source of truth, not the framework's default. For new projects, allocate from a project-specific range (8090-8099 for tig-swarm-demo here) and register the assignment so the next project sees it as taken.

**Solution**:
- `portctl allocate -s tig-swarm-demo-server --preferred 8090 --range-min 8090 --range-max 8099` → got 8090
- `portctl register -p 5173 -s tig-swarm-demo-dashboard` → claimed the Vite dev port explicitly
- Updated `Dockerfile`, `CLAUDE.md`, `README.md`, `DEPENDENCIES.md` to 8090
- Added a **Port Assignments** section to `CLAUDE.md` documenting both registrations so future sessions don't re-collide

**Prevention**:
- Before starting work on a project that exposes HTTP endpoints, run `/pcr` (resolve-port-conflicts) to scan references, check the registry, and reassign conflicts in one pass.
- When scaffolding a new project, pick ports from a free range and register them immediately — don't wait for the first collision to find out.
- Treat ports in `Dockerfile` / `docker-compose.yml` / framework CLI flags as project configuration that must match the local registry, not as upstream defaults to preserve verbatim.
