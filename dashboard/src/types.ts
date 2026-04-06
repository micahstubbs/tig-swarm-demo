export interface RoutePoint {
  x: number;
  y: number;
  customer_id: number;
}

export interface VehicleRoute {
  vehicle_id: number;
  path: RoutePoint[];
}

export interface RouteData {
  depot: { x: number; y: number };
  routes: VehicleRoute[];
}

export interface LeaderboardEntry {
  rank: number;
  agent_id: string;
  agent_name: string;
  best_score: number;
  best_experiment_id: string;
  experiments_completed: number;
  improvement_pct: number;
}

// WebSocket message types
export type WSMessage =
  | AgentJoined
  | AgentOffline
  | HypothesisProposed
  | ExperimentPublished
  | NewGlobalBest
  | LeaderboardUpdate
  | StatsUpdate
  | AdminBroadcastMsg
  | ResetMsg;

export interface AgentJoined {
  type: "agent_joined";
  agent_id: string;
  agent_name: string;
  timestamp: string;
}

export interface AgentOffline {
  type: "agent_offline";
  agent_id: string;
  agent_name: string;
  timestamp: string;
}

export interface HypothesisProposed {
  type: "hypothesis_proposed";
  hypothesis_id: string;
  agent_name: string;
  agent_id: string;
  title: string;
  strategy_tag: string;
  parent_hypothesis_id: string | null;
  timestamp: string;
}

export interface ExperimentPublished {
  type: "experiment_published";
  experiment_id: string;
  agent_name: string;
  agent_id: string;
  score: number;
  feasible: boolean;
  improvement_pct: number;
  is_new_best: boolean;
  hypothesis_id: string | null;
  notes: string;
  timestamp: string;
}

export interface NewGlobalBest {
  type: "new_global_best";
  experiment_id: string;
  agent_name: string;
  agent_id: string;
  score: number;
  improvement_pct: number;
  route_data: RouteData | null;
  timestamp: string;
}

export interface LeaderboardUpdate {
  type: "leaderboard_update";
  entries: LeaderboardEntry[];
  timestamp: string;
}

export interface StatsUpdate {
  type: "stats_update";
  active_agents: number;
  total_experiments: number;
  hypotheses_count: number;
  best_score: number | null;
  baseline_score: number;
  improvement_pct: number;
  timestamp: string;
}

export interface AdminBroadcastMsg {
  type: "admin_broadcast";
  message: string;
  priority: "normal" | "high";
  timestamp: string;
}

export interface ResetMsg {
  type: "reset";
  timestamp: string;
}

// Panel interfaces
export interface Panel {
  init(container: HTMLElement): void;
  handleMessage(msg: WSMessage): void;
}
