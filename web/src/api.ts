/** HTTP endpoints of `marg serve` (same origin; proxied by `npm run dev`). */
import type { RegisterFile } from "./register";

export interface ScenarioListing {
  file: string;
  name: string;
  description: string;
  duration_s: number;
}

export interface SessionCreated {
  id: string;
  ws: string;
  scenario: string;
  initial_run_hash: string;
}

export interface RunListing {
  path: string;
  scenario: string;
  seed: number;
  run_hash: string;
  status: string;
  output_hash: string;
  experiment: { experiment: string; run_id: number; point: number; arm: string; rep: number } | null;
  live: boolean;
  tables: string[];
}

export interface Listing {
  root: string;
  runs: RunListing[];
  experiments: { path: string; name: string; runs: number; arms: string[] }[];
}

async function json<T>(r: Response): Promise<T> {
  if (!r.ok) {
    let msg = `${r.status} ${r.statusText}`;
    try {
      msg = (await r.json()).error ?? msg;
    } catch {
      /* not JSON */
    }
    throw new Error(msg);
  }
  return (await r.json()) as T;
}

export const api = {
  scenarios: () => fetch("/scenarios").then((r) => json<ScenarioListing[]>(r)),
  register: () => fetch("/register").then((r) => json<{ files: RegisterFile[] }>(r)),
  runs: () => fetch("/runs").then((r) => json<Listing>(r)),
  createSession: (body: { scenario?: string; yaml?: string; seed: number; live_window_s?: number; threads?: number }) =>
    fetch("/sessions", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) }).then((r) =>
      json<SessionCreated>(r),
    ),
  deleteSession: (id: string) => fetch(`/sessions/${encodeURIComponent(id)}`, { method: "DELETE" }),
  /** URL of a file under the server's runs directory. */
  file: (runPath: string, name: string) => `/files/${runPath.split("/").map(encodeURIComponent).join("/")}/${name}`,
};
