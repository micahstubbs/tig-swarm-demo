import type { Panel, WSMessage } from "../types";
import { counterTween, pulseGlow } from "../lib/animate";


export class StatsPanel implements Panel {
  private agentsEl!: HTMLElement;
  private experimentsEl!: HTMLElement;
  private improvementEl!: HTMLElement;
  private heroEl!: HTMLElement;

  init(container: HTMLElement) {
    container.innerHTML = `
      <div class="stats-bar">
        <div class="stats-logo">
          <span class="stats-diamond">&#9670;</span>
          <span class="stats-title">Automated Discovery</span>
          <span id="ws-status" class="ws-status connected">LIVE</span>
          <a href="/ideas.html" class="stats-nav-link">Ideas &rarr;</a>
          <a href="/diversity.html" class="stats-nav-link">Diversity &rarr;</a>
          <a href="/benchmark.html" class="stats-nav-link">Benchmark &rarr;</a>
        </div>
        <div class="stats-chips">
          <div class="stat-chip" id="stat-agents">
            <span class="stat-label">AGENTS</span>
            <span class="stat-value" id="stat-agents-val">0</span>
          </div>
          <div class="stat-chip" id="stat-experiments">
            <span class="stat-label">EXPERIMENTS</span>
            <span class="stat-value" id="stat-experiments-val">0</span>
          </div>
          <div class="stat-chip" id="stat-improvement">
            <span class="stat-label">IMPROVEMENT</span>
            <span class="stat-value" id="stat-improvement-val" title="No feasible experiment yet">—</span>
          </div>
          <div class="stat-hero" id="stat-hero"></div>
        </div>
      </div>
    `;

    this.agentsEl = document.getElementById("stat-agents-val")!;
    this.experimentsEl = document.getElementById("stat-experiments-val")!;
    this.improvementEl = document.getElementById("stat-improvement-val")!;
    this.heroEl = document.getElementById("stat-hero")!;

  }

  handleMessage(msg: WSMessage) {
    if (msg.type === "reset") {
      this.agentsEl.textContent = "0";
      this.experimentsEl.textContent = "0";
      this.improvementEl.textContent = "—";
      this.improvementEl.style.color = "";
      this.improvementEl.title = "No feasible experiment yet";
      this.heroEl.textContent = "";
      this.heroEl.style.opacity = "0";
      return;
    }

    if (msg.type === "stats_update") {
      counterTween(this.agentsEl, msg.total_agents ?? msg.active_agents);
      counterTween(this.experimentsEl, msg.total_experiments);

      // improvement_pct is improvement-positive (positive = score dropped).
      // We display it as a score change: an improvement of 5% shows "-5.0%"
      // in green, a 5% regression shows "+5.0%" in red — the sign tracks the
      // direction of the score, not the direction of improvement.
      //
      // There is no meaningful "improvement" until at least two feasible
      // experiments have landed. Show an em-dash in those cases so a visitor
      // doesn't mistake 0.0% for "we did a lot of work and made zero
      // progress" — they're distinct states.
      const impEl = this.improvementEl;
      const target = msg.improvement_pct;
      const hasBaseline = msg.baseline_score != null && msg.best_score != null;
      const hasRoomToMove = hasBaseline && msg.baseline_score !== msg.best_score;
      if (!hasBaseline || !hasRoomToMove) {
        impEl.textContent = "—";
        impEl.style.color = "";
        impEl.title = !hasBaseline
          ? "No feasible experiment yet"
          : "First global best — improvement metric unlocks after the second feasible experiment";
      } else {
        const scoreChange = -target;
        const sign = scoreChange >= 0 ? "+" : "";
        impEl.textContent = `${sign}${scoreChange.toFixed(1)}%`;
        impEl.style.color = target > 0 ? "var(--green)" : target < 0 ? "var(--red)" : "";
        impEl.title = "Score change vs the first feasible experiment";
      }
    }

    if (msg.type === "agent_joined") {
      pulseGlow(document.getElementById("stat-agents")!);
    }

    if (msg.type === "experiment_published") {
      pulseGlow(document.getElementById("stat-experiments")!);
    }

    if (msg.type === "new_global_best") {
      this.heroEl.textContent = msg.agent_name;
      this.heroEl.style.opacity = "1";
      pulseGlow(document.getElementById("stat-improvement")!, "#ffab00");
      setTimeout(() => {
        this.heroEl.style.opacity = "0";
      }, 5000);
    }
  }
}
