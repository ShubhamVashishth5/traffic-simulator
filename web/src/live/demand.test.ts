import { describe, expect, it } from "vitest";
import { classShares, flowAt, setClassShare, typesFromClasses } from "./demand";

const classOf = { motorcycle: "two_wheeler", scooter: "two_wheeler", hatchback: "car", sedan: "car", bus: "bus" };
const typesOfClass = { two_wheeler: ["scooter", "motorcycle"], car: ["hatchback", "sedan"], bus: ["bus"], truck: ["truck"] };

describe("live demand helpers", () => {
  it("sums type shares into class shares", () => {
    const s = classShares({ motorcycle: 2, scooter: 2, hatchback: 3, sedan: 1, bus: 2 }, classOf);
    expect(s.two_wheeler).toBeCloseTo(0.4);
    expect(s.car).toBeCloseTo(0.4);
    expect(s.bus).toBeCloseTo(0.2);
  });

  it("sets one class and rescales the others", () => {
    const s = setClassShare({ two_wheeler: 0.5, car: 0.3, bus: 0.2 }, "bus", 0.6);
    expect(s.bus).toBeCloseTo(0.6);
    expect(s.two_wheeler).toBeCloseTo(0.25);
    expect(s.car).toBeCloseTo(0.15);
    expect(setClassShare({ car: 1 }, "car", 0.3)).toEqual({ car: 1 });
    expect(setClassShare({ car: 1 }, "bus", 0.25)).toEqual({ car: 0.75, bus: 0.25 });
    expect(setClassShare({ car: 0.5, bus: 0.5 }, "bus", 0)).toEqual({ car: 1 });
  });

  it("splits classes into types like the entry, then like all entries, then equally", () => {
    const t = typesFromClasses({ two_wheeler: 0.5, car: 0.25, truck: 0.25 }, { motorcycle: 3, scooter: 1 }, { hatchback: 1 }, typesOfClass);
    expect(t.motorcycle).toBeCloseTo(0.375);
    expect(t.scooter).toBeCloseTo(0.125);
    expect(t.hatchback).toBeCloseTo(0.25);
    expect(t.sedan).toBeUndefined();
    expect(t.truck).toBeCloseTo(0.25);
    const eq = typesFromClasses({ car: 1 }, {}, {}, typesOfClass);
    expect(eq).toEqual({ hatchback: 0.5, sedan: 0.5 });
  });

  it("reads a flow profile", () => {
    const f = [
      { from_s: 0, veh_per_h: 100 },
      { from_s: 600, veh_per_h: 300 },
    ];
    expect(flowAt(f, 10)).toBe(100);
    expect(flowAt(f, 600)).toBe(300);
  });
});
