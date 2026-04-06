import * as d3 from "d3";
import type { Panel, WSMessage, RouteData, RoutePoint } from "../types";
import { getRouteColor } from "../lib/colors";

export class RoutesPanel implements Panel {
  private svg!: any;
  private routeGroup!: any;
  private ghostGroup!: any;
  private customerGroup!: any;
  private depotGroup!: any;
  private scoreEl!: HTMLElement;
  private deltaEl!: HTMLElement;
  private currentRouteData: RouteData | null = null;

  init(container: HTMLElement) {
    container.innerHTML = `
      <div class="panel-inner routes-panel">
        <div class="panel-label">ROUTES</div>
        <svg id="routes-svg"></svg>
        <div class="routes-score">
          <div class="routes-score-label">TOTAL DISTANCE</div>
          <div class="routes-score-value" id="routes-score">---</div>
        </div>
        <div class="routes-delta" id="routes-delta"></div>
      </div>
    `;

    this.scoreEl = document.getElementById("routes-score")!;
    this.deltaEl = document.getElementById("routes-delta")!;

    this.svg = d3.select("#routes-svg");
    this.svg
      .attr("viewBox", "0 0 1000 1000")
      .attr("preserveAspectRatio", "xMidYMid meet");

    // Glow filter
    const defs = this.svg.append("defs");
    const filter = defs.append("filter").attr("id", "route-glow");
    filter.append("feGaussianBlur").attr("stdDeviation", "1.5").attr("result", "blur");
    const merge = filter.append("feMerge");
    merge.append("feMergeNode").attr("in", "blur");
    merge.append("feMergeNode").attr("in", "SourceGraphic");

    // Shockwave gradient
    const radGrad = defs.append("radialGradient").attr("id", "shockwave-grad");
    radGrad.append("stop").attr("offset", "0%").attr("stop-color", "#00e5ff").attr("stop-opacity", "0");
    radGrad.append("stop").attr("offset", "70%").attr("stop-color", "#00e5ff").attr("stop-opacity", "0.2");
    radGrad.append("stop").attr("offset", "100%").attr("stop-color", "#00e5ff").attr("stop-opacity", "0");

    this.ghostGroup = this.svg.append("g").attr("class", "ghost-routes");
    this.routeGroup = this.svg.append("g").attr("class", "routes");
    this.customerGroup = this.svg.append("g").attr("class", "customers");
    this.depotGroup = this.svg.append("g").attr("class", "depot");
  }

  handleMessage(msg: WSMessage) {
    if (msg.type === "new_global_best" && msg.route_data) {
      this.animateNewBest(msg.route_data, msg.score, msg.improvement_pct);
    }
    if (msg.type === "stats_update" && msg.best_score && !this.currentRouteData) {
      this.scoreEl.textContent = msg.best_score.toFixed(1);
    }
  }

  private animateNewBest(data: RouteData, score: number, improvementPct: number) {
    const oldData = this.currentRouteData;
    this.currentRouteData = data;

    // Move current routes to ghost layer
    if (oldData) {
      this.ghostGroup.selectAll("*").remove();
      this.drawRoutes(this.ghostGroup, oldData, true);
      this.ghostGroup.transition().duration(600).style("opacity", 0.06);
      setTimeout(() => this.ghostGroup.selectAll("*").remove(), 12000);
    }

    // Clear main routes
    this.routeGroup.selectAll("*").remove();
    this.customerGroup.selectAll("*").remove();
    this.depotGroup.selectAll("*").remove();

    // Draw new routes with animation
    this.drawRoutesAnimated(data);

    // Shockwave
    this.shockwave(data.depot.x, data.depot.y);

    // Score update
    this.scoreEl.textContent = score.toFixed(1);

    // Delta text
    this.deltaEl.textContent = `+${improvementPct.toFixed(1)}% improvement`;
    this.deltaEl.style.opacity = "1";
    this.deltaEl.style.transform = "translateY(0)";
    setTimeout(() => {
      this.deltaEl.style.transition = "opacity 1s ease, transform 1s ease";
      this.deltaEl.style.opacity = "0";
      this.deltaEl.style.transform = "translateY(-10px)";
      setTimeout(() => { this.deltaEl.style.transition = ""; }, 1000);
    }, 2500);
  }

  private drawRoutes(
    group: d3.Selection<SVGGElement, unknown, null, undefined>,
    data: RouteData,
    ghost = false,
  ) {
    const line = d3.line<RoutePoint>()
      .x((d) => d.x)
      .y((d) => d.y)
      .curve(d3.curveCatmullRom.alpha(0.5));

    data.routes.forEach((route, i) => {
      const fullPath = [
        { x: data.depot.x, y: data.depot.y, customer_id: -1 },
        ...route.path,
        { x: data.depot.x, y: data.depot.y, customer_id: -1 },
      ];
      const color = getRouteColor(i);

      // Trail (glow)
      if (!ghost) {
        group.append("path")
          .datum(fullPath)
          .attr("d", line as any)
          .attr("fill", "none")
          .attr("stroke", color)
          .attr("stroke-width", 20)
          .attr("stroke-opacity", 0.1)
          .attr("filter", "url(#route-glow)");
      }

      // Main path
      group.append("path")
        .datum(fullPath)
        .attr("d", line as any)
        .attr("fill", "none")
        .attr("stroke", color)
        .attr("stroke-width", ghost ? 5 : 8)
        .attr("stroke-opacity", ghost ? 0.3 : 0.85)
        .attr("stroke-dasharray", ghost ? "none" : "20 8")
        .attr("class", ghost ? "" : "route-flowing");
    });
  }

  private drawRoutesAnimated(data: RouteData) {
    const line = d3.line<RoutePoint>()
      .x((d) => d.x)
      .y((d) => d.y)
      .curve(d3.curveCatmullRom.alpha(0.5));

    data.routes.forEach((route, i) => {
      const fullPath = [
        { x: data.depot.x, y: data.depot.y, customer_id: -1 },
        ...route.path,
        { x: data.depot.x, y: data.depot.y, customer_id: -1 },
      ];
      const color = getRouteColor(i);

      // Trail (glow)
      this.routeGroup.append("path")
        .datum(fullPath)
        .attr("d", line as any)
        .attr("fill", "none")
        .attr("stroke", color)
        .attr("stroke-width", 20)
        .attr("stroke-opacity", 0)
        .attr("filter", "url(#route-glow)")
        .transition()
        .delay(i * 100)
        .duration(800)
        .attr("stroke-opacity", 0.1);

      // Main path with draw-in animation
      const path = this.routeGroup.append("path")
        .datum(fullPath)
        .attr("d", line as any)
        .attr("fill", "none")
        .attr("stroke", color)
        .attr("stroke-width", 8)
        .attr("stroke-opacity", 0.85)
        .attr("class", "route-flowing");

      // Animate stroke dash
      const node = path.node()!;
      const totalLength = node.getTotalLength();
      path
        .attr("stroke-dasharray", `${totalLength}`)
        .attr("stroke-dashoffset", totalLength)
        .transition()
        .delay(i * 100)
        .duration(1200)
        .ease(d3.easeCubicInOut)
        .attr("stroke-dashoffset", 0)
        .on("end", function (this: SVGPathElement) {
          d3.select(this)
            .attr("stroke-dasharray", "20 8")
            .attr("stroke-dashoffset", 0);
        });

      // Customers
      route.path.forEach((pt) => {
        this.customerGroup.append("circle")
          .attr("cx", pt.x)
          .attr("cy", pt.y)
          .attr("r", 0)
          .attr("fill", color)
          .attr("opacity", 0.7)
          .transition()
          .delay(i * 100 + 400)
          .duration(300)
          .attr("r", 10);
      });
    });

    // Depot
    const depotSize = 25;
    this.depotGroup.append("rect")
      .attr("x", data.depot.x - depotSize / 2)
      .attr("y", data.depot.y - depotSize / 2)
      .attr("width", depotSize)
      .attr("height", depotSize)
      .attr("fill", "#fff")
      .attr("transform", `rotate(45, ${data.depot.x}, ${data.depot.y})`)
      .attr("class", "depot-pulse")
      .attr("opacity", 0)
      .transition()
      .duration(400)
      .attr("opacity", 0.9);
  }

  private shockwave(cx: number, cy: number) {
    const ring = this.svg.append("circle")
      .attr("cx", cx)
      .attr("cy", cy)
      .attr("r", 20)
      .attr("fill", "none")
      .attr("stroke", "#00e5ff")
      .attr("stroke-width", 5)
      .attr("stroke-opacity", 0.4);

    ring.transition()
      .duration(1000)
      .ease(d3.easeCubicOut)
      .attr("r", 400)
      .attr("stroke-opacity", 0)
      .attr("stroke-width", 1)
      .remove();
  }
}
