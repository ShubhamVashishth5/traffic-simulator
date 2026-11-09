import { describe, expect, it } from "vitest";
import { describeScenario } from "./scenario";

describe("describeScenario", () => {
  it("formats name and duration", () => {
    expect(describeScenario({ name: "toy", duration_s: 600, toy_arrivals_per_h: 1 })).toBe(
      "toy: 10 min simulated",
    );
  });
});
