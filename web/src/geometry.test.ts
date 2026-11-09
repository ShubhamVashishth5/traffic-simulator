import { describe, expect, it } from "vitest";
import { Ribbon, geometry } from "./geometry";

const spec = {
  id: "r",
  polyline: [
    [0, 0],
    [200, 0],
  ] as [number, number][],
  width: [
    { s: 0, y_min: -5, y_max: 5 },
    { s: 100, y_min: -3, y_max: 4 },
  ],
};

describe("Ribbon (port of marg_net::Ribbon)", () => {
  it("matches the Rust straight-ribbon test", () => {
    const r = new Ribbon(spec);
    expect(r.length).toBe(200);
    const [x, y, h] = r.toWorld(50, 2);
    expect(x).toBeCloseTo(50, 12);
    expect(y).toBeCloseTo(2, 12);
    expect(h).toBeCloseTo(0, 12);
    expect(r.yBounds(-10)).toEqual([-5, 5]);
    expect(r.yBounds(50)).toEqual([-4, 4.5]);
    expect(r.yBounds(500)).toEqual([-3, 4]);
  });
  it("bends: the left normal follows each segment", () => {
    const r = new Ribbon({ ...spec, polyline: [[0, 0], [100, 0], [100, 100]] });
    const [x, y, h] = r.toWorld(150, 1);
    expect([x, y]).toEqual([99, 50]);
    expect(h).toBeCloseTo(Math.PI / 2, 12);
    expect(r.outlineStations().length).toBe(5);
  });
  it("draws relative to the server's origin", () => {
    const g = geometry(
      { name: "x", duration_s: 1, network: { ribbons: [{ ...spec, polyline: [[100.5, 50], [700, 50]] }] }, demand: { entries: [] }, detectors: [{ kind: "screenline", id: "s", ribbon: "r", s: 10 }] },
      1,
    );
    expect(g.origin).toEqual([100, 50]);
    expect(g.detectors[0].shape[0][0]).toBeCloseTo(10.5, 9);
  });
});
