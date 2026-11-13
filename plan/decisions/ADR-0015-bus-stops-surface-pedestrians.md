# ADR-0015: Bus stops, the road surface and pedestrians

- **Status:** proposed. Implemented in P6.4–P6.7 by agreement to choose and record
  implementation rules and have them reviewed afterwards. Items marked ⚑ are the ones where
  another choice was reasonable.
- **Date:** 2026-11-12
- **Phase / task:** P6.4, P6.5, P6.6, P6.7

## Context

Plan/03 §5 and §10 give each of these a table row or a few bullets. The rules below fill them
in. As with parking (ADR-0013) and stop-anywhere (ADR-0014), **the guide has no numbers** for
most inputs; the register leaves them unset and a run that needs one is refused by name.

## Bus stops (P6.4)

`network.bus_stops`: platform station and length, the share of buses that serve it, the width
of carriageway taken by waiting passengers, and optionally a multiplier on the stop-anywhere
rate along the platform (autos gather at stops).

1. A bus that serves a stop uses ADR-0014's stop mechanism: it makes for the kerb, brakes to a
   stand, dwells (`road.bus.dwell_time`) and drives on.
2. It aims to stand at the platform station plus a drawn longitudinal offset, with a drawn gap
   to the kerb (`road.bus.stop_offset_*`), outside the passengers.
3. ⚑ It starts making for the kerb from as far off as drivers look ahead
   (`core.lookahead_distance`). From its braking distance only, a bus in the middle of the road
   stood 3 m out.
4. ⚑ It counts as standing only at its stop, or right behind a bus that is standing there.
   Stopped in a queue further back, it is just in a queue.
5. Passengers waiting on the road are a strip along the platform: a body to everyone else.
   ⚑ To a bus pulling in at that stop they are a wall beside it, not something ahead to queue
   behind (otherwise the bus stops short of its own platform).

## Potholes (P6.5)

`network.pothole_fields`: a stretch, a density (or the register's), and a kerb bias. Potholes
are laid as a Poisson process along the stretch, with the register's diameter and depth.

6. A pothole is not a body. A driver whose wheels' path it lies in, and who sees it, chooses
   once: **swerve, slow, straddle or hit**, with probabilities by group of classes
   (two-wheelers and bicycles; cars and three-wheelers; goods vehicles, buses and tractors:
   `road.pothole.response.<group>.<choice>`; what the three leave is hitting).
7. ⚑ Aggressiveness shifts the choice from slowing to hitting: the weight of slowing is
   multiplied by `exp(−g·κ)` and of hitting by `exp(+g·κ)`
   (`road.pothole.aggressiveness_link`, unset; 0 switches it off).
8. **Swerve:** lines that miss the hole are worth more in the lateral choice (ADR-0010), by
   what slowing for it would cost; the move itself goes through the usual feasibility. If the
   driver does not get clear, he slows. **Slow:** he comes down to the group's crossing speed
   by the hole. **Straddle:** nothing. **Hit:** nothing until he is on it; there the crossing
   speed caps him.
9. ⚑ He sees it within `road.pothole.detection_distance`, unless a vehicle in his path is
   between him and it, or water covers it. Unseen, it is hit. The guide also names speed and
   darkness; those wait for P9.
10. ⚑ Coming down to a speed by a point uses the Intelligent Driver Model's braking strategy,
    `b_kin²/b` with `b_kin = (v² − v_c²)/(2·gap)`. Humps and water use the same law. (A first
    version, a free acceleration towards the allowed speed, arrived about 25% too fast.)
11. Depth is drawn and recorded but does not yet change the response.

## Humps, gradients, roughness, water (P6.6)

12. **Hump** (`network.humps`): each driver has a hump speed drawn from
    `road.hump.crossing_speed` (the register's 14–23 km/h). He comes down to it for a hump in
    his path. Lines that miss the hump are worth more, so where it stops short of the kerb
    those who fit go round its end. Nothing says "two-wheelers bypass": it follows from width.
13. **Gradient and roughness** (`network.surfaces`): the desired speed falls by the register's
    loss per percent of upgrade and per IRI unit, by class. ⚑ Roughness applies the car value
    to cars and three-wheelers and the heavy value to goods vehicles and buses; two-wheelers
    are not slowed (the guide gives only "cars more than heavy vehicles").
14. ⚑ **E-rickshaw on an upgrade:** capped at `power / (mass × g × gradient)`, rolling
    resistance and drag neglected, as the guide suggests. New entry
    `vehicles.e_rickshaw.loaded_mass` (unset).
15. **Water** (`network.waterlogging`): a patch with a speed it can be driven through at
    (set per patch; G§6.10 cites Pregnolato et al. 2017 for the depth–speed curve), or closed,
    which makes it a body. It hides the potholes under it.

## Pedestrians (P6.7)

`network.pedestrian_crossings`: a stretch, crossings per hour, the share starting from the left.

16. ⚑ A pedestrian or group is a **moving road object**, not an agent in the vehicle store.
    Vehicles brake for it and steer round it as for any body (ADR-0012). A group of n walks
    abreast along the road.
17. ⚑ **Rolling gap acceptance, vehicle by vehicle.** The pedestrian steps into a vehicle's
    path only if that vehicle is at least its accepted gap away in time
    (`road.pedestrian.accepted_gap`, drawn per crossing). Otherwise it waits where it is, at
    the edge or in the middle of the road. Once in a vehicle's path it keeps walking. It never
    walks into a vehicle level with it. There are no bands or lanes in this rule.
18. Simplified: pedestrians do not interact with each other or with parked vehicles, cross
    straight, and all walk at one speed. Pedestrians walking along the road because the
    footpath is blocked are a strip object a scenario places (`network.objects`).

## Register

New, all unset: `road.pothole.response.<group>.<swerve|slow|straddle>`,
`road.pothole.crossing_speed.<group>` (these replace the two single unset entries),
`road.pothole.aggressiveness_link`, `road.pedestrian.walking_speed`,
`road.pedestrian.body_size`, `vehicles.e_rickshaw.loaded_mass`.

## Outputs

Events `bus_stop`, `bus_standing`, `pothole_crossed` (choice, speed, size),
`pedestrian_crossing` (group, accepted gap, waits, duration). Object kinds `waiting_passengers`,
`pedestrians`, `pothole`, `hump`, `water`.

## Results

See `reports/phases/P6.md` (`scripts/p6_milestone.py`). Tests: `tests/friction.rs`.
Scenarios without any of this give bitwise the same results (reference corridor: same hash).
