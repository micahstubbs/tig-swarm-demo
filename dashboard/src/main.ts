import "./style.css";
import { initParticles } from "./lib/particles";
import { SwarmWebSocket } from "./lib/websocket";
import { MockDataGenerator } from "./mock";
import { viewportFlash } from "./lib/animate";

import { StatsPanel } from "./panels/stats";
import { RoutesPanel } from "./panels/routes";
import { ChartPanel } from "./panels/chart";
import { IdeaFlowPanel } from "./panels/ideaflow";
import { FeedPanel } from "./panels/feed";
import { LeaderboardPanel } from "./panels/leaderboard";

import type { WSMessage, Panel } from "./types";

// ── Config ──
const params = new URLSearchParams(window.location.search);
const isMock = params.has("mock");
const wsUrl = params.get("ws") || `ws://${window.location.hostname}:8080/ws/dashboard`;

// Derive REST API URL from WS URL
function getApiUrl(): string {
  const explicit = params.get("api");
  if (explicit) return explicit;
  // Convert ws(s)://host/ws/dashboard -> http(s)://host
  return wsUrl
    .replace("ws://", "http://")
    .replace("wss://", "https://")
    .replace("/ws/dashboard", "");
}

// ── Background particles ──
const canvas = document.getElementById("particleCanvas") as HTMLCanvasElement;
initParticles(canvas);

// ── Initialize panels ──
const panels: Panel[] = [];

function initPanel(PanelClass: new () => Panel, containerId: string) {
  const panel = new PanelClass();
  const container = document.getElementById(containerId)!;
  panel.init(container);
  panels.push(panel);
  return panel;
}

initPanel(StatsPanel, "panel-stats");
initPanel(RoutesPanel, "panel-routes");
initPanel(ChartPanel, "panel-chart");
initPanel(IdeaFlowPanel, "panel-ideaflow");
initPanel(FeedPanel, "panel-feed");
initPanel(LeaderboardPanel, "panel-leaderboard");

// ── Message dispatch ──
function handleMessage(msg: WSMessage) {
  if (msg.type === "new_global_best") {
    viewportFlash("rgba(0, 229, 255, 0.03)", 150);
  }
  panels.forEach((panel) => panel.handleMessage(msg));
}

// ── Fetch initial state from REST API ──
async function loadInitialState(apiUrl: string) {
  try {
    const res = await fetch(`${apiUrl}/api/state`);
    if (!res.ok) return;
    const state = await res.json();

    // Emit stats
    handleMessage({
      type: "stats_update",
      active_agents: state.active_agents,
      total_experiments: state.recent_experiments?.length || 0,
      hypotheses_count: state.active_hypotheses?.length || 0,
      best_score: state.best_score,
      baseline_score: state.baseline_score,
      improvement_pct:
        state.baseline_score > 0
          ? Number((((state.baseline_score - state.best_score) / state.baseline_score) * 100).toFixed(2))
          : 0,
      timestamp: new Date().toISOString(),
    });

    // Emit route data if available
    if (state.best_route_data && state.best_score < state.baseline_score) {
      handleMessage({
        type: "new_global_best",
        experiment_id: state.best_experiment_id || "",
        agent_name: "swarm",
        agent_id: "",
        score: state.best_score,
        improvement_pct:
          Number((((state.baseline_score - state.best_score) / state.baseline_score) * 100).toFixed(2)),
        route_data: state.best_route_data,
        timestamp: new Date().toISOString(),
      });
    }

    // Emit leaderboard
    if (state.leaderboard?.length) {
      handleMessage({
        type: "leaderboard_update",
        entries: state.leaderboard,
        timestamp: new Date().toISOString(),
      });
    }

    // Replay recent experiments as feed items (most recent last so they stack correctly)
    const recent = (state.recent_experiments || []).slice().reverse();
    for (const exp of recent) {
      handleMessage({
        type: "experiment_published",
        experiment_id: exp.id || "",
        agent_name: exp.agent_name,
        agent_id: "",
        score: exp.score,
        feasible: exp.feasible !== false,
        improvement_pct: exp.improvement_pct || 0,
        is_new_best: false,
        hypothesis_id: null,
        notes: exp.notes || "",
        timestamp: exp.created_at || new Date().toISOString(),
      });
    }

    // Replay active hypotheses as feed items
    for (const hyp of state.active_hypotheses || []) {
      handleMessage({
        type: "hypothesis_proposed",
        hypothesis_id: hyp.id || "",
        agent_name: hyp.agent_name,
        agent_id: "",
        title: hyp.title,
        strategy_tag: hyp.strategy_tag,
        parent_hypothesis_id: null,
        timestamp: new Date().toISOString(),
      });
    }

    console.log("[Dashboard] Loaded initial state:", {
      agents: state.active_agents,
      experiments: state.recent_experiments?.length,
      bestScore: state.best_score,
    });
  } catch (e) {
    console.warn("[Dashboard] Failed to load initial state:", e);
  }
}

// ── Connect ──
if (isMock) {
  console.log("[Dashboard] Running in MOCK mode");
  const mock = new MockDataGenerator();
  mock.onMessage(handleMessage);
  mock.start();

  const wsEl = document.getElementById("ws-status");
  if (wsEl) {
    wsEl.textContent = "MOCK";
    wsEl.className = "ws-status connected";
  }
} else {
  const apiUrl = getApiUrl();
  console.log(`[Dashboard] Connecting to ${wsUrl}, API: ${apiUrl}`);

  // Load existing state after panels have laid out
  setTimeout(() => loadInitialState(apiUrl), 500);

  // Then connect WebSocket for live updates
  const ws = new SwarmWebSocket(wsUrl);
  ws.onMessage(handleMessage);
  ws.connect();
}
