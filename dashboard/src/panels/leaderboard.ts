import type { Panel, WSMessage, LeaderboardEntry } from "../types";
import { getAgentColor } from "../lib/colors";

export class LeaderboardPanel implements Panel {
  private list!: HTMLElement;
  private currentEntries: LeaderboardEntry[] = [];

  init(container: HTMLElement) {
    container.innerHTML = `
      <div class="panel-inner">
        <div class="panel-label">LEADERBOARD</div>
        <div class="leaderboard-header">
          <span class="lb-rank">#</span>
          <span class="lb-name">Agent</span>
          <span class="lb-score">Score</span>
          <span class="lb-exp">Runs</span>
        </div>
        <div class="leaderboard-list" id="leaderboard-list"></div>
      </div>
    `;
    this.list = document.getElementById("leaderboard-list")!;
  }

  handleMessage(msg: WSMessage) {
    if (msg.type !== "leaderboard_update") return;

    const entries = msg.entries.slice(0, 10);

    // Record first positions for FLIP
    const firstRects = new Map<string, DOMRect>();
    Array.from(this.list.children).forEach((child) => {
      const el = child as HTMLElement;
      firstRects.set(el.dataset.agentId || "", el.getBoundingClientRect());
    });

    // Determine which scores improved
    const prevScores = new Map<string, number>();
    this.currentEntries.forEach((e) => prevScores.set(e.agent_id, e.best_score));

    // Render
    this.list.innerHTML = "";
    entries.forEach((entry) => {
      const row = document.createElement("div");
      row.className = "leaderboard-row";
      row.dataset.agentId = entry.agent_id;

      const rankClass =
        entry.rank === 1 ? "rank-gold" :
        entry.rank === 2 ? "rank-cyan" :
        entry.rank === 3 ? "rank-teal" : "";

      const color = getAgentColor(entry.agent_id);
      const improved = prevScores.has(entry.agent_id) &&
        entry.best_score < prevScores.get(entry.agent_id)!;

      row.innerHTML = `
        <span class="lb-rank ${rankClass}">${entry.rank}</span>
        <span class="lb-name">
          <span class="lb-dot" style="background:${color}"></span>
          ${entry.agent_name}
        </span>
        <span class="lb-score ${improved ? "lb-score--improved" : ""}">${entry.best_score.toFixed(1)}</span>
        <span class="lb-exp">${entry.experiments_completed}</span>
      `;

      if (entry.rank <= 3) {
        const accentColor = entry.rank === 1 ? "var(--amber)" : entry.rank === 2 ? "var(--cyan)" : "var(--teal)";
        row.style.borderLeft = `2px solid ${accentColor}`;
        row.style.boxShadow = `inset 4px 0 12px -4px ${accentColor}44`;
      }

      this.list.appendChild(row);
    });

    // FLIP animation
    if (firstRects.size > 0) {
      Array.from(this.list.children).forEach((child) => {
        const el = child as HTMLElement;
        const agentId = el.dataset.agentId || "";
        const first = firstRects.get(agentId);
        if (!first) {
          // New entry - fade in
          el.style.opacity = "0";
          el.style.transform = "translateX(20px)";
          requestAnimationFrame(() => {
            el.style.transition = "opacity 0.4s ease, transform 0.4s ease";
            el.style.opacity = "1";
            el.style.transform = "translateX(0)";
            setTimeout(() => { el.style.transition = ""; }, 400);
          });
          return;
        }

        const last = el.getBoundingClientRect();
        const deltaY = first.top - last.top;
        if (Math.abs(deltaY) < 1) return;

        el.style.transform = `translateY(${deltaY}px)`;
        el.style.transition = "none";

        requestAnimationFrame(() => {
          el.style.transition = "transform 0.5s cubic-bezier(0.4, 0, 0.2, 1)";
          el.style.transform = "";
          setTimeout(() => { el.style.transition = ""; }, 500);
        });
      });
    }

    this.currentEntries = entries;
  }
}
