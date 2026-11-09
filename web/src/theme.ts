/**
 * Viewer colours (the house style of plan P3.9, `python/marg/style.py`, extended to dark mode).
 *
 * - Chrome and charts follow the OS light/dark setting, each mode with its own validated steps.
 * - The map draws vehicles on an asphalt-dark carriageway in both modes, so vehicle colours use
 *   the dark-surface steps, validated against `ROAD` (scripts/validate_palette.js: all checks pass).
 * - Categorical hues are assigned in a fixed order and never cycled. Ten vehicle classes fold into
 *   seven groups; vehicle footprint (length × width) is the secondary encoding on the map.
 */

export type Rgba = [number, number, number, number];

export const SERIES_LIGHT = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];
export const SERIES_DARK = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"];

/** Carriageway fill: the chart surface the vehicle marks are validated against. */
export const ROAD = "#1a1a19";

export interface Chrome {
  dark: boolean;
  surface: string;
  page: string;
  ink: string;
  ink2: string;
  muted: string;
  grid: string;
  axis: string;
  border: string;
  series: string[];
  /** Sequential blue, low → high, for charts on this surface. */
  sequential: string[];
  /** Diverging neutral midpoint for charts. */
  neutral: string;
}

const BLUE = ["#cde2fb", "#b7d3f6", "#9ec5f4", "#86b6ef", "#6da7ec", "#5598e7", "#3987e5", "#2a78d6", "#256abf", "#1c5cab", "#184f95", "#104281", "#0d366b"];

export const LIGHT: Chrome = {
  dark: false,
  surface: "#fcfcfb",
  page: "#f9f9f7",
  ink: "#0b0b0b",
  ink2: "#52514e",
  muted: "#898781",
  grid: "#e1e0d9",
  axis: "#c3c2b7",
  border: "rgba(11,11,11,0.10)",
  series: SERIES_LIGHT,
  sequential: BLUE,
  neutral: "#f0efec",
};

export const DARK: Chrome = {
  dark: true,
  surface: "#1a1a19",
  page: "#0d0d0d",
  ink: "#f0efec",
  ink2: "#c3c2b7",
  muted: "#898781",
  grid: "#2c2c2a",
  axis: "#383835",
  border: "rgba(255,255,255,0.10)",
  series: SERIES_DARK,
  sequential: [...BLUE].slice(0, 11).reverse(),
  neutral: "#383835",
};

export function chrome(): Chrome {
  const dark = typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches;
  return dark ? DARK : LIGHT;
}

/** Class groups in colour order; every class key belongs to exactly one. */
export const CLASS_GROUPS: { label: string; classes: string[] }[] = [
  { label: "Two-wheeler", classes: ["two_wheeler"] },
  { label: "Car", classes: ["car"] },
  { label: "Auto-rickshaw", classes: ["auto_rickshaw"] },
  { label: "E-rickshaw", classes: ["e_rickshaw"] },
  { label: "Bus", classes: ["bus"] },
  { label: "Non-motorised", classes: ["bicycle", "cycle_rickshaw"] },
  { label: "Goods", classes: ["lcv", "truck", "tractor"] },
];

export function groupOfClass(cls: string): number {
  const g = CLASS_GROUPS.findIndex((x) => x.classes.includes(cls));
  return g < 0 ? CLASS_GROUPS.length - 1 : g;
}

export function hexToRgba(hex: string, alpha = 255): Rgba {
  const h = hex.replace("#", "");
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16), alpha];
}

function mix(a: Rgba, b: Rgba, f: number): Rgba {
  return [0, 1, 2, 3].map((k) => Math.round(a[k] + (b[k] - a[k]) * f)) as Rgba;
}

/** Vehicle colours on the road surface, by class index (`Hello.classes`). */
export function classColours(classes: string[]): Rgba[] {
  return classes.map((c) => hexToRgba(SERIES_DARK[groupOfClass(c)]));
}

/** Sequential ramp for marks on the road: dark blue (low) → pale blue (high), ordinal floor ≥ 2:1. */
const ROAD_SEQUENTIAL = ["#184f95", "#1c5cab", "#256abf", "#2a78d6", "#3987e5", "#5598e7", "#6da7ec", "#86b6ef", "#9ec5f4", "#b7d3f6", "#cde2fb"].map((h) =>
  hexToRgba(h),
);
/** Diverging on the road: red (negative) ↔ muted grey ↔ blue (positive). */
const ROAD_NEG = hexToRgba("#e66767");
const ROAD_MID = hexToRgba("#898781");
const ROAD_POS = hexToRgba("#5598e7");

export function sequentialOnRoad(f: number): Rgba {
  if (!Number.isFinite(f)) return hexToRgba("#898781");
  const x = Math.min(1, Math.max(0, f)) * (ROAD_SEQUENTIAL.length - 1);
  const i = Math.min(ROAD_SEQUENTIAL.length - 2, Math.floor(x));
  return mix(ROAD_SEQUENTIAL[i], ROAD_SEQUENTIAL[i + 1], x - i);
}

export function divergingOnRoad(f: number): Rgba {
  if (!Number.isFinite(f)) return ROAD_MID;
  const x = Math.min(1, Math.max(-1, f));
  return x < 0 ? mix(ROAD_MID, ROAD_NEG, -x) : mix(ROAD_MID, ROAD_POS, x);
}

/** CSS custom properties for the chrome. */
export function applyChrome(c: Chrome): void {
  const r = document.documentElement.style;
  r.setProperty("--surface", c.surface);
  r.setProperty("--page", c.page);
  r.setProperty("--ink", c.ink);
  r.setProperty("--ink2", c.ink2);
  r.setProperty("--muted", c.muted);
  r.setProperty("--grid", c.grid);
  r.setProperty("--axis", c.axis);
  r.setProperty("--border", c.border);
  r.setProperty("--accent", c.series[0]);
  r.setProperty("--road", ROAD);
  document.documentElement.style.colorScheme = c.dark ? "dark" : "light";
}
