/**
 * One run folder opened in the browser (plan P4.7): its Parquet tables registered with
 * DuckDB-WASM (read over HTTP range requests, or from local files), its spec parsed for the
 * geometry, and queries for frames, dashboards and heatmaps. Trajectory windows are loaded a
 * minute at a time, so scrubbing a 1 h run never holds the whole table.
 */
import { parse as parseYaml } from "yaml";
import type { DetRow } from "../analytics";
import { type Geometry, geometry } from "../geometry";
import type { AgentFrame } from "../protocol";
import type { Scenario } from "../schema";
import { duck, duckdb } from "./duck";

export type RunSource = { kind: "url"; base: string; label: string } | { kind: "files"; files: File[]; label: string };

/** Tables a run folder may hold. */
const TABLES = ["agents", "detectors", "events", "kpis", "trajectories", "commands", "params"] as const;

type Row = Record<string, unknown>;

interface ArrowLike {
  numRows: number;
  toArray(): { toJSON(): Row }[];
  getChild(name: string): { toArray(): ArrayLike<number> & { [i: number]: unknown } } | null;
}

let counter = 0;

function num(x: unknown): number {
  return typeof x === "bigint" ? Number(x) : (x as number);
}

/** Quote a value as a SQL string literal. */
export function sqlString(s: string): string {
  return `'${s.replace(/'/g, "''")}'`;
}

export class RunData {
  readonly alias: string;
  manifest: Row = {};
  scenario!: Scenario;
  geometry!: Geometry;
  /** Class keys present, in a fixed order (index = `cls` in frames). */
  classes: string[] = [];
  kpis = new Map<string, number | null>();
  detectors: DetRow[] = [];
  tables = new Set<string>();
  tRange: [number, number] = [0, 0];
  trajEvery = 1;
  agentsCount = 0;
  private conn: duckdb.AsyncDuckDBConnection | null = null;

  private constructor(readonly source: RunSource) {
    this.alias = `r${++counter}`;
  }

  get label(): string {
    return this.source.label;
  }

  table(name: string): string {
    return `${this.alias}_${name}`;
  }

  static async open(source: RunSource, classOrder: (a: string, b: string) => number): Promise<RunData> {
    const r = new RunData(source);
    const db = await duck();
    r.conn = await db.connect();
    const text = async (name: string): Promise<string | null> => {
      if (source.kind === "url") {
        const res = await fetch(`${source.base}/${name}`);
        return res.ok ? res.text() : null;
      }
      const f = source.files.find((x) => x.name === name);
      return f ? f.text() : null;
    };
    const manifest = await text("manifest.json");
    if (manifest === null) throw new Error(`${source.label}: no manifest.json (not a run folder)`);
    r.manifest = JSON.parse(manifest);
    const specText = await text("spec.resolved.yaml");
    const scenarioText = specText === null ? await text("scenario.yaml") : null;
    if (specText === null && scenarioText === null) throw new Error(`${source.label}: no spec.resolved.yaml`);
    const spec = specText !== null ? parseYaml(specText) : { scenario: parseYaml(scenarioText!), params: {} };
    r.scenario = spec.scenario as Scenario;
    const band = spec.params?.["vehicles.median_band.width"]?.si;
    r.geometry = geometry(r.scenario, typeof band === "number" ? band : 0);
    for (const t of TABLES) {
      const file = `${t}.parquet`;
      const name = `${r.alias}_${file}`;
      if (source.kind === "url") {
        const url = new URL(`${source.base}/${file}`, window.location.href).toString();
        const head = await fetch(url, { method: "HEAD" });
        if (!head.ok) continue;
        await db.registerFileURL(name, url, duckdb.DuckDBDataProtocol.HTTP, false);
      } else {
        const f = source.files.find((x) => x.name === file);
        if (!f) continue;
        await db.registerFileHandle(name, f, duckdb.DuckDBDataProtocol.BROWSER_FILEREADER, true);
      }
      await r.conn.query(`CREATE OR REPLACE VIEW ${r.table(t)} AS SELECT * FROM read_parquet(${sqlString(name)})`);
      r.tables.add(t);
    }
    if (!r.tables.has("agents")) throw new Error(`${source.label}: no agents.parquet`);
    const classes = await r.rows(`SELECT DISTINCT class FROM ${r.table("agents")}`);
    r.classes = classes.map((x) => String(x.class)).sort(classOrder);
    r.agentsCount = num((await r.rows(`SELECT count(*) AS n FROM ${r.table("agents")}`))[0].n);
    if (r.tables.has("kpis")) {
      const k = (await r.rows(`SELECT * FROM ${r.table("kpis")}`))[0] ?? {};
      for (const [key, v] of Object.entries(k)) if (typeof v === "number" || v === null) r.kpis.set(key, v as number | null);
    }
    if (r.tables.has("detectors")) {
      r.detectors = (await r.rows(`SELECT * FROM ${r.table("detectors")}`)).map((x) => ({
        ...(x as unknown as DetRow),
        count: num(x.count),
        lateral_hist: x.lateral_hist ? Array.from(x.lateral_hist as ArrayLike<number>, num) : null,
      }));
      const area = r.detectors.filter((d) => d.kind === "area" && !d.in_warmup);
      if (area.length) {
        const first = area[0].detector_id;
        const rows = area.filter((d) => d.detector_id === first);
        const windows = new Set(rows.map((d) => d.t_start)).size;
        r.kpis.set("density", (1000 * rows.reduce((a, d) => a + (d.density ?? 0), 0)) / Math.max(1, windows));
      }
    }
    if (r.tables.has("trajectories")) {
      const q = (await r.rows(`SELECT min(t) AS t0, max(t) AS t1 FROM ${r.table("trajectories")}`))[0];
      r.tRange = [num(q.t0), num(q.t1)];
      const two = await r.rows(`SELECT DISTINCT t FROM ${r.table("trajectories")} WHERE t <= ${r.tRange[0] + 30} ORDER BY t LIMIT 2`);
      if (two.length === 2) r.trajEvery = num(two[1].t) - num(two[0].t);
    }
    return r;
  }

  async query(sql: string): Promise<ArrowLike> {
    return (await this.conn!.query(sql)) as unknown as ArrowLike;
  }

  async rows(sql: string): Promise<Row[]> {
    const t = await this.query(sql);
    return t.toArray().map((r) => r.toJSON());
  }

  /** Agent frames at every trajectory sample in `[t0, t1]`, filtered by a SQL predicate on the trajectory and agent columns. */
  async frames(t0: number, t1: number, filter: string): Promise<AgentFrame[]> {
    const where = filter.trim() ? `AND (${filter})` : "";
    const ox = this.geometry.origin[0];
    const oy = this.geometry.origin[1];
    const t = await this.query(
      `SELECT tr.t AS t, tr.agent_id AS id, (tr.x_world - ${ox}) AS x, (tr.y_world - ${oy}) AS y, tr.heading AS heading,
              a.length AS length, a.width AS width, a.class AS cls, tr.v AS v
       FROM ${this.table("trajectories")} tr JOIN ${this.table("agents")} a USING (agent_id)
       WHERE tr.t BETWEEN ${t0} AND ${t1} ${where}
       ORDER BY tr.t, tr.agent_id`,
    );
    const col = (n: string) => t.getChild(n)!.toArray();
    const ts = col("t");
    const ids = col("id") as unknown as BigUint64Array;
    const xs = col("x");
    const ys = col("y");
    const hs = col("heading");
    const ls = col("length");
    const ws = col("width");
    const cs = col("cls") as unknown as string[];
    const vs = col("v");
    const clsIndex = new Map(this.classes.map((c, i) => [c, i]));
    const out: AgentFrame[] = [];
    let i = 0;
    const n = t.numRows;
    while (i < n) {
      let j = i;
      while (j < n && ts[j] === ts[i]) j++;
      const m = j - i;
      const f: AgentFrame = {
        t: Number(ts[i]),
        step: 0,
        total: m,
        colour: "speed",
        n: m,
        id: new BigUint64Array(m),
        x: new Float32Array(m),
        y: new Float32Array(m),
        heading: new Float32Array(m),
        length: new Float32Array(m),
        width: new Float32Array(m),
        cls: new Uint8Array(m),
        value: new Float32Array(m),
      };
      for (let k = 0; k < m; k++) {
        const r = i + k;
        f.id[k] = BigInt(ids[r] as unknown as bigint);
        f.x[k] = Number(xs[r]);
        f.y[k] = Number(ys[r]);
        f.heading[k] = Number(hs[r]);
        f.length[k] = Number(ls[r]);
        f.width[k] = Number(ws[r]);
        f.cls[k] = clsIndex.get(String(cs[r])) ?? 0;
        f.value[k] = Number(vs[r]);
      }
      out.push(f);
      i = j;
    }
    return out;
  }

  /** Time–space samples of one ribbon (at most about `maxPoints`). */
  async timeSpace(ribbon: string, filter: string, maxPoints = 200_000): Promise<{ t: Float64Array; s: Float64Array; v: Float64Array }> {
    const where = filter.trim() ? `AND (${filter})` : "";
    const base = `FROM ${this.table("trajectories")} tr JOIN ${this.table("agents")} a USING (agent_id) WHERE tr.ribbon = ${sqlString(ribbon)} ${where}`;
    const n = num((await this.rows(`SELECT count(*) AS n ${base}`))[0].n);
    const stride = Math.max(1, Math.ceil(n / maxPoints));
    const every = this.trajEvery * stride;
    const sampled = stride > 1 ? `AND abs(tr.t / ${every} - round(tr.t / ${every})) < 1e-6` : "";
    const t = await this.query(`SELECT tr.t AS t, tr.s AS s, tr.v AS v ${base} ${sampled}`);
    return { t: Float64Array.from(t.getChild("t")!.toArray(), Number), s: Float64Array.from(t.getChild("s")!.toArray(), Number), v: Float64Array.from(t.getChild("v")!.toArray(), Number) };
  }

  /** Mean speed by time bin × station bin, and occupancy by station × lateral bin, on one ribbon. */
  async heatmaps(ribbon: string, filter: string, dt: number, ds: number, dy: number) {
    const where = filter.trim() ? `AND (${filter})` : "";
    const base = `FROM ${this.table("trajectories")} tr JOIN ${this.table("agents")} a USING (agent_id) WHERE tr.ribbon = ${sqlString(ribbon)} ${where}`;
    const speed = await this.rows(`SELECT floor(tr.t / ${dt}) AS i, floor(tr.s / ${ds}) AS j, avg(tr.v) AS v, count(*) AS n ${base} GROUP BY 1, 2`);
    const lateral = await this.rows(`SELECT floor(tr.s / ${ds}) AS i, floor(tr.y / ${dy}) AS j, count(*) AS n ${base} GROUP BY 1, 2`);
    return {
      speed: speed.map((r) => [num(r.i) * dt, num(r.j) * ds, num(r.v), num(r.n)]),
      lateral: lateral.map((r) => [num(r.i) * ds, num(r.j) * dy, num(r.n)]),
    };
  }

  async close(): Promise<void> {
    await this.conn?.close();
    this.conn = null;
  }
}

/** Files of one run folder from a directory picker (the shallowest folder with a manifest). */
export function runFiles(files: FileList | File[]): { files: File[]; label: string } {
  const all = Array.from(files);
  const withPath = all.map((f) => ({ f, path: (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name }));
  const manifests = withPath.filter((x) => x.path.endsWith("manifest.json")).sort((a, b) => a.path.split("/").length - b.path.split("/").length);
  if (!manifests.length) return { files: all, label: "local files" };
  const dir = manifests[0].path.slice(0, -"manifest.json".length);
  return { files: withPath.filter((x) => x.path.startsWith(dir) && !x.path.slice(dir.length).includes("/")).map((x) => x.f), label: dir.replace(/\/$/, "") || "local files" };
}
