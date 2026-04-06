const PALETTE = [
  "#00e5ff", "#ff6d00", "#00e676", "#e040fb", "#ffea00",
  "#ff5252", "#40c4ff", "#69f0ae", "#ff80ab", "#ffd740",
  "#b388ff", "#00bfa5", "#ffab00", "#18ffff", "#ff9100",
];

export const ROUTE_COLORS = PALETTE.slice(0, 10);

const agentColorMap = new Map<string, string>();
let colorIndex = 0;

export function getAgentColor(agentId: string): string {
  if (!agentColorMap.has(agentId)) {
    agentColorMap.set(agentId, PALETTE[colorIndex % PALETTE.length]);
    colorIndex++;
  }
  return agentColorMap.get(agentId)!;
}

export function getRouteColor(vehicleIndex: number): string {
  return ROUTE_COLORS[vehicleIndex % ROUTE_COLORS.length];
}
