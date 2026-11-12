/**
 * The map (plan P4.3): deck.gl with an orthographic, metric view; ribbons, the median-side band
 * and detectors underneath; vehicles as oriented rectangles; optional trails; the selected
 * agent ringed. Drawn every animation frame, interpolating between the last two frames.
 */
import { Deck, OrthographicView, type OrthographicViewState, type PickingInfo } from "@deck.gl/core";
import { PathLayer, PolygonLayer, ScatterplotLayer } from "@deck.gl/layers";
import { useEffect, useRef } from "react";
import type { DetectorView, RibbonView } from "../protocol.gen";
import { ROAD, classColours, divergingOnRoad, hexToRgba, sequentialOnRoad, type Rgba } from "../theme";
import { OrientedRectLayer, type RectData } from "./OrientedRectLayer";
import { type Display, type DrawBuffers, interpolate } from "./interpolate";

export interface ColourScale {
  kind: "categorical" | "sequential" | "diverging";
  /** Sequential: [0, max]; diverging: [−max, max]. */
  max: number;
}

export interface MapLayerSource {
  id: string;
  /** Called every animation frame. */
  display: () => Display;
  /** Fixed RGBA for every vehicle (compare overlay); otherwise coloured by `scale`. */
  fixedColour?: Rgba;
  opacity?: number;
}

export interface MapProps {
  ribbons: RibbonView[];
  detectors: DetectorView[];
  /** Footprints of the road objects on the road now. */
  objects?: { id: string; shape: number[][] }[];
  bounds: number[];
  classes: string[];
  scale: ColourScale;
  sources: MapLayerSource[];
  selected?: bigint | null;
  highlighted?: Set<bigint>;
  trails?: boolean;
  onPick?: (id: bigint | null, source: string) => void;
  onViewport?: (rect: [number, number, number, number]) => void;
  /** Shared view state (compare side by side). */
  viewState?: OrthographicViewState;
  onViewState?: (v: OrthographicViewState) => void;
  /** Rendered frames per second, reported once a second. */
  onRenderRate?: (fps: number) => void;
  hoverLabel?: (source: string, i: number, d: DrawBuffers) => string | null;
}

const ROAD_RGBA = hexToRgba(ROAD);
const BAND_RGBA = hexToRgba("#2c2c2a");
const EDGE_RGBA = hexToRgba("#898781", 200);
const DETECTOR_RGBA = hexToRgba("#c3c2b7", 220);
const RING_RGBA = hexToRgba("#f0efec");
const OBJECT_RGBA = hexToRgba("#898781");
const TRAIL_LEN = 12;

/** Fraction of the view height a long, thin network is zoomed to at first. */
const MIN_ROAD_SHARE = 0.1;

export function fitView(bounds: number[], width: number, height: number): OrthographicViewState {
  const [x0, y0, x1, y1] = bounds;
  const w = Math.max(1, x1 - x0);
  const h = Math.max(1, y1 - y0);
  let zoom = Math.log2(Math.min(width / (w * 1.05), height / (h * 1.2)));
  let cx = (x0 + x1) / 2;
  // A corridor fitted whole is a hairline: zoom in on its start instead.
  const readable = Math.log2((height * MIN_ROAD_SHARE) / h);
  if (zoom < readable) {
    zoom = readable;
    cx = x0 + width / 2 / 2 ** zoom - 5;
  }
  return { target: [cx, (y0 + y1) / 2, 0], zoom, minZoom: -6, maxZoom: 8 };
}

function colourOf(scale: ColourScale, classes: Rgba[], cls: number, v: number): Rgba {
  if (scale.kind === "categorical") return classes[cls] ?? classes[classes.length - 1];
  if (scale.kind === "sequential") return sequentialOnRoad(v / scale.max);
  return divergingOnRoad(v / scale.max);
}

export function MapView(p: MapProps) {
  const host = useRef<HTMLDivElement>(null);
  const deck = useRef<Deck<OrthographicView> | null>(null);
  const props = useRef(p);
  props.current = p;
  const buffers = useRef(new Map<string, DrawBuffers>());
  const trails = useRef(new Map<string, { frame: unknown; paths: Map<bigint, number[]> }>());
  const internalView = useRef<OrthographicViewState | null>(null);

  useEffect(() => {
    const el = host.current!;
    const initial = fitView(props.current.bounds, el.clientWidth || 800, el.clientHeight || 400);
    internalView.current = initial;
    const d = new Deck<OrthographicView>({
      parent: el,
      views: new OrthographicView({ id: "map", flipY: false }),
      initialViewState: props.current.viewState ?? initial,
      controller: { inertia: false, scrollZoom: { smooth: false, speed: 0.02 } },
      useDevicePixels: true,
      onViewStateChange: ({ viewState }) => {
        internalView.current = viewState as OrthographicViewState;
        props.current.onViewState?.(viewState as OrthographicViewState);
        report(viewState as OrthographicViewState);
        return viewState;
      },
      onClick: (info: PickingInfo) => {
        const src = info.layer?.id?.replace(/^agents-/, "");
        const b = src ? buffers.current.get(src) : undefined;
        props.current.onPick?.(b && info.index >= 0 ? b.id[info.index] : null, src ?? "");
      },
      getTooltip: (info: PickingInfo) => {
        const src = info.layer?.id?.startsWith("agents-") ? info.layer.id.slice(7) : null;
        const b = src ? buffers.current.get(src) : undefined;
        if (!src || !b || info.index < 0) return null;
        const label = props.current.hoverLabel?.(src, info.index, b);
        return label ? { text: label } : null;
      },
      getCursor: ({ isHovering, isDragging }) => (isDragging ? "grabbing" : isHovering ? "pointer" : "grab"),
      style: { background: "transparent" },
    });
    deck.current = d;
    let last = 0;
    const report = (v: OrthographicViewState) => {
      const now = performance.now();
      if (now - last < 150) return;
      last = now;
      const scale = 2 ** (v.zoom as number);
      const [cx, cy] = v.target as number[];
      const hw = el.clientWidth / 2 / scale;
      const hh = el.clientHeight / 2 / scale;
      props.current.onViewport?.([cx - hw, cy - hh, cx + hw, cy + hh]);
    };
    report(initial);
    let raf = 0;
    let frames = 0;
    let since = performance.now();
    const loop = () => {
      raf = requestAnimationFrame(loop);
      d.setProps({ layers: layers() });
      frames++;
      const now = performance.now();
      if (now - since >= 1000) {
        props.current.onRenderRate?.((frames * 1000) / (now - since));
        frames = 0;
        since = now;
      }
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      d.finalize();
      deck.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (p.viewState && deck.current) deck.current.setProps({ viewState: p.viewState });
  }, [p.viewState]);

  function layers() {
    const q = props.current;
    const classCols = classColours(q.classes);
    const out: unknown[] = [
      new PolygonLayer({
        id: "ribbons",
        data: q.ribbons,
        getPolygon: (r: RibbonView) => r.outline,
        getFillColor: ROAD_RGBA,
        getLineColor: EDGE_RGBA,
        lineWidthMinPixels: 1,
        stroked: true,
      }),
      new PolygonLayer({
        id: "median-band",
        data: q.ribbons,
        getPolygon: (r: RibbonView) => r.median_band,
        getFillColor: BAND_RGBA,
        stroked: false,
      }),
      new PathLayer({
        id: "screenlines",
        data: q.detectors.filter((d) => d.kind === "screenline"),
        getPath: (d: DetectorView) => d.shape,
        getColor: DETECTOR_RGBA,
        widthMinPixels: 1.5,
      }),
      new PolygonLayer({
        id: "areas",
        data: q.detectors.filter((d) => d.kind === "area"),
        getPolygon: (d: DetectorView) => d.shape,
        filled: false,
        stroked: true,
        getLineColor: DETECTOR_RGBA,
        lineWidthMinPixels: 1,
      }),
      new PolygonLayer({
        id: "objects",
        data: q.objects ?? [],
        getPolygon: (o: { shape: number[][] }) => o.shape,
        getFillColor: OBJECT_RGBA,
        getLineColor: RING_RGBA,
        lineWidthMinPixels: 1,
        stroked: true,
      }),
    ];
    for (const src of q.sources) {
      const disp = src.display();
      const prev = buffers.current.get(src.id);
      const fixed = src.fixedColour;
      const colourKey = fixed ? fixed.join(",") : `${q.scale.kind}:${q.scale.max}:${q.classes.length}`;
      const b = interpolate(disp, prev, colourKey, (cls, v) => fixed ?? colourOf(q.scale, classCols, cls, v));
      buffers.current.set(src.id, b);
      if (q.trails) {
        let tr = trails.current.get(src.id);
        if (!tr) {
          tr = { frame: null, paths: new Map() };
          trails.current.set(src.id, tr);
        }
        if (disp.cur && tr.frame !== disp.cur) {
          tr.frame = disp.cur;
          const seen = new Set<bigint>();
          const f = disp.cur;
          for (let i = 0; i < f.n; i++) {
            const id = f.id[i];
            seen.add(id);
            let path = tr.paths.get(id);
            if (!path) {
              path = [];
              tr.paths.set(id, path);
            }
            path.push(f.x[i], f.y[i]);
            if (path.length > 2 * TRAIL_LEN) path.splice(0, path.length - 2 * TRAIL_LEN);
          }
          for (const id of tr.paths.keys()) if (!seen.has(id)) tr.paths.delete(id);
        }
        const paths = [...tr.paths.values()].filter((x) => x.length >= 4);
        out.push(
          new PathLayer({
            id: `trails-${src.id}`,
            data: paths,
            getPath: (x: number[]) => {
              const pts: [number, number][] = [];
              for (let k = 0; k < x.length; k += 2) pts.push([x[k], x[k + 1]]);
              return pts;
            },
            getColor: [195, 194, 183, 90],
            widthMinPixels: 1,
            updateTriggers: { getPath: tr.frame },
          }),
        );
      }
      const data: RectData = {
        length: b.n,
        attributes: {
          getPosition: { value: b.pos, size: 2 },
          getSize: { value: b.size, size: 2 },
          getAngle: { value: b.angle, size: 1 },
          getFillColor: { value: b.colour, size: 4, normalized: true },
        },
      };
      out.push(new OrientedRectLayer({ id: `agents-${src.id}`, data, pickable: true, opacity: src.opacity ?? 1, minPixels: 3 }));
      const rings: { x: number; y: number; r: number }[] = [];
      for (let i = 0; i < b.n; i++) {
        const id = b.id[i];
        if (id === q.selected || q.highlighted?.has(id)) rings.push({ x: b.pos[2 * i], y: b.pos[2 * i + 1], r: 0.6 * b.size[2 * i] + 1 });
      }
      if (rings.length)
        out.push(
          new ScatterplotLayer({
            id: `rings-${src.id}`,
            data: rings,
            getPosition: (r: { x: number; y: number }) => [r.x, r.y],
            getRadius: (r: { r: number }) => r.r,
            radiusMinPixels: 7,
            filled: false,
            stroked: true,
            getLineColor: RING_RGBA,
            lineWidthMinPixels: 2,
          }),
        );
    }
    return out as never[];
  }

  return <div ref={host} className="map" />;
}
