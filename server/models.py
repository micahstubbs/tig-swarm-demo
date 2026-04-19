import re
from pydantic import BaseModel, Field, field_validator
from typing import Literal, Optional
import uuid


def new_id() -> str:
    return uuid.uuid4().hex[:12]


def improvement_pct(baseline: float, score: float) -> float:
    if baseline <= 0:
        return 0.0
    return round(((baseline - score) / baseline) * 100, 2)


_HTML_TAG_RE = re.compile(r'<[^>]+>')

def _strip_html(v: str) -> str:
    if not isinstance(v, str):
        return v
    return _HTML_TAG_RE.sub('', v).strip()


# ── Request models ──

class RegisterRequest(BaseModel):
    client_version: str = Field(default="1.0", max_length=50)


class HeartbeatRequest(BaseModel):
    status: Literal["idle", "working"] = "working"
    current_hypothesis_id: Optional[str] = Field(default=None, max_length=50)
    agent_token: Optional[str] = None


class HypothesisCreate(BaseModel):
    agent_id: str = Field(max_length=50)
    title: str = Field(max_length=200)
    description: str = Field(max_length=2000)
    strategy_tag: Literal[
        "construction",
        "local_search",
        "metaheuristic",
        "constraint_relaxation",
        "decomposition",
        "hybrid",
        "data_structure",
        "other",
    ]
    parent_hypothesis_id: Optional[str] = Field(default=None, max_length=50)
    agent_token: Optional[str] = Field(default=None, max_length=100)

    @field_validator('title', 'description', mode='before')
    @classmethod
    def sanitize_text(cls, v: str) -> str:
        return _strip_html(v)


class ExperimentCreate(BaseModel):
    agent_id: str = Field(max_length=50)
    hypothesis_id: Optional[str] = Field(default=None, max_length=50)
    algorithm_code: str = Field(default="", max_length=500000)
    score: float
    feasible: bool = True
    num_vehicles: int = 0
    total_distance: float = 0.0
    runtime_seconds: float = 0.0
    notes: str = Field(default="", max_length=2000)
    route_data: Optional[dict] = None
    agent_token: Optional[str] = Field(default=None, max_length=100)
    agent_aliases: list[str] = Field(default_factory=list, max_length=16)

    @field_validator("agent_aliases", mode="before")
    @classmethod
    def sanitize_aliases(cls, v):
        if v is None:
            return []
        if not isinstance(v, list):
            return []
        return [_strip_html(str(item)) for item in v if str(item).strip()]


class IterationCreate(BaseModel):
    agent_id: str = Field(max_length=50)
    title: str = Field(max_length=200)
    description: str = Field(default="", max_length=2000)
    strategy_tag: Literal[
        "construction",
        "local_search",
        "metaheuristic",
        "constraint_relaxation",
        "decomposition",
        "hybrid",
        "data_structure",
        "other",
    ] = "other"
    algorithm_code: str = Field(default="", max_length=500000)
    score: float
    feasible: bool = True
    num_vehicles: int = 0
    total_distance: float = 0.0
    notes: str = Field(default="", max_length=2000)
    route_data: Optional[dict] = None
    agent_token: Optional[str] = Field(default=None, max_length=100)
    agent_aliases: list[str] = Field(default_factory=list, max_length=16)

    @field_validator("agent_aliases", mode="before")
    @classmethod
    def sanitize_aliases(cls, v):
        if v is None:
            return []
        if not isinstance(v, list):
            return []
        return [_strip_html(str(item)) for item in v if str(item).strip()]


class AdminAuth(BaseModel):
    admin_key: str = Field(max_length=200)


class AdminBroadcast(AdminAuth):
    message: str = Field(max_length=2000)
    priority: Literal["normal", "high"] = "normal"

    @field_validator('message', mode='before')
    @classmethod
    def sanitize_text(cls, v: str) -> str:
        return _strip_html(v)


class MessageCreate(BaseModel):
    agent_id: Optional[str] = Field(default=None, max_length=50)
    agent_name: str = Field(max_length=100)
    content: str = Field(max_length=2000)
    msg_type: Literal["agent", "milestone"] = "agent"
    agent_token: Optional[str] = Field(default=None, max_length=100)

    @field_validator('agent_name', 'content', mode='before')
    @classmethod
    def sanitize_text(cls, v: str) -> str:
        return _strip_html(v)


# ── Response models ──

class AgentResponse(BaseModel):
    agent_id: str
    agent_name: str
    registered_at: str
    config: dict
    agent_token: str


class HypothesisResponse(BaseModel):
    hypothesis_id: str
    status: str
    fingerprint: str


class ExperimentResponse(BaseModel):
    experiment_id: str
    is_new_best: bool
    rank: int
    improvement_over_baseline_pct: float
    hypothesis_status_updated_to: Optional[str] = None


class IterationResponse(BaseModel):
    experiment_id: str
    hypothesis_id: str
    is_new_best: bool
    beats_own_best: bool
    rank: int
    runs: int
    improvements: int
    runs_since_improvement: int
