# ADR-0013: Kerbside parking and the effective-width observer

- **Status:** proposed. Implemented in P6.2; it adds rules the plan's one table row does not
  spell out, so please review. Items marked ⚑ are the ones where another choice was reasonable.
- **Date:** 2026-11-12
- **Phase / task:** P6.2 (also used by P6.8)

## Context

Plan/03 §10 gives parking one row: a parked vehicle is a static obstacle (footprint plus
offset), parking and unparking block space for a lognormal manoeuvre time, cruisers slow near
the kerb. G§6.2 adds that `W_eff = W − Σ w_i − c_shy` with `c_shy` from the clearance model,
and that the Varanasi width–capacity relation must emerge from it.

The guide has **no numbers** for the kerb offset, double parking, the manoeuvre rate and
duration, or the cruising share (grade B–C, "survey"). The register has them unset, and they
stay unset: nothing was invented.

## Decision

### Where the numbers come from

A scenario describes each stretch of kerb in `network.parking`: ribbon, `s_from`–`s_to`, side,
`occupancy` (share of the kerb length parked, measured from video or imagery) and the
`composition` of what parks there.

The survey quantities can be given per stretch (`offset_from_kerb`,
`double_parking_probability`, `manoeuvres_per_100m_h`, `cruising_share`). Where a stretch
omits one, the register value is used (`road.parking.*`). If that is unset too, **the run is
refused** with the name of what is missing. Nothing defaults silently to zero.

The manoeuvre duration and the cruising speed come from the register only
(`road.parking.manoeuvre_duration`, and the new `road.parking.cruising_speed`: unset, grade C,
`fit`, stage 5). They are needed only if the rate or the share is above zero.

### Parked vehicles

1. Vehicles are drawn one after another (type from the composition, length and width from
   that type's distributions, offset from the kerb) until the next one would take the parked
   length over `occupancy × kerb length`. So the kerb is short of the target by less than one
   vehicle.
2. ⚑ The free length is divided at random between them (a uniform random division), so parked
   vehicles bunch and leave gaps as they do on a street. There is no bay grid and no spacing
   parameter.
3. With the double-parking probability, a parked vehicle has a second one standing beside it,
   on the carriageway side, at its own drawn offset. The inner one cannot leave.
4. Each is a road object (ADR-0012) with the clearance line of its own class. Traffic keeps
   its lateral clearance to it; that is the whole effect. There is no capacity or speed factor.

### Manoeuvres

5. Manoeuvres are a Poisson process over the stretch (rate × length / 100 m).
6. ⚑ Each is an arrival or a departure with equal probability, so occupancy stays near its
   starting value. If the kind drawn is impossible (nobody free to leave; no gap long enough),
   the other happens.
7. ⚑ A manoeuvring vehicle blocks **the strip beside its space, one vehicle wide**, for the
   drawn duration. A departing vehicle's space is empty from the moment it starts; an arriving
   vehicle's space fills when it ends.
8. ⚑ A manoeuvre begins only when nobody is on that strip and nobody approaching it would have
   to brake harder than `interaction.lateral.safe_decel`. This is the safety criterion of a
   lateral move (ADR-0010 §3) applied to the vehicle that manoeuvres; it needs no new
   parameter. Until then the manoeuvre waits, and the wait is logged. An arriving vehicle also
   waits in the strip while traffic is using its space.
9. **Simplified:** a vehicle that leaves does not join the stream, and one that arrives does
   not come out of it. Flows are unchanged.

### Cruising

10. Each agent gets one uniform draw. Along a stretch whose cruising share is above its draw,
    and if its class is one that parks there, its desired speed is capped at the cruising
    speed. This is a speed cap, one of the four channels of G§6.
11. **Simplified:** a cruising vehicle never takes a space.

### The effective-width observer

`outputs.effective_width` lists stretches and a bin length. Per bin, over the measured period,
`effective_width.parquet` has:

- `blocked_width`: footprints, plus any strip between a footprint and the kerb (or another
  footprint) narrower than the narrowest vehicle type. This is "footprint plus offset".
- `shy_desired`, `shy_minimum`: ADR-0010's desired and minimum clearance to the nearest object,
  for the vehicles that were actually beside one (their class, speed and κ).
- `effective_width_model = free_width − shy_desired`.
- ⚑ `shy_observed`: the gap to the nearest object that vehicles beside one were within for 5%
  of their time. `effective_width_observed = free_width − shy_observed`.
- the lateral band vehicle bodies covered, the time spent, the mean speed.

Nothing in the model reads these numbers.

## Results

`scenarios/kerbside_parking.yaml`: 10.5 m carriageway, 500 m of kerb at 70% occupancy, 2,000
veh/h, seeds 1–3. The survey inputs in it are placeholders.

| | Seed 1 | Seed 2 | Seed 3 |
| --- | --- | --- | --- |
| Blocked width (m) | 1.31 | 1.29 | 1.24 |
| Shy distance: model desired / minimum (m) | 1.35 / 0.15 | 1.34 / 0.15 | 1.35 / 0.15 |
| Shy distance observed (m) | 1.39 | 1.29 | 1.34 |
| `W_eff` model / observed (m) | 7.84 / 7.80 | 7.87 / 7.91 | 7.91 / 7.93 |
| Speed before the stretch / beside it (km/h) | 45.9 / 33.7 | 44.6 / 31.3 | 44.9 / 24.7 |
| Throughput (veh/h) | 2,052 | 2,088 | 1,930 |
| Manoeuvres; mean and longest wait to begin (s) | 49; 0.1, 1.2 | 39; 0.1, 1.7 | 46; 0.5, 11.5 |
| Guard events | 0 | 0 | 0 |

- **P6.2's check holds.** `W_eff` from the observer agrees with footprints plus the
  clearance-derived shy distance to within 0.04 m over the stretch. In the test (static
  parking, 80% occupancy) it is 7.65 m against 7.61 m, and within 0.5 m in every 50 m bin.
- The stream slows by 27%, 30% and 45% beside the parked kerb. The guide's range for
  on-street parking is 15–44%. This is a validation target; nothing was tuned, and the
  survey inputs are placeholders, so it is an order-of-magnitude check only.
- Seed 3 is the slowest: manoeuvres and cruising vehicles there bring the stretch close to
  its capacity at this demand. P6.8 measures capacity properly.

## Consequences

- Scenario schema: `network.parking`, `outputs.effective_width`; object kinds `parked_vehicle`
  and `parking_manoeuvre`. Python and TypeScript types regenerated.
- Outputs: new table `effective_width.parquet` (only when asked for); event kind
  `parking_manoeuvre`; `object_added` and `object_removed` now carry the object's kind. Output
  schema version 3.
- Register: one new entry, `road.parking.cruising_speed` (unset).
- Each agent has one more draw (its own counter block, so nothing else changes for a seed).
- Scenarios without parking give bitwise the same results (reference corridor: same hash).
- **Known and left alone:** the kerb bounds a body's unturned width (since P5), so the
  corners of a long vehicle that turns right beside the kerb can cross its line (the observer
  showed a bus creeping in a queue a metre over it). The observer therefore measures bodies
  as if straight. A rule against it was tried and locks up a taper (ADR-0012 part 2).
- The viewer shows parked and manoeuvring vehicles as objects (no change needed).
