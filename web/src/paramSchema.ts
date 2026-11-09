/**
 * The parameter panel's knowledge of values comes from the register JSON Schema
 * (`schema/register_file.schema.json`, generated from the Rust types): which distribution forms
 * exist, which numeric fields each needs, and the field descriptions. Nothing here is written by
 * hand per distribution.
 */
import schema from "../../schema/register_file.schema.json";
import type { ParamEntry, RegisterFile } from "./register";

export type ParamValue = NonNullable<ParamEntry["value"]>;

export interface DistForm {
  /** The `dist` tag, e.g. `trunc_normal`. */
  key: string;
  description: string;
  /** Required numeric fields, in schema order (with `dist` removed). */
  fields: string[];
}

type JsonSchema = { oneOf?: JsonSchema[]; properties?: Record<string, { const?: string; type?: string }>; required?: string[]; description?: string };

/** Every distribution form with numeric fields only (mixtures and empirical tables are shown read-only). */
export function distForms(): DistForm[] {
  const dist = (schema as unknown as { $defs: Record<string, JsonSchema> }).$defs.Dist;
  const out: DistForm[] = [];
  for (const v of dist.oneOf ?? []) {
    const props = v.properties ?? {};
    const key = props.dist?.const;
    if (!key) continue;
    const fields = (v.required ?? []).filter((f) => f !== "dist");
    if (!fields.every((f) => props[f]?.type === "number")) continue;
    out.push({ key, description: v.description ?? "", fields });
  }
  return out;
}

export function entryDescription(field: keyof ParamEntry): string {
  const pe = (schema as unknown as { $defs: Record<string, { properties: Record<string, { description?: string }> }> }).$defs.ParamEntry;
  return pe.properties[field as string]?.description ?? "";
}

export interface Param {
  path: string;
  group: string;
  /** Second path segment (sub-group in the panel). */
  section: string;
  entry: ParamEntry;
}

export function flatten(files: RegisterFile[]): Param[] {
  const out: Param[] = [];
  for (const f of files) {
    for (const [key, entry] of Object.entries(f.entries)) {
      out.push({ path: `${f.group}.${key}`, group: f.group, section: key.split(".")[0], entry });
    }
  }
  return out;
}

/** Value shown for a parameter: the latest live change, else the scenario override, else the register value. */
export function currentValue(p: Param, overrides: Record<string, ParamValue> | undefined, changes: { path: string; after: unknown }[]): ParamValue | null {
  for (let i = changes.length - 1; i >= 0; i--) if (changes[i].path === p.path) return changes[i].after as ParamValue;
  if (overrides && p.path in overrides) return overrides[p.path];
  return p.entry.value ?? null;
}

export function describe(v: ParamValue | null): string {
  if (v === null || v === undefined) return "unset";
  if (typeof v === "number") return String(v);
  const { dist, ...rest } = v as Record<string, unknown>;
  return `${dist}(${Object.entries(rest)
    .filter(([, x]) => typeof x === "number")
    .map(([k, x]) => `${k}=${x}`)
    .join(", ")})`;
}

/** A distribution's location field that a single slider moves (keeps the other fields). */
export function locationField(v: ParamValue | null): string | null {
  if (v === null || typeof v === "number") return null;
  for (const f of ["mean", "median", "mode", "value", "mu"]) if (f in (v as object)) return f;
  return null;
}

/** Why an entry cannot be edited live. */
export function readOnlyReason(e: ParamEntry): string {
  const role =
    e.role === "fix"
      ? "measured (role fix): never changed"
      : e.role === "fit"
        ? `calibrated in stage ${e.stage}: change it in a scenario or an experiment, not mid-run`
        : e.role === "sweep"
          ? "an experimental factor: sweep it in an experiment"
          : "set per scenario";
  return `Read-only during a run (live: false). ${role[0].toUpperCase()}${role.slice(1)}.`;
}

/** Whether a value is a per-agent draw (a distribution) rather than a population-level number. */
export function isDrawn(v: ParamValue | null): boolean {
  return v !== null && typeof v === "object";
}
