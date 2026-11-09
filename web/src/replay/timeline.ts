/**
 * Playback of a run's trajectories: a time cursor (shared between the two runs of a compare),
 * a cache of one-minute windows loaded from DuckDB, and the display for the map (positions
 * interpolated between the 1 Hz samples).
 */
import type { AgentFrame } from "../protocol";
import type { Display } from "../render/interpolate";
import { Store } from "../store";
import type { RunData } from "./runData";

/** Simulated seconds per loaded window. */
export const WINDOW_S = 60;

export class Clock {
  readonly t = new Store(0);
  readonly playing = new Store(false);
  /** Simulated seconds per wall second. */
  rate = 10;
  range: [number, number] = [0, 0];
  private raf = 0;
  private last = 0;

  constructor() {
    const loop = (now: number) => {
      this.raf = requestAnimationFrame(loop);
      if (this.playing.get()) {
        const dt = this.last ? (now - this.last) / 1000 : 0;
        let t = this.t.get() + this.rate * dt;
        if (t >= this.range[1]) {
          t = this.range[1];
          this.playing.set(false);
        }
        this.t.set(t);
      }
      this.last = now;
    };
    this.raf = requestAnimationFrame(loop);
  }
  seek(t: number): void {
    this.t.set(Math.min(this.range[1], Math.max(this.range[0], t)));
  }
  dispose(): void {
    cancelAnimationFrame(this.raf);
  }
}

export class Timeline {
  private cache = new Map<number, AgentFrame[] | "loading">();
  private filterKey = "";
  error: string | null = null;
  readonly loadedVersion = new Store(0);

  constructor(
    readonly run: RunData,
    readonly clock: Clock,
  ) {}

  setFilter(filter: string): void {
    if (filter === this.filterKey) return;
    this.filterKey = filter;
    this.cache.clear();
    this.error = null;
    this.loadedVersion.set((v) => v + 1);
  }

  private windowStart(t: number): number {
    return Math.floor(t / WINDOW_S) * WINDOW_S;
  }

  private ensure(start: number): AgentFrame[] | null {
    const c = this.cache.get(start);
    if (c && c !== "loading") return c;
    if (!c && start <= this.run.tRange[1]) {
      this.cache.set(start, "loading");
      const key = this.filterKey;
      // One sample of overlap, so interpolation works across the window boundary.
      this.run
        .frames(start, start + WINDOW_S + this.run.trajEvery, key)
        .then((frames) => {
          if (key !== this.filterKey) return;
          this.cache.set(start, frames);
          // Keep the cache small: windows near the cursor only.
          for (const k of [...this.cache.keys()]) if (Math.abs(k - start) > 3 * WINDOW_S) this.cache.delete(k);
          this.loadedVersion.set((v) => v + 1);
        })
        .catch((e) => {
          this.error = String(e);
          this.cache.delete(start);
          this.loadedVersion.set((v) => v + 1);
        });
    }
    return null;
  }

  /** Frames bracketing time `t`, loading and prefetching windows as needed. */
  display(t = this.clock.t.get()): Display {
    const start = this.windowStart(t);
    const frames = this.ensure(start);
    if (t - start > WINDOW_S * 0.7) this.ensure(start + WINDOW_S);
    if (!frames || frames.length === 0) return { alpha: 1 };
    let k = 0;
    while (k + 1 < frames.length && frames[k + 1].t <= t) k++;
    const prev = frames[k];
    const cur = frames[k + 1];
    if (!cur || prev.t > t) return { cur: prev, alpha: 1 };
    return { prev, cur, alpha: (t - prev.t) / (cur.t - prev.t) };
  }

  /** The frame at or before `t` (for speed histograms and the agent list). */
  frameAt(t = this.clock.t.get()): AgentFrame | undefined {
    const d = this.display(t);
    return d.prev && d.alpha < 1 ? d.prev : d.cur;
  }
}
