import { describe, expect, it } from "vitest";
import { type DetRow, fdPoints, histogram, intervals, lateralByClass } from "./analytics";

const screen = (vt: string, cls: string, count: number, sms: number, hist: number[], t = 0, warm = false): DetRow => ({
  detector_id: "s",
  kind: "screenline",
  t_start: t,
  t_end: t + 60,
  in_warmup: warm,
  vehicle_type: vt,
  class: cls,
  count,
  flow: count / 60,
  space_mean_speed: sms,
  lateral_y0: -5,
  lateral_bin: 0.5,
  lateral_hist: hist,
});

describe("intervals", () => {
  it("sums flows and takes the harmonic mean across types at a screenline", () => {
    const iv = intervals([screen("hatchback", "car", 3, 10, [1, 2]), screen("motorcycle", "two_wheeler", 1, 20, [0, 1])]);
    expect(iv).toHaveLength(1);
    expect(iv[0].count).toBe(4);
    expect(iv[0].flow).toBeCloseTo(4 / 60, 12);
    // 4 vehicles: 3 at harmonic 10 m/s, 1 at 20 m/s → 4 / (3/10 + 1/20)
    expect(iv[0].speed).toBeCloseTo(4 / (0.3 + 0.05), 12);
  });

  it("uses Edie's definitions for areas: densities and flows add, speed is ΣTTD/ΣTTS", () => {
    const area = (tts: number, ttd: number): DetRow => ({
      detector_id: "a",
      kind: "area",
      t_start: 0,
      t_end: 60,
      in_warmup: false,
      vehicle_type: "x",
      class: "car",
      count: 1,
      tts,
      ttd,
      density: tts / (1000 * 60),
      flow: ttd / (1000 * 60),
    });
    const iv = intervals([area(100, 1000), area(50, 800)]);
    expect(iv[0].density).toBeCloseTo(150 / 60000, 15);
    expect(iv[0].flow).toBeCloseTo(1800 / 60000, 15);
    expect(iv[0].speed).toBeCloseTo(1800 / 150, 12);
    expect(fdPoints(iv)).toHaveLength(1);
  });
});

describe("lateralByClass", () => {
  it("adds histograms by class after warm-up", () => {
    const rows = [
      screen("hatchback", "car", 3, 10, [1, 2]),
      screen("sedan", "car", 1, 10, [0, 1], 60),
      screen("sedan", "car", 9, 10, [9, 0], 0, true),
    ];
    const h = lateralByClass(rows, "s");
    expect(h).toEqual([{ cls: "car", y0: -5, bin: 0.5, counts: [1, 3], total: 4 }]);
  });
});

describe("histogram", () => {
  it("bins from zero and clamps the last bin", () => {
    expect(histogram([0.2, 1.5, 1.7, 9, NaN], 1, 3)).toEqual([1, 2, 1]);
  });
});

import { thinTimes } from "./live/client";

describe("thinTimes", () => {
  it("keeps every other sample time with all its samples", () => {
    const ts = { ribbon: "r", t: [0, 0, 1, 1, 2, 3, 3], s: [1, 2, 3, 4, 5, 6, 7], v: [0, 0, 0, 0, 0, 0, 0] };
    expect(thinTimes(ts)).toEqual({ ribbon: "r", t: [0, 0, 2], s: [1, 2, 5], v: [0, 0, 0] });
  });
});
