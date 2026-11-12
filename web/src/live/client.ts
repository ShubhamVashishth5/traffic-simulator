/**
 * A live session over WebSocket: sends commands, keeps the latest frame, dashboards, inspector
 * and change history in stores the views subscribe to.
 */
import type { AgentFrame } from "../protocol";
import { PROTOCOL_VERSION, decodeFrame } from "../protocol";
import type { ChangeView, ClientMessage, DetectorRow, Hello, Inspector, Kpi, ObjectView, ServerMessage, Status } from "../protocol.gen";
import { Store } from "../store";

export interface Ack {
  seq: number | null;
  cmd: string;
  ok: boolean;
  message: string;
  at: number;
}

export interface LiveState {
  connection: "connecting" | "open" | "closed" | "error";
  error?: string;
  hello?: Hello;
  status?: Status;
  kpi?: Kpi;
  /** Every live detector window so far. */
  windows: DetectorRow[];
  /** Time–space samples so far for `tsRibbon`. */
  ts: { ribbon: string; t: number[]; s: number[]; v: number[] };
  inspector?: Inspector;
  history: ChangeView[];
  /** Every road object, on the road now or not. */
  objects: ObjectView[];
  acks: Ack[];
  breakpoint?: { reason: string; t: number };
  saved?: Extract<ServerMessage, { type: "saved" }>["data"];
  /** Received frames per second and MB/s, measured over the last second. */
  rate: { fps: number; mbps: number };
}

/** Cap on time–space samples kept in the browser. */
const TS_KEEP = 300_000;

/** Keep every other sample time (the diagram keeps its whole span at half the resolution). */
export function thinTimes<T extends { t: number[]; s: number[]; v: number[] }>(ts: T): T {
  const keep: boolean[] = [];
  let k = 0;
  let last = NaN;
  for (const t of ts.t) {
    if (t !== last) {
      k++;
      last = t;
    }
    keep.push(k % 2 === 1);
  }
  return { ...ts, t: ts.t.filter((_, i) => keep[i]), s: ts.s.filter((_, i) => keep[i]), v: ts.v.filter((_, i) => keep[i]) };
}

export class LiveClient {
  readonly state = new Store<LiveState>({
    connection: "connecting",
    windows: [],
    ts: { ribbon: "", t: [], s: [], v: [] },
    history: [],
    objects: [],
    acks: [],
    rate: { fps: 0, mbps: 0 },
  });
  /** Latest frame and the one before it (for interpolation), with arrival times. */
  readonly frames = new Store<{ prev?: AgentFrame; cur?: AgentFrame; prevAt: number; curAt: number }>({ prevAt: 0, curAt: 0 });
  private ws: WebSocket;
  private counter = { n: 0, bytes: 0, since: performance.now() };

  constructor(
    readonly sessionId: string,
    wsPath: string,
  ) {
    const url = new URL(wsPath.includes("protocol=") ? wsPath : `${wsPath}?protocol=${PROTOCOL_VERSION}`, window.location.href);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    this.ws = new WebSocket(url);
    this.ws.binaryType = "arraybuffer";
    this.ws.onopen = () => this.state.set((s) => ({ ...s, connection: "open" }));
    this.ws.onclose = () => this.state.set((s) => ({ ...s, connection: s.connection === "error" ? "error" : "closed" }));
    this.ws.onerror = () => this.state.set((s) => ({ ...s, connection: "error", error: "WebSocket error (is `marg serve` running?)" }));
    this.ws.onmessage = (e) => (typeof e.data === "string" ? this.onText(e.data) : this.onFrame(e.data as ArrayBuffer));
  }

  send(m: ClientMessage): void {
    if (this.ws.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(m));
  }

  close(): void {
    this.ws.close();
  }

  private onFrame(buf: ArrayBuffer): void {
    const now = performance.now();
    const c = this.counter;
    c.n++;
    c.bytes += buf.byteLength;
    if (now - c.since >= 1000) {
      const dt = (now - c.since) / 1000;
      const rate = { fps: c.n / dt, mbps: c.bytes / dt / 1e6 };
      this.counter = { n: 0, bytes: 0, since: now };
      this.state.set((s) => ({ ...s, rate }));
    }
    try {
      const f = decodeFrame(buf);
      this.frames.set((x) => ({ prev: x.cur, prevAt: x.curAt, cur: f, curAt: now }));
    } catch (err) {
      this.state.set((s) => ({ ...s, error: String(err) }));
    }
  }

  private onText(text: string): void {
    const m = JSON.parse(text) as ServerMessage;
    switch (m.type) {
      case "hello": {
        const hello = m.data;
        if (hello.protocol !== PROTOCOL_VERSION) {
          this.state.set((s) => ({ ...s, connection: "error", error: `server speaks protocol ${hello.protocol}, viewer ${PROTOCOL_VERSION}` }));
          this.ws.close();
          return;
        }
        this.state.set((s) => ({ ...s, hello }));
        break;
      }
      case "status":
        this.state.set((s) => ({ ...s, status: m.data }));
        break;
      case "kpi": {
        const k = m.data;
        this.state.set((s) => {
          let ts = s.ts;
          if (k.ts && k.ts.t.length) {
            ts = k.ts.ribbon === s.ts.ribbon ? { ribbon: k.ts.ribbon, t: s.ts.t.concat(k.ts.t), s: s.ts.s.concat(k.ts.s), v: s.ts.v.concat(k.ts.v) } : { ribbon: k.ts.ribbon, t: k.ts.t, s: k.ts.s, v: k.ts.v };
            while (ts.t.length > TS_KEEP) ts = thinTimes(ts);
          }
          return { ...s, kpi: k, windows: k.windows.length ? s.windows.concat(k.windows) : s.windows, ts };
        });
        break;
      }
      case "inspector":
        this.state.set((s) => ({ ...s, inspector: m.data }));
        break;
      case "history":
        this.state.set((s) => ({ ...s, history: m.data.changes }));
        break;
      case "objects":
        this.state.set((s) => ({ ...s, objects: m.data.objects }));
        break;
      case "ack": {
        const a = m.data;
        this.state.set((s) => ({ ...s, acks: [{ seq: a.seq ?? null, cmd: a.cmd, ok: a.ok, message: a.message, at: Date.now() }, ...s.acks].slice(0, 40) }));
        break;
      }
      case "breakpoint":
        this.state.set((s) => ({ ...s, breakpoint: { reason: m.data.reason, t: m.data.t } }));
        break;
      case "saved":
        this.state.set((s) => ({ ...s, saved: m.data }));
        break;
      case "error":
        this.state.set((s) => ({ ...s, error: m.data.message }));
        break;
    }
  }

  /** Forget time–space samples (a new ribbon was chosen). */
  resetTimeSpace(ribbon: string): void {
    this.state.set((s) => ({ ...s, ts: { ribbon, t: [], s: [], v: [] } }));
  }
}
