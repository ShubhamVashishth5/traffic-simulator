# ADR-0012: Road objects, a narrowing carriageway, and pulling out from a standstill

- **Status:** part 1 implemented, needs review (it adds rules the plan did not spell out).
  **Part 2 decided by the user on 2026-11-12: option A** (a creep speed for every class);
  implemented, see "Decision" below.
- **Date:** 2026-11-12
- **Phase / task:** P6.1 (governs P6.2–P6.8)

## Context

P6.1 asks for road objects that the lateral logic sees "as static neighbours", for width changes
in a ribbon's own profile to be "seen ahead the same way", and for the P5.5 bottleneck scenarios
to log no collision-guard events. Before P6.1 the lateral model read the road width only where
the vehicle was. Where the road narrowed, the kerb moved vehicles sideways into their neighbours
and the guard stepped in: 964,509 events in 2,050 of the 4,086 random narrowing scenarios.

Two things came out of the work. The first is a set of rules for a narrowing road that the
plan's one sentence does not contain. The second is a problem the existing lateral model has
with anything that stands still, which this task makes visible and cannot settle on its own.

## Part 1. What is implemented (for review)

### Objects

An object is a rectangle on a ribbon (`network.objects`: id, ribbon, `s`, `y`, length, width,
optional `clearance_as`, `response` per class, `present` periods). In the scan of the move phase
it is a standing body, handled with ADR-0010's equations and no new parameter:

- **ahead**, it is a leader: IDM to a standing leader, weighted by `ω` from the lateral gap and
  the minimum clearance;
- **level**, it is a side neighbour: steering response, and a limit on lateral movement. A
  vehicle may use the whole gap to it, not half, because it does not move;
- **in the lateral choice**, it is a leader term or a side term like a vehicle.

The clearance line is that of the pair with `clearance_as` (a parked car is passed like a car),
or the pooled line (ADR-0010, option C1, for pairs the paper did not observe). The response per
class is `block` (the default) or `ignore`; speed caps and the pothole responses come with
P6.5–P6.6. An object that appears on top of a vehicle is ignored by that vehicle until it has
driven off. A vehicle that drives into an object is put back by the collision guard and logged.
There is no field for a capacity or flow factor, and an unknown field is refused (ADR-0005).

### A narrowing carriageway

1. **The kerb does not turn a body.** A vehicle the kerb moves over keeps its heading. Before,
   a 12 m truck following a 5° taper was given that heading and its corners "swung" 0.5 m to
   each side, into its neighbours and through the kerb itself.
2. **The kerb never pushes a vehicle into a neighbour's half of the gap.** If it would, the
   vehicle stops short. This is logged as a `collision_guard` event (`other: edge`), so it is
   counted, not hidden.
3. **Vehicles brake for the kerb ahead** as for a standing obstacle, at the station where it
   would take them inside their minimum clearance of a vehicle level with them.
4. **The lateral choice sees the narrowing.** A line that runs into the kerb within the
   lookahead is worth what following a standing obstacle there is worth. This is an incentive
   to move over early, the fourth channel of G§6.
5. **The vehicle in front goes first.** A vehicle ahead becomes a leader if there will be no
   room to draw level with it: the kerb closes the space beside it, the road becomes too narrow
   for the two, or its own kerb will bring it into the follower's line. Of two vehicles level
   with each other that cannot stay abreast further on, the one whose front is ahead goes on and
   the other brakes so as not to be beside it there. A vehicle coming up behind has no say in
   where the kerb moves the one in front.

Rule 5 is a behavioural assumption (a zipper merge by position). It is deterministic and has no
parameter. Urgency and aggressiveness will want a say in it in P7 and P8.

### Results

| Check | Before | After |
| --- | --- | --- |
| Guard events, 10⁴ random dense scenarios (208 M agent-steps) | 964,509 in 2,050 scenarios | **136 in 72 scenarios** |
| Guard events per 10⁶ agent-steps | 4,587 | **0.65** |
| Overlapping bodies after the guard | 0 | 0 |
| Guard events where the width does not change (5,914 scenarios) | 0 | 0 |
| Vehicles through the 4,086 narrowing scenarios | 89,775 | 92,286 (+2.8%) |
| Reference corridor, barrier queue, moving bottleneck: output hash | | unchanged |
| Reference corridor, ns per agent-step (3 runs each, same machine) | 922 | 944 (+2.3%, within noise) |

**The card asks for no guard events; 136 remain.** All but a handful are a vehicle stopped
short at the kerb because a neighbour did not give way in time (rule 2 acting). Five, in one
scenario, are an older case that has nothing to do with narrowing: a bicycle creeping sideways
behind the swung-out rear corner of a turning auto in a standing queue.

A version without rule 5 reached 7 events in 3 scenarios, but it locked up: two buses abreast
at the head of a 10.5 m to 5 m taper, each braking for the kerb, with a motorcycle's nose between
their tails. A test now requires the taper to keep discharging.

### Not done in P6.1

- Conflicts (`conflicts.parquet`) do not include objects.
- The live inspector's neighbour list does not show objects or the kerb.
- The viewer draws objects and adds them through a small form (ribbon, `s`, `y`, length,
  width); placing by clicking on the map is not built.
- The kerb acts at a vehicle's centre station, as before, so the front corner of a long vehicle
  can still cross a taper's line.

## Part 2. Pulling out from behind something that stands still

### The problem

ADR-0010 §4 limits lateral speed to `v · tan θ_max`, plus a creep speed when nearly stopped.
The register gives a creep speed to two-wheelers and bicycles (0.5 m/s, "assumed", grade C) and
0 to every other class. So a car that has come to a stop behind a standing body cannot move
sideways at all, and cannot move forward either. It is stuck for good, and it is then a
standing body for the vehicle behind it.

Until P6 nothing stood still for long, so this did not show. With one parked car (4.5 × 1.8 m)
in the middle of a 10.5 m road, 30 minutes, mixed traffic
(`cargo test --release -p marg-core --test objects standing_obstacle -- --ignored --nocapture`):

| Demand (veh/h) | Creep | Throughput (veh/h) | Mean delay (s) | Standing behind the object at the end |
| --- | --- | --- | --- | --- |
| 600 | register | 584 | 1.6 | 0 |
| 600 | every class, 0.5 m/s | 584 | 1.6 | 0 |
| 1,200 | register | 668 | 14.8 | 268 |
| 1,200 | every class, 0.5 m/s | 1,194 | 3.0 | 0 |
| 1,800 | register | 508 | 8.3 | 369 |
| 1,800 | every class, 0.5 m/s | 1,816 | 5.1 | 0 |
| 4,000 | register | 314 | 37.7 | 343 |
| 4,000 | every class, 0.5 m/s | 3,514 | 93.1 | 21 |

With the register as it is, one parked car gridlocks the whole width at 1,200 veh/h. Every
later P6 task (parking, bus stops, stop-anywhere) puts standing bodies on the road, so none of
their validation targets can be approached until this is settled.

### Options

**A. A creep speed for every class.** Change the existing register values
(`interaction.lateral.creep_speed.<class>`, role `fit`, stage 3). No new mechanism.
- For: smallest change; the table above shows it works; the parameter already exists.
- Against: a bus sliding sideways at 0.5 m/s at a standstill is not how a bus moves. It also
  lets cars seep in standing queues, so the P5.4 result (two-wheelers gain places at a barrier,
  cars lose them) has to be re-checked. The values would be guesses (grade C); smaller ones for
  larger vehicles (for example 0.5 / 0.3 / 0.15 m/s) are more plausible than one value.

**B. A swing-out gap.** A vehicle whose lateral target is clear of a standing leader measures
its gap along the path it can steer (`gap − Δy / tan θ_max`), and stops that much further back.
It then pulls out moving forward, within the heading limit.
- For: physical; no sideways sliding; leaves P5 queue results alone.
- Against: a new rule with its own cases (the target changes, a neighbour blocks the swing, the
  vehicle already stopped too close). Needs its own micro-scenarios and probably a parameter
  for the heading available at crawl speed. More work, and it may still need a small creep as a
  fallback.

**C. Leave it.** Objects are then usable only in light traffic.
- Against: blocks stage 5.

### Decision (2026-11-12): option A

The user chose A. Option B stays the model to return to if the creep values turn out to matter
in the stage-3 or stage-5 fits.

**Register.** `interaction.lateral.creep_speed.<class>` (grade C, `fit`, stage 3, all assumed,
none measured):

| Classes | Creep speed (m/s) |
| --- | --- |
| two-wheeler, bicycle | 0.5 (unchanged) |
| car, auto-rickshaw, e-rickshaw, cycle-rickshaw | 0.3 |
| LCV, bus, truck, tractor | 0.15 |

**Two rules were needed with it (please review).** ADR-0010 §4 turns a body that creeps at a
standstill to the heading limit at once. That was harmless for a 1.9 m motorcycle. For a car
or a bus it is not:

1. *A turned body is longer along the road as well as wider.* A 4.7 m car at 0.2 rad reaches
   0.13 m further at each end. A vehicle does not creep if that would reach a body just in
   front of or behind it (each vehicle's own possible turn is allowed for).
2. *The corners of a long vehicle swing further than its side neighbours' range.* A 12 m bus
   swings 1.2 m to each side. A creeping vehicle's lateral limits now count every body within
   its swing, with half of the gap each, as for side neighbours.

Both apply only to a vehicle that is creeping (below the stop speed, moving sideways faster
than its forward movement allows). A moving vehicle is treated exactly as before, so scenarios
in which nothing stops give bitwise the same results (reference corridor: same output hash).

**Results.**

| Check | Before (two-wheelers and bicycles creep) | After (every class) |
| --- | --- | --- |
| One parked car, 10.5 m road, 600 veh/h: throughput | 584 | 584 |
| 1,200 veh/h: throughput / standing behind it at the end | 602 / 311 | **1,194 / 0** |
| 1,800 veh/h | 508 / 367 | **1,804 / 2** |
| 4,000 veh/h (over capacity) | 320 / 347 | **3,292 / 47** |
| Barrier queue (P5.4), places gained in the queue: two-wheelers / cars | +1.4 to +1.7 / −0.4 to −0.9 | **+2.35 / −0.55** (seed 1, three closures) |
| Guard events, 10⁴ random dense scenarios | 136 in 72 scenarios | **93 in 44**, all where the road narrows |
| Overlapping bodies after the guard | 0 | 0 |
| Reference corridor: output hash, ns per agent-step | | unchanged, 940 (944 before) |

The P5.4 result holds: two-wheelers still gain places at a barrier and cars still lose them.
Cars rarely creep in a packed queue because the swing of their corners needs room on both
sides. Without the two rules above the barrier scenario logged 8 guard events and the random
scenarios 14 outside narrowings; with them, none.

**The creep values matter (P6.8, 2026-11-12).** Parking costs 1.6 times as much capacity as the
effective width it takes; a plain narrower road costs 1.0 times. The loss is made where the
parked row begins, by vehicles that stop behind the first parked vehicle and creep out. With
every class creeping at 1.5 m/s the same kerb is close to one for one
(`reports/phases/P6.md`). This is the condition under which option B was to be revisited.

**Tried and dropped: keeping a creeping body's corners on the carriageway.** The kerb bounds
a body's unturned width (since P5), so the corners of a long vehicle that turns beside the kerb
cross its line: a bus creeping in a queue by up to a metre. Refusing such a turn locks up a
taper: a vehicle stopped against the kerb can then never turn away from it (the taper test
stopped discharging at t = 557 s). Left as it is; it belongs with part 1's open item on the
kerb acting at a vehicle's centre station.

`standing_obstacle_measurement` (ignored test) reproduces the first four rows;
`stopped_vehicles_pull_out_from_behind_a_parked_car` is the regression test.

## Consequences

- `plan/03` §10 and the P6 card point here.
- Scenario schema: `network.objects` (Python and TypeScript types regenerated). Viewer protocol
  version 2: `add_object` takes an object in ribbon coordinates, `remove_object` works, and an
  `objects` message lists them. `add_object` and `remove_object` are state commands: logged and
  replayed.
- `events.parquet` gains the kinds `object_added` and `object_removed`, and `collision_guard`
  events against an object or the kerb. No column changed.
- Scenarios without objects or width changes give bitwise the same results as before.
