# ADR-0002: Continuous lateral coordinates on ribbons (no lanes, no strips)

- **Status:** proposed
- **Date:** 2026-11-09
- **Phase / task:** plan (affects P2, P5, P7, P11)

## Context
Indian mixed traffic is lane-free (guide §2, §4.4). The guide's mechanisms are continuous in metres:
clearance vs speed (C = 0.615 v̄ + 120.9 cm), effective width, pothole offsets, lateral position
distributions. The scale has to grow from corridors to zones of 20–100 junctions.

## Options
1. **Ribbons:** a reference line with arc length `s`, a continuous lateral `y` within a width
   profile, and movement ribbons inside junctions.
2. Strips or sublanes (SiMTraM, SUMO sublane): lateral position discretised into strips.
3. Free 2-D world coordinates everywhere, social-force style.

## Decision
Option 1.

- **Why not strips.** They add resolution artefacts to exactly the quantities we validate. SUMO's
  maintainers note resolution adds noise and slows runs.
- **Why not free 2-D.** It makes route following, deadlines and detector definitions awkward, and
  costs more in neighbour search.
- **Coverage.** Ribbons give continuous lateral freedom while keeping a road-aligned frame for
  following, routes and detectors. World coordinates are used only for cross-ribbon interaction
  inside junctions.

## Consequences
- Junctions need movement ribbons and conflict areas (P7). The OSM importer must produce ribbons
  (P11).
- Footpaths, service roads and the opposite carriageway are strips with access flags. That makes
  footpath riding and wrong-way riding simple moves, not special cases.
