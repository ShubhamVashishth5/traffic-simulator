import type { Scenario } from "./schema";

/** One-line description of a scenario, used in the viewer header. */
export function describeScenario(s: Scenario): string {
  const minutes = Math.round(s.duration_s / 60);
  const entries = s.demand.entries.length;
  return `${s.name}: ${minutes} min simulated, ${entries} entr${entries === 1 ? "y" : "ies"}`;
}
