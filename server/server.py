import json
import asyncio
import logging
from datetime import datetime, timezone
from contextlib import asynccontextmanager

from fastapi import FastAPI, WebSocket, WebSocketDisconnect, HTTPException, Depends
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from pathlib import Path

from models import (
    RegisterRequest, HeartbeatRequest, HypothesisCreate, ExperimentCreate,
    AdminBroadcast, AdminAuth, AgentResponse, HypothesisResponse, DuplicateResponse,
    ExperimentResponse, new_id, improvement_pct,
)
from names import generate_agent_name, load_used_names
from dedup import fingerprint, check_duplicate, check_saturation
import db

logger = logging.getLogger("swarm")

DEFAULT_BASELINE = 1850.5

# Cached config — refreshed on admin config update
_config_cache: dict | None = None


async def get_config_cached() -> dict:
    global _config_cache
    if _config_cache is None:
        async with db.connect() as conn:
            _config_cache = await db.get_config(conn)
    return _config_cache


def get_baseline(config: dict) -> float:
    return float(config.get("baseline_score", str(DEFAULT_BASELINE)))


async def verify_admin(req: AdminAuth) -> None:
    config = await get_config_cached()
    if req.admin_key != config.get("admin_key", "ads-2026"):
        raise HTTPException(status_code=403, detail="Invalid admin key")


async def get_agent_name(conn, agent_id: str) -> str:
    cursor = await conn.execute("SELECT name FROM agents WHERE id = ?", (agent_id,))
    row = await cursor.fetchone()
    return row["name"] if row else "unknown"


# ── WebSocket manager ──

class ConnectionManager:
    def __init__(self):
        self.connections: list[WebSocket] = []

    async def connect(self, ws: WebSocket):
        await ws.accept()
        self.connections.append(ws)

    def disconnect(self, ws: WebSocket):
        if ws in self.connections:
            self.connections.remove(ws)

    async def broadcast(self, event: dict):
        if not self.connections:
            return
        results = await asyncio.gather(
            *(ws.send_json(event) for ws in self.connections),
            return_exceptions=True,
        )
        self.connections = [
            ws for ws, result in zip(self.connections, results)
            if not isinstance(result, Exception)
        ]


manager = ConnectionManager()


# ── App lifecycle ──

@asynccontextmanager
async def lifespan(app: FastAPI):
    await db.init_db()
    async with db.connect() as conn:
        names = await db.get_all_agent_names(conn)
    load_used_names(names)
    task = asyncio.create_task(periodic_stats())
    yield
    task.cancel()


app = FastAPI(title="Swarm Coordination Server", lifespan=lifespan)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

dashboard_dir = Path(__file__).parent.parent / "dashboard" / "dist"
if dashboard_dir.exists():
    app.mount("/dashboard", StaticFiles(directory=str(dashboard_dir), html=True), name="dashboard")


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


# ── Periodic stats ──

async def periodic_stats():
    while True:
        await asyncio.sleep(10)
        try:
            config = await get_config_cached()
            baseline = get_baseline(config)
            async with db.connect() as conn:
                best = await db.get_global_best(conn)
                cursor = await conn.execute(
                    "SELECT COUNT(*) as agents FROM agents WHERE status != 'offline',"
                    " (SELECT COUNT(*) FROM experiments) as experiments,"
                    " (SELECT COUNT(*) FROM hypotheses) as hypotheses"
                )
                # SQLite doesn't support multi-table in one SELECT easily, use separate queries
                active = await db.get_agent_count(conn, active_only=True)
                total_exp = (await (await conn.execute("SELECT COUNT(*) as c FROM experiments")).fetchone())["c"]
                total_hyp = (await (await conn.execute("SELECT COUNT(*) as c FROM hypotheses")).fetchone())["c"]

            await manager.broadcast({
                "type": "stats_update",
                "active_agents": active,
                "total_experiments": total_exp,
                "hypotheses_count": total_hyp,
                "best_score": best["score"] if best else None,
                "baseline_score": baseline,
                "improvement_pct": improvement_pct(baseline, best["score"]) if best else 0,
                "timestamp": now(),
            })
        except Exception:
            logger.exception("Error in periodic_stats")


# ── Agent endpoints ──

@app.post("/api/agents/register", response_model=AgentResponse)
async def register_agent(req: RegisterRequest):
    agent_id = new_id()
    agent_name = generate_agent_name()
    timestamp = now()

    async with db.connect() as conn:
        await conn.execute(
            "INSERT INTO agents (id, name, registered_at, last_heartbeat, status) VALUES (?, ?, ?, ?, ?)",
            (agent_id, agent_name, timestamp, timestamp, "idle"),
        )
        await conn.commit()
        config = await db.get_config(conn)

    await manager.broadcast({
        "type": "agent_joined",
        "agent_id": agent_id,
        "agent_name": agent_name,
        "timestamp": timestamp,
    })

    return AgentResponse(
        agent_id=agent_id,
        agent_name=agent_name,
        registered_at=timestamp,
        config={
            "heartbeat_interval_seconds": 30,
            "benchmark_instances": json.loads(config.get("benchmark_instances", "[]")),
            "baseline_score": get_baseline(config),
        },
    )


@app.post("/api/agents/{agent_id}/heartbeat")
async def heartbeat(agent_id: str, req: HeartbeatRequest):
    timestamp = now()
    async with db.connect() as conn:
        await conn.execute(
            "UPDATE agents SET last_heartbeat = ?, status = ? WHERE id = ?",
            (timestamp, req.status, agent_id),
        )
        await conn.commit()
    return {"ack": True, "server_time": timestamp}


# ── State endpoint ──

@app.get("/api/state")
async def get_state():
    config = await get_config_cached()
    baseline = get_baseline(config)

    async with db.connect() as conn:
        best = await db.get_global_best(conn)
        active = await db.get_agent_count(conn, active_only=True)

        total_exp = (await (await conn.execute("SELECT COUNT(*) as c FROM experiments")).fetchone())["c"]

        cursor = await conn.execute("""
            SELECT e.*, a.name as agent_name
            FROM experiments e JOIN agents a ON a.id = e.agent_id
            ORDER BY e.created_at DESC LIMIT 20
        """)
        recent_experiments = [dict(row) for row in await cursor.fetchall()]

        cursor = await conn.execute("""
            SELECT h.*, a.name as agent_name
            FROM hypotheses h JOIN agents a ON a.id = h.agent_id
            WHERE h.status IN ('proposed', 'claimed', 'testing')
            ORDER BY h.created_at DESC
        """)
        active_hypotheses = [dict(row) for row in await cursor.fetchall()]

        cursor = await conn.execute("""
            SELECT h.id, h.title, h.strategy_tag, h.description, a.name as agent_name
            FROM hypotheses h JOIN agents a ON a.id = h.agent_id
            WHERE h.status = 'failed'
            ORDER BY h.created_at DESC LIMIT 20
        """)
        failed_hypotheses = [dict(row) for row in await cursor.fetchall()]

        cursor = await conn.execute("""
            SELECT h.id, h.title, h.strategy_tag, h.description, a.name as agent_name
            FROM hypotheses h JOIN agents a ON a.id = h.agent_id
            WHERE h.status = 'succeeded'
            ORDER BY h.created_at DESC LIMIT 10
        """)
        succeeded_hypotheses = [dict(row) for row in await cursor.fetchall()]

        leaderboard = await db.compute_leaderboard(conn, baseline)

    return {
        "baseline_score": baseline,
        "best_score": best["score"] if best else baseline,
        "best_algorithm_diff": best["algorithm_diff"] if best else "",
        "best_experiment_id": best["id"] if best else None,
        "best_route_data": json.loads(best["route_data"]) if best and best["route_data"] else None,
        "active_agents": active,
        "total_experiments": total_exp,
        "recent_experiments": [
            {
                "id": e["id"],
                "agent_name": e["agent_name"],
                "score": e["score"],
                "feasible": bool(e["feasible"]),
                "improvement_pct": improvement_pct(baseline, e["score"]),
                "created_at": e["created_at"],
                "notes": e["notes"],
            }
            for e in recent_experiments
        ],
        "active_hypotheses": [
            {"id": h["id"], "title": h["title"], "strategy_tag": h["strategy_tag"],
             "status": h["status"], "agent_name": h["agent_name"]}
            for h in active_hypotheses
        ],
        "failed_hypotheses": [
            {"id": h["id"], "title": h["title"], "strategy_tag": h["strategy_tag"],
             "agent_name": h["agent_name"], "description": h["description"]}
            for h in failed_hypotheses
        ],
        "succeeded_hypotheses": [
            {"id": h["id"], "title": h["title"], "strategy_tag": h["strategy_tag"],
             "agent_name": h["agent_name"], "description": h["description"]}
            for h in succeeded_hypotheses
        ],
        "leaderboard": leaderboard,
    }


# ── Hypothesis endpoints ──

@app.post("/api/hypotheses")
async def create_hypothesis(req: HypothesisCreate):
    async with db.connect() as conn:
        cursor = await conn.execute(
            "SELECT id, title, strategy_tag, status, fingerprint FROM hypotheses"
        )
        all_hyps = [dict(row) for row in await cursor.fetchall()]

        dup = check_duplicate(req.title, req.strategy_tag, all_hyps)
        if dup:
            raise HTTPException(status_code=409, detail=DuplicateResponse(
                similar_hypothesis_id=dup["id"],
                similar_title=dup["title"],
                similar_status=dup["status"],
            ).model_dump())

        if check_saturation(req.strategy_tag, all_hyps):
            raise HTTPException(status_code=409, detail={
                "error": "strategy_saturated",
                "strategy_tag": req.strategy_tag,
                "suggestion": f"Too many active hypotheses in '{req.strategy_tag}'. Try a different strategy.",
            })

        hyp_id = new_id()
        fp = fingerprint(req.title, req.strategy_tag)
        timestamp = now()
        status = "claimed" if req.auto_claim else "proposed"
        claimed_by = req.agent_id if req.auto_claim else None

        await conn.execute(
            """INSERT INTO hypotheses
               (id, agent_id, title, description, strategy_tag, status, fingerprint,
                parent_hypothesis_id, claimed_by, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (hyp_id, req.agent_id, req.title, req.description, req.strategy_tag,
             status, fp, req.parent_hypothesis_id, claimed_by, timestamp),
        )
        await conn.commit()

        agent_name = await get_agent_name(conn, req.agent_id)

    await manager.broadcast({
        "type": "hypothesis_proposed",
        "hypothesis_id": hyp_id,
        "agent_name": agent_name,
        "agent_id": req.agent_id,
        "title": req.title,
        "strategy_tag": req.strategy_tag,
        "parent_hypothesis_id": req.parent_hypothesis_id,
        "timestamp": timestamp,
    })

    return HypothesisResponse(hypothesis_id=hyp_id, status=status, fingerprint=fp)


@app.get("/api/hypotheses")
async def list_hypotheses(status: str | None = None, strategy_tag: str | None = None):
    async with db.connect() as conn:
        query = "SELECT h.*, a.name as agent_name FROM hypotheses h JOIN agents a ON a.id = h.agent_id WHERE 1=1"
        params = []
        if status:
            query += " AND h.status = ?"
            params.append(status)
        if strategy_tag:
            query += " AND h.strategy_tag = ?"
            params.append(strategy_tag)
        query += " ORDER BY h.created_at DESC"
        cursor = await conn.execute(query, params)
        return [dict(row) for row in await cursor.fetchall()]


# ── Experiment endpoints ──

@app.post("/api/experiments", response_model=ExperimentResponse)
async def create_experiment(req: ExperimentCreate):
    config = await get_config_cached()
    baseline = get_baseline(config)

    exp_id = new_id()
    timestamp = now()
    route_data_json = json.dumps(req.route_data) if req.route_data else None

    async with db.connect() as conn:
        await conn.execute(
            """INSERT INTO experiments
               (id, agent_id, hypothesis_id, algorithm_diff, score, feasible,
                num_vehicles, total_distance, runtime_seconds, notes, route_data, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (exp_id, req.agent_id, req.hypothesis_id, req.algorithm_diff, req.score,
             1 if req.feasible else 0, req.num_vehicles, req.total_distance,
             req.runtime_seconds, req.notes, route_data_json, timestamp),
        )

        await conn.execute(
            "UPDATE agents SET experiments_completed = experiments_completed + 1 WHERE id = ?",
            (req.agent_id,),
        )

        if req.feasible:
            await conn.execute(
                "UPDATE agents SET best_score = MIN(COALESCE(best_score, ?), ?) WHERE id = ?",
                (req.score, req.score, req.agent_id),
            )

        prev_best = await db.get_global_best(conn)
        is_new_best = req.feasible and (prev_best is None or req.score < prev_best["score"])

        hyp_status = None
        if req.hypothesis_id:
            hyp_status = "succeeded" if (is_new_best or (req.feasible and req.score < baseline)) else "failed"
            await conn.execute(
                "UPDATE hypotheses SET status = ? WHERE id = ?",
                (hyp_status, req.hypothesis_id),
            )

        await conn.commit()

        agent_name = await get_agent_name(conn, req.agent_id)
        leaderboard = await db.compute_leaderboard(conn, baseline)
        rank = next((e["rank"] for e in leaderboard if e["agent_id"] == req.agent_id), 0)

    imp = improvement_pct(baseline, req.score)

    await manager.broadcast({
        "type": "experiment_published",
        "experiment_id": exp_id,
        "agent_name": agent_name,
        "agent_id": req.agent_id,
        "score": req.score,
        "feasible": req.feasible,
        "improvement_pct": imp,
        "is_new_best": is_new_best,
        "hypothesis_id": req.hypothesis_id,
        "notes": req.notes,
        "timestamp": timestamp,
    })

    if is_new_best:
        await manager.broadcast({
            "type": "new_global_best",
            "experiment_id": exp_id,
            "agent_name": agent_name,
            "agent_id": req.agent_id,
            "score": req.score,
            "improvement_pct": imp,
            "route_data": req.route_data,
            "timestamp": timestamp,
        })

    await manager.broadcast({
        "type": "leaderboard_update",
        "entries": leaderboard,
        "timestamp": timestamp,
    })

    return ExperimentResponse(
        experiment_id=exp_id,
        is_new_best=is_new_best,
        rank=rank,
        improvement_over_baseline_pct=imp,
        hypothesis_status_updated_to=hyp_status,
    )


# ── Leaderboard ──

@app.get("/api/leaderboard")
async def get_leaderboard():
    config = await get_config_cached()
    baseline = get_baseline(config)
    async with db.connect() as conn:
        leaderboard = await db.compute_leaderboard(conn, baseline)
    return {"updated_at": now(), "baseline_score": baseline, "entries": leaderboard}


# ── Admin endpoints ──

@app.post("/api/admin/broadcast")
async def admin_broadcast(req: AdminBroadcast):
    await verify_admin(req)
    await manager.broadcast({
        "type": "admin_broadcast",
        "message": req.message,
        "priority": req.priority,
        "timestamp": now(),
    })
    return {"sent": True}


@app.post("/api/admin/reset")
async def admin_reset(req: AdminAuth):
    await verify_admin(req)
    async with db.connect() as conn:
        await conn.execute("DELETE FROM experiments")
        await conn.execute("DELETE FROM hypotheses")
        await conn.execute("DELETE FROM agents")
        await conn.commit()
    await manager.broadcast({"type": "reset", "timestamp": now()})
    return {"reset": True}


@app.post("/api/admin/config")
async def admin_config(req: AdminAuth, key: str = "", value: str = ""):
    global _config_cache
    await verify_admin(req)
    if key and value:
        async with db.connect() as conn:
            await conn.execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)",
                (key, value),
            )
            await conn.commit()
        _config_cache = None  # invalidate cache
    return {"updated": True}


# ── WebSocket ──

@app.websocket("/ws/dashboard")
async def websocket_endpoint(ws: WebSocket):
    await manager.connect(ws)
    try:
        while True:
            await ws.receive_text()
    except WebSocketDisconnect:
        manager.disconnect(ws)


# ── Health ──

@app.get("/health")
async def health():
    return {"status": "ok", "timestamp": now()}
