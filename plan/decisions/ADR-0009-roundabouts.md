# ADR-0009: Roundabouts

- **Status:** accepted — **option 1, a ring of ribbons** (the user, 2026-11-13, at the P7.0 gate).
  The ring is built and run (P7.8, 2026-11-14): it carries light traffic and **locks at 300–500
  veh/h per arm**. How vehicles on a ring are arbitrated **needs your decision** (addendum below)
- **Date:** 2026-11-09
- **Phase / task:** P7 (review gate P7.0), read by P11

## Context
The simulator should handle most real road situations, and roundabouts are common: guide §5.3
describes Lutyens'-type zones as "wide roads, continuous medians, roundabouts", and OSM imports
(P11) will contain them. P7 as written covers signalised and uncontrolled junctions, median
openings and splits, but not roundabouts.

The guide gives no roundabout behaviour or numbers. Its only mention is the §5.3 zone-profile
row. Its critical gaps (§2.4) come from uncontrolled intersections in Warangal (Abhigna et al.
2020), not from roundabout entries. Its weaving material (§6.7) is from motorway weaving sections,
and gives capacity drops of 3–20%, which are validation targets.

Three things therefore need a decision, not just an implementation:

1. **Representation.** How the circulating roadway, entries and exits are built. The network
   builder rejects cycles today (`marg_net::Network::build`: ribbons must form chains), and a
   circulating roadway is a cycle.
2. **Behaviour with no guide numbers:**
   - who yields at an entry (circulating traffic has priority by rule, but entering traffic often
     forces its way in);
   - entry critical gaps by class;
   - circulating speed;
   - when an agent starts moving outward to exit.
3. **Grades.** Every new parameter would start as grade C (swept), unless Indian roundabout
   studies are found and graded first.

## Options
1. **A ring of ribbons (proposed).** The circulating roadway is a closed loop of arc ribbons,
   split at each entry and exit.
   - Entries are merges with gap acceptance against circulating traffic (P7.4), plus a
     priority-compliance share: a swept parameter for the fraction of entrants who wait for a gap
     rather than force their way in.
   - Exits are diverges with a deadline, so they reuse the P7.6 urgency logic. Weaving on the
     ring then comes from the P5 lateral model, as on any link.
   - Circulating speed is capped by the ring radius: a speed cap from geometry, one of the four
     channels ADR-0005 allows.
   - *Pros:* mechanism-based, as ADR-0005 requires; weaving and entry blocking emerge; large
     Delhi rotaries are represented as they are.
   - *Cons:* the network graph must allow cycles, and the leader scan must stop after one lap.
     About five new register entries, all grade C. One more P7 task with its own micro-tests.
2. **An ordinary junction box.** The roundabout becomes a P7.1 junction with one movement ribbon
   per entry–exit pair through the central area, and conflict areas where those ribbons overlap.
   - *Pros:* no new topology; P7.1 is reused unchanged.
   - *Cons:* there is no circulating stream, so priority to the ring and weaving between entry and
     exit cannot appear. Movement ribbons through a large rotary overlap almost everywhere, giving
     huge conflict areas. Adequate for small mini-roundabouts at most.
3. **Out of scope.** The P11 importer flags roundabouts as unsupported. **Ruled out by the user
   (2026-11-09).**
4. **A capacity formula** (a roundabout capacity equation, e.g. from Indian design guidance such
   as IRC:65 or the HCM, to be checked). **Rejected:** ADR-0005 forbids capacity formulas as model
   inputs. Such equations can serve as validation checks under options 1 and 2.

## Decision (proposed, pending review)
Option 1, as a new task P7.8 after P7.4 and P7.6, because it builds on both. Its outline:

- **Network:** cycle support (a closed ring of arc ribbons), with the leader scan bounded to one
  lap.
- **New register entries,** grade C and role `sweep` until sources are found:
  - `junctions.roundabout.entry_critical_gap.<class>` (lognormal, scaled like §2.4 gaps);
  - `junctions.roundabout.priority_compliance` (share of entrants who yield);
  - `junctions.roundabout.exit_awareness` (distance before the exit at which the move outward
    starts, reusing the §6.7 awareness mixture);
  - the lateral acceleration that sets circulating speed from radius.
- **Validation targets, not inputs:** entry capacity against circulating flow, from Indian design
  guidance once found.
- **Done when:**
  - a four-arm roundabout builds and renders;
  - circulating traffic is never blocked by entrants who comply;
  - raising entry flow on one arm lowers entry capacity on the next arm downstream (the standard
    roundabout pattern), with no collision-guard events.

## Consequences
- P7 grows by one task. P11.1 must recognise roundabouts in OSM (`junction=roundabout`) and build
  rings rather than junction boxes.
- `marg-net` gains cycle support. Every scan that follows `next` must bound itself by distance,
  not by reaching the end of a chain. The current leader scan (`MoveCtx::accel`) stops only when
  it meets an agent beyond the look-ahead, so on an empty ring it would never stop. It must also
  stop once the distance along the chain passes the look-ahead.
- The register gains a `junctions` group with about five grade C entries, which the Lutyens'
  zone studies would sweep.

## As built (2026-11-13)
- `marg-net` builds the ring from `centre`, `ring_radius` and `ring_width`: an arc between each
  pair of neighbouring points where an arm leaves or joins the ring, a connector on from each
  arriving arm and one off to each leaving arm. Circulation is clockwise (left-hand traffic).
- The ring is closed only through branch tables: no ribbon on it has a `next`, so every walk
  that follows `next` still ends, and the walk that follows a vehicle's own route leaves the ring
  at its arm. Route-following walks also stop once they are past the lookahead.
- Register entries differ from the outline above in two places. The entry critical gap is one
  factor on each class's §2.4 gap (`road.junction.roundabout.entry_critical_gap_factor`), not
  ten new distributions. `exit_awareness` was not added: a vehicle can leave from any part of
  the ring's width. Both are unset in the register, with `priority_compliance`; a scenario with
  a roundabout must supply them.
- Speed on the ring comes from `road.junction.turn_lateral_acceleration` and the radius.

## Addendum (2026-11-14): P7.8 run for the first time

**Fixed, with tests** *(mine, please review)*:
1. The scenario's stop lines lay on the ring (arms ending 14 m from the centre of a ring whose
   outer edge is at 20 m): circulating vehicles braked for entrants waiting at their lines and
   the ring locked at once. A ring whose arms end inside its carriageway is now refused, and the
   scenario's arms end 4 m outside it.
2. Ways on and off could change order: each was moved along the ring by up to 45°, so with arms
   ending well outside the ring an arm's way on came after the next arm's way off (the first
   exit became the last, and connectors of neighbouring arms crossed). The move is now at most
   40% of the angle to the nearest other arm (`LEAD_SHARE`); a test checks that the left turn
   takes the first exit.
3. Two vehicles merging onto the same arc were never compared (ADR-0016 part 5).
4. Ring-only arbitration rules (ADR-0016 part 5): tickets ordered along shared arcs; first come,
   first served decides a pair only once one of them is near the contact; the yielder rule of
   part 4 applied to bodies ahead at a shallow angle; "blocked" read from the move phase too.

**Result** (`reports/phases/P7.md` §5, 30 min, three seeds): 150 veh/h per arm flows on every
seed (0–15 guard events); 300 veh/h locks on one seed in three; 500 veh/h (the scenario) locks
within minutes on every seed. The done-when is not met: entry capacity against upstream flow
cannot be measured on a ring that locks.

**Why.** Decision D (ADR-0016 part 4) was built for a box that vehicles cross in seconds: a
small area, short paths, an order of tickets. On a ring every vehicle is inside the junction
for its whole time there and paths run round it, so tickets order vehicles that are far apart,
and cycles of waiting form through vehicles following one another, which the cycle breaker
does not see. Long vehicles on a continuous curve also meet open problem 3 of ADR-0016 (the
inside sweep of a bus) all the time.

**Options for your decision:**
- R1. *(recommended)* **Each entry its own small junction.** The ring's arcs are ordinary
  ribbons; each way on is a priority merge (the connector and the arc it joins, ring first,
  P7.4 gap acceptance at the give-way line) and each way off a diverge. Arbitration then spans
  one merge area, as a box does, and vehicles on the ring follow one another with the ordinary
  scan. Roughly a rewrite of `build_ring`'s grouping, not of the arbitration.
- R2. **One junction, priority instead of tickets.** At a roundabout a ring vehicle always goes
  before an entrant still on its connector unless physics says otherwise; cycles through
  following are found and broken too. Smaller change, but keeps long paths in one arbitration.
- R3. **A plain box** (option 2 above) for small roundabouts only, leaving large rotaries
  unsupported for now.
