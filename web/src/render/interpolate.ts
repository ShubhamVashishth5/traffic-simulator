/**
 * Draw buffers for the vehicle layer: positions interpolated between the last two frames (the
 * simulation produces 10 states a second at dt = 0.1 s; the screen draws 60), colours from the
 * current frame. Buffers alternate between two sets, so deck.gl sees new arrays only when the
 * picture changes and uploads nothing while paused.
 */
import type { AgentFrame } from "../protocol";
import type { Rgba } from "../theme";

export interface Display {
  prev?: AgentFrame;
  cur?: AgentFrame;
  /** 0 = show `prev`, 1 = show `cur`. */
  alpha: number;
}

interface Arrays {
  id: BigUint64Array;
  pos: Float32Array;
  size: Float32Array;
  angle: Float32Array;
  colour: Uint8Array;
  cls: Uint8Array;
  value: Float32Array;
}

export interface DrawBuffers extends Arrays {
  n: number;
  key: { cur?: AgentFrame; prev?: AgentFrame; alpha: number; colourKey: string };
  sets: [Arrays, Arrays];
  which: number;
  prevIndex?: { frame: AgentFrame; map: Map<bigint, number> };
}

function alloc(n: number): Arrays {
  return {
    id: new BigUint64Array(n),
    pos: new Float32Array(2 * n),
    size: new Float32Array(2 * n),
    angle: new Float32Array(n),
    colour: new Uint8Array(4 * n),
    cls: new Uint8Array(n),
    value: new Float32Array(n),
  };
}

/** Shortest-way angle interpolation. */
function lerpAngle(a: number, b: number, f: number): number {
  let d = b - a;
  if (d > Math.PI) d -= 2 * Math.PI;
  if (d < -Math.PI) d += 2 * Math.PI;
  return a + d * f;
}

export function interpolate(d: Display, reuse: DrawBuffers | undefined, colourKey: string, colour: (cls: number, value: number) => Rgba): DrawBuffers {
  const alpha = Math.min(1, Math.max(0, d.alpha));
  if (reuse && reuse.key.cur === d.cur && reuse.key.prev === d.prev && reuse.key.alpha === alpha && reuse.key.colourKey === colourKey) return reuse;
  const cur = d.cur;
  const n = cur?.n ?? 0;
  let sets = reuse?.sets;
  let which = reuse ? 1 - reuse.which : 0;
  if (!sets || sets[which].angle.length < n) {
    const cap = Math.max(1024, Math.ceil(n * 1.25));
    sets = [alloc(cap), alloc(cap)];
    which = 0;
  }
  const a = sets[which];
  let prevIndex = reuse?.prevIndex;
  const prev = alpha < 1 ? d.prev : undefined;
  if (prev && prevIndex?.frame !== prev) {
    const map = new Map<bigint, number>();
    for (let j = 0; j < prev.n; j++) map.set(prev.id[j], j);
    prevIndex = { frame: prev, map };
  }
  for (let i = 0; i < n; i++) {
    const c = cur!;
    let x = c.x[i];
    let y = c.y[i];
    let h = c.heading[i];
    const j = prev ? prevIndex!.map.get(c.id[i]) : undefined;
    if (prev && j !== undefined) {
      x = prev.x[j] + (x - prev.x[j]) * alpha;
      y = prev.y[j] + (y - prev.y[j]) * alpha;
      h = lerpAngle(prev.heading[j], h, alpha);
    }
    a.id[i] = c.id[i];
    a.pos[2 * i] = x;
    a.pos[2 * i + 1] = y;
    a.size[2 * i] = c.length[i];
    a.size[2 * i + 1] = c.width[i];
    a.angle[i] = h;
    a.cls[i] = c.cls[i];
    a.value[i] = c.value[i];
    a.colour.set(colour(c.cls[i], c.value[i]), 4 * i);
  }
  // Views of the first n rows, so the layer gets exactly n instances.
  const view: Arrays = {
    id: a.id.subarray(0, n),
    pos: a.pos.subarray(0, 2 * n),
    size: a.size.subarray(0, 2 * n),
    angle: a.angle.subarray(0, n),
    colour: a.colour.subarray(0, 4 * n),
    cls: a.cls.subarray(0, n),
    value: a.value.subarray(0, n),
  };
  return { ...view, n, key: { cur: d.cur, prev: d.prev, alpha, colourKey }, sets, which, prevIndex };
}

/** Interpolation factor for a live stream: time since the current frame over the frame interval. */
export function liveAlpha(prevAt: number, curAt: number, now: number, prev?: AgentFrame, cur?: AgentFrame): number {
  if (!prev || !cur) return 1;
  const interval = curAt - prevAt;
  // Jumps (a big step, a seek) and slow streams are shown as they are.
  if (interval <= 0 || interval > 500 || cur.t - prev.t > 2 || cur.t <= prev.t) return 1;
  return Math.min(1, (now - curAt) / interval);
}
