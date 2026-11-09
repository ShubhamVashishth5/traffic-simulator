# P11 · Network scale-up (zones of 20–100 junctions)

**Goal:** run the same behaviour on real neighbourhood networks imported from OpenStreetMap, at the
zone performance target. Optionally add route choice as a separate experiment.

- **Depends on:** P7 (junctions); P9 for zone studies.
- **Read first:**
  - [02-architecture.md](../02-architecture.md) §1, §3, §4 and the performance targets;
  - guide §7.5.

## Tasks

### P11.1 OSM importer
- **Do:**
  - Python tool (OSMnx + shapely): pull a polygon, build link ribbons with width from tags (`width`,
    `lanes`, road class defaults), medians and opening positions, footpaths, and junction boxes with
    movement ribbons.
  - Write the network YAML/binary.
  - A manual override file for widths, medians and footpath bollards (site survey).
- **Done when:** a ~1 km² Delhi patch imports, builds and renders. Manual overrides apply and are
  recorded in provenance.

### P11.2 Network demand
- **Do:** OD matrices or entry flows plus turning shares across many entries; fixed routes from
  shortest paths (time-weighted); route sets stored in the spec.
- **Done when:** realised turning counts match the inputs at sample junctions.

### P11.3 Parallel scaling
- **Do:**
  - Profile the zone case.
  - Partition work by ribbon groups; tune chunk sizes.
  - Consider f32 for hot columns only if benchmarks justify it (ADR).
  - Keep determinism.
- **Done when:** ≥ 10× real time on 8 cores for ~50 junctions and ~20,000 agents, deterministic
  across thread counts.

### P11.4 Viewer at zone scale
- **Do:** server-side viewport culling, level of detail (dots when zoomed out), MapLibre basemap,
  and zone overlays (enforcement field, kerb occupancy).
- **Done when:** the zone case renders at 30 fps at 1–5× real time.

### P11.5 Optional: route choice
- **Do:** a separate module for adaptive rerouting (a navigation-app-user share, G§7.5), off by
  default and kept separate from the lane-level dilemmas.
- **Done when:** an experiment comparing fixed routes and adaptive rerouting runs and reports the
  difference.

## Milestone check
A Shahdara-type and a Lutyens-type zone patch imported from OSM, surveyed overrides applied, the P9
population-swap experiment re-run at network scale, and the performance target met.
