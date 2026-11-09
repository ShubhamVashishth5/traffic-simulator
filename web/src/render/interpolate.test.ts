import { describe, expect, it } from "vitest";
import type { AgentFrame } from "../protocol";
import { interpolate, liveAlpha } from "./interpolate";

function frame(n: number, t: number, dx: number, skip = -1): AgentFrame {
  const ids: bigint[] = [];
  for (let i = 0; i < n; i++) if (i !== skip) ids.push(BigInt(i) + (3n << 48n));
  const m = ids.length;
  const f: AgentFrame = {
    t,
    step: t * 10,
    total: m,
    colour: "speed",
    n: m,
    id: BigUint64Array.from(ids),
    x: new Float32Array(m),
    y: new Float32Array(m),
    heading: new Float32Array(m),
    length: new Float32Array(m).fill(4),
    width: new Float32Array(m).fill(1.8),
    cls: new Uint8Array(m),
    value: new Float32Array(m),
  };
  for (let k = 0; k < m; k++) {
    f.x[k] = Number(ids[k] - (3n << 48n)) * 10 + dx;
    f.y[k] = 1;
  }
  return f;
}

const grey = () => [1, 2, 3, 255] as [number, number, number, number];

describe("interpolate", () => {
  it("moves matching agents between frames and keeps new ones where they are", () => {
    const prev = frame(3, 1, 0, 1);
    const cur = frame(3, 1.1, 2);
    const b = interpolate({ prev, cur, alpha: 0.25 }, undefined, "k", grey);
    expect(b.n).toBe(3);
    expect(b.pos[0]).toBeCloseTo(0.5, 6); // 0 → 2
    expect(b.pos[2]).toBeCloseTo(12, 6); // not in prev: shown at its current position
    expect(b.pos[4]).toBeCloseTo(20.5, 6);
    expect(Array.from(b.colour.subarray(0, 4))).toEqual([1, 2, 3, 255]);
    // Nothing changed: the same buffers (deck.gl uploads nothing).
    expect(interpolate({ prev, cur, alpha: 0.25 }, b, "k", grey)).toBe(b);
    // Changed: the other buffer set, so the arrays are new objects.
    const c = interpolate({ prev, cur, alpha: 0.5 }, b, "k", grey);
    expect(c.pos.buffer).not.toBe(b.pos.buffer);
  });

  it("does not interpolate across jumps", () => {
    expect(liveAlpha(0, 100, 150, frame(1, 0, 0), frame(1, 0.1, 0))).toBeCloseTo(0.5, 9);
    expect(liveAlpha(0, 100, 150, frame(1, 0, 0), frame(1, 30, 0))).toBe(1);
  });

  it("is fast enough for 20,000 agents at 60 fps", () => {
    const prev = frame(20000, 1, 0);
    const cur = frame(20000, 1.1, 1);
    let b = interpolate({ prev, cur, alpha: 0.1 }, undefined, "k", grey);
    const t0 = performance.now();
    const n = 30;
    for (let k = 0; k < n; k++) b = interpolate({ prev, cur, alpha: (k + 2) / (n + 2) }, b, "k", grey);
    const ms = (performance.now() - t0) / n;
    console.log(`interpolate 20,000 agents: ${ms.toFixed(2)} ms per frame`);
    expect(ms).toBeLessThan(16);
  });
});
