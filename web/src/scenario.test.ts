import { describe, expect, it } from "vitest";
import { describeScenario } from "./scenario";
import type { Scenario } from "./schema";

const toy: Scenario = {
  name: "toy",
  duration_s: 600,
  network: { ribbons: [{ id: "r", polyline: [[0, 0], [1000, 0]], width: [{ s: 0, y_min: -3.5, y_max: 3.5 }] }] },
  demand: {
    entries: [{ id: "e", ribbon: "r", flow: [{ from_s: 0, veh_per_h: 1800 }], composition: { hatchback: 1 } }],
  },
};

describe("describeScenario", () => {
  it("formats name, duration and entries", () => {
    expect(describeScenario(toy)).toBe("toy: 10 min simulated, 1 entry");
  });
});
