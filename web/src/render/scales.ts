import { KMH_PER_MS } from "../format";
import type { ColourScale } from "./MapView";

/** Display domains of the map colour modes (values arrive in SI). */
export const SCALES: Record<string, ColourScale & { unit: string; factor: number }> = {
  class: { kind: "categorical", max: 0, unit: "", factor: 1 },
  speed: { kind: "sequential", max: 25, unit: "km/h", factor: KMH_PER_MS },
  desired_speed: { kind: "sequential", max: 25, unit: "km/h", factor: KMH_PER_MS },
  speed_ratio: { kind: "sequential", max: 1, unit: "", factor: 1 },
  accel: { kind: "diverging", max: 3, unit: "m/s²", factor: 1 },
  lateral: { kind: "diverging", max: 5, unit: "m", factor: 1 },
};
