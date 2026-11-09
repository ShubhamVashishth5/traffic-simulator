# ADR-0009: Roundabouts

- **Status:** proposed: **needs the user's decision** (open question 11)
- **Date:** 2026-11-09
- **Phase / task:** P7 (review gate P7.0), read by P11

## Context
Open question 7 names Lutyens'-type patches as study sites, and guide §5.3 describes that zone
type as "wide roads, continuous medians, roundabouts". The Lutyens' vs Shahdara contrast is one of
the thesis comparisons, so a model of Lutyens' Delhi without roundabouts would leave out the
junction type that defines it. P7 as written covers signalised and uncontrolled junctions,
median openings and splits, but not roundabouts.

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
3. **Out of scope.** The P11 importer flags roundabouts as unsupported, and Lutyens'-type studies
   use patches without them.
   - *Pros:* no work.
   - *Cons:* the Lutyens' vs Shahdara comparison can no longer be run on real Lutyens' networks.
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
- If option 3 is chosen instead, open question 7 should say that Lutyens'-type study patches
  exclude roundabouts.
