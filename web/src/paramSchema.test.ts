import { describe as group, expect, it } from "vitest";
import { currentValue, describe, distForms, flatten, locationField, readOnlyReason } from "./paramSchema";
import type { RegisterFile } from "./register";

const file: RegisterFile = {
  group: "vehicles",
  entries: {
    "free_flow_speed.hatchback": {
      value: { dist: "trunc_normal", mean: 60.61, sd: 15.52, min: 15, max: 120 },
      unit: "km/h",
      grade: "A",
      role: "fit",
      stage: 1,
      source: "s",
      guide: "§1",
      live: true,
    },
    "length.bus": { value: 11, unit: "m", grade: "B", role: "fix", source: "s", guide: "§1" },
  },
};

group("parameter panel schema", () => {
  it("reads the distribution forms from the register JSON Schema", () => {
    const forms = distForms();
    const tn = forms.find((f) => f.key === "trunc_normal");
    expect(tn?.fields.sort()).toEqual(["max", "mean", "min", "sd"]);
    expect(forms.find((f) => f.key === "normal")?.fields).toEqual(["mean", "sd"]);
    expect(forms.some((f) => f.key === "mixture")).toBe(false);
  });

  it("groups like the register and resolves the value shown", () => {
    const ps = flatten([file]);
    expect(ps.map((p) => [p.path, p.section])).toEqual([
      ["vehicles.free_flow_speed.hatchback", "free_flow_speed"],
      ["vehicles.length.bus", "length"],
    ]);
    const changes = [{ path: "vehicles.free_flow_speed.hatchback", after: { dist: "trunc_normal", mean: 70, sd: 15.52, min: 15, max: 120 } }];
    expect(describe(currentValue(ps[0], undefined, changes))).toBe("trunc_normal(mean=70, sd=15.52, min=15, max=120)");
    expect(currentValue(ps[1], { "vehicles.length.bus": 12 }, changes)).toBe(12);
    expect(locationField(ps[0].entry.value!)).toBe("mean");
    expect(readOnlyReason(ps[1].entry)).toMatch(/measured/i);
  });
});
