//! Kerbside parking (G§6.2, plan/03-model-design.md §10, phase P6.2).
//!
//! A parking stretch fills a share of a kerb's length with parked vehicles
//! at the start of the run. Each is a road object, so traffic sees a
//! standing body and keeps its lateral clearance to it: parking acts only
//! by taking width (ADR-0005). During the run vehicles leave and arrive as a
//! Poisson process; each manoeuvre blocks the strip beside its space for a
//! drawn time. A share of the passing vehicles is looking for a space and
//! drives slowly along the stretch.
//!
//! What is simplified: a vehicle that leaves does not join the stream and
//! one that arrives does not come out of it (flows are unchanged), and a
//! vehicle that is looking for a space never takes one.

use crate::agents::AgentStore;
use crate::model::lateral::edge_gap;
use crate::objects::Objects;
use crate::params::{CoreParams, Drawn, N_CLASSES};
use crate::rng::{StreamKind, StreamRng};
use marg_net::Network;
use marg_params::spec::{ObjectKind, ObjectSpec, ParkingSpec, RoadSide};
use marg_params::{Dist, VehicleType};

/// The length of kerb the register's manoeuvre rate refers to (events per
/// 100 m per hour).
const RATE_REFERENCE_KERB_M: f64 = 100.0;
/// Counter blocks of a stretch's random stream: one per parked vehicle, one
/// per manoeuvre, one for the spacing of the vehicles along the kerb.
const DRAW_BLOCK: u64 = 1 << 16;
const EVENT_BASE: u64 = 1 << 40;
const SPACING_BASE: u64 = 1 << 41;

/// What a space at the kerb is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BayState {
    Parked,
    /// Parked with another vehicle standing beside it: it cannot leave.
    BoxedIn,
    /// Waiting to pull out, or pulling out.
    Leaving,
    /// Kept for a vehicle that is waiting to park, or parking.
    Arriving,
}

/// A vehicle-sized space at the kerb.
#[derive(Debug, Clone, PartialEq)]
struct Bay {
    serial: u64,
    /// Centre of the vehicle.
    s: f64,
    y: f64,
    length: f64,
    width: f64,
    vtype: VehicleType,
    state: BayState,
}

/// A parking or unparking manoeuvre that is due.
#[derive(Debug, Clone, PartialEq)]
struct Manoeuvre {
    bay: u64,
    park: bool,
    duration: f64,
    due: f64,
    /// When it ends, once it has begun.
    ends: Option<f64>,
}

/// A manoeuvre that began this step (for the event log). `duration` is the
/// time drawn for it; a vehicle that parks takes longer if traffic is in
/// its space when that time is up.
#[derive(Debug, Clone, PartialEq)]
pub struct ManoeuvreStarted {
    pub zone: String,
    pub object: String,
    pub ribbon: usize,
    pub s: f64,
    pub y: f64,
    pub park: bool,
    pub duration: f64,
    pub waited: f64,
}

/// One stretch of kerb with parking.
#[derive(Debug, Clone)]
struct Zone {
    id: String,
    ribbon: usize,
    s_from: f64,
    s_to: f64,
    left: bool,
    offset: Dist,
    /// Types that park here with cumulative probability.
    types: Vec<(VehicleType, f64)>,
    /// Manoeuvres per second over the whole stretch.
    rate: f64,
    duration: Option<Dist>,
    cruising_share: f64,
    cruising_class: [bool; N_CLASSES],
    /// Sorted by station.
    bays: Vec<Bay>,
    next_serial: u64,
    events: u64,
    next_event_t: f64,
    queue: Vec<Manoeuvre>,
}

/// Every parking stretch of a scenario.
#[derive(Debug, Clone, Default)]
pub struct Parking {
    zones: Vec<Zone>,
    seed: u64,
    cruising_speed: f64,
    any_cruising: bool,
}

fn positive(x: f64, mean: f64) -> f64 {
    if x > 0.0 { x } else { mean }
}

/// A vehicle drawn for a space: type, length, width, gap to the kerb.
fn draw_vehicle(
    rng: &mut StreamRng,
    types: &[(VehicleType, f64)],
    offset: &Dist,
    p: &CoreParams,
) -> (VehicleType, f64, f64, f64) {
    let u = rng.uniform();
    let vtype = types
        .iter()
        .find(|(_, cum)| u < *cum)
        .unwrap_or(types.last().expect("validated: not empty"))
        .0;
    let dim = |drawn, rng: &mut StreamRng| {
        let d = p.draw_dist(drawn, vtype.index());
        positive(d.sample(rng), d.mean())
    };
    let length = dim(Drawn::Length, rng);
    let width = dim(Drawn::Width, rng);
    (vtype, length, width, offset.sample(rng).max(0.0))
}

impl Zone {
    /// The carriageway edge this stretch parks at, at its nearest along a
    /// vehicle of `length` centred at `s`.
    fn kerb(&self, net: &Network, s: f64, length: f64) -> f64 {
        let rib = net.ribbon(self.ribbon);
        let at = |x: f64| rib.y_bounds(x.clamp(0.0, rib.length()));
        let pts = [at(s - 0.5 * length), at(s), at(s + 0.5 * length)];
        if self.left {
            pts.iter().map(|b| b.1).fold(f64::INFINITY, f64::min)
        } else {
            pts.iter().map(|b| b.0).fold(f64::NEG_INFINITY, f64::max)
        }
    }

    /// `y` moved `d` m from the kerb towards the carriageway.
    fn inward(&self, y: f64, d: f64) -> f64 {
        if self.left { y - d } else { y + d }
    }

    fn object_id(&self, what: char, serial: u64) -> String {
        format!("{}/{what}{serial}", self.id)
    }

    fn object(&self, net: &Network, id: String, kind: ObjectKind, b: &Bay, y: f64) -> ObjectSpec {
        ObjectSpec {
            id,
            kind,
            ribbon: net.ribbon(self.ribbon).id.clone(),
            s: b.s,
            y,
            length: b.length,
            width: b.width,
            clearance_as: Some(b.vtype.class()),
            response: Default::default(),
            present: Vec::new(),
        }
    }

    fn bay(&self, serial: u64) -> usize {
        self.bays
            .iter()
            .position(|b| b.serial == serial)
            .expect("a manoeuvre's bay exists until the manoeuvre ends")
    }

    /// Free lengths of kerb as `(from, to)`, in order.
    fn gaps(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let n = self.bays.len();
        (0..=n).map(move |k| {
            let from = if k == 0 {
                self.s_from
            } else {
                self.bays[k - 1].s + 0.5 * self.bays[k - 1].length
            };
            let to = if k == n {
                self.s_to
            } else {
                self.bays[k].s - 0.5 * self.bays[k].length
            };
            (from, to)
        })
    }
}

impl Parking {
    /// Lay out the scenario's parking stretches and put the parked vehicles
    /// on the road as objects.
    pub fn new(
        specs: &[ParkingSpec],
        net: &Network,
        p: &CoreParams,
        seed: u64,
        objects: &mut Objects,
    ) -> Result<Self, String> {
        let mut parking = Self {
            seed,
            ..Self::default()
        };
        for (z, spec) in specs.iter().enumerate() {
            let bad = |m: String| format!("parking `{}`: {m}", spec.id);
            let unset = |field: &str, path: &str| {
                bad(format!(
                    "`{field}` is not given and the register's `{path}` is unset: \
                     give one of them (the guide has no number for it)"
                ))
            };
            if spec.id.is_empty() || specs[..z].iter().any(|o| o.id == spec.id) {
                return Err(bad("ids must be unique and not empty".into()));
            }
            let ribbon = net
                .index(&spec.ribbon)
                .map_err(|_| bad(format!("unknown ribbon `{}`", spec.ribbon)))?;
            let len = net.ribbon(ribbon).length();
            if !(spec.s_from >= 0.0 && spec.s_to > spec.s_from && spec.s_to <= len) {
                return Err(bad(format!(
                    "needs 0 <= s_from < s_to <= {len} (the ribbon's length)"
                )));
            }
            if !(0.0..=1.0).contains(&spec.occupancy) {
                return Err(bad("occupancy must be between 0 and 1".into()));
            }
            let total: f64 = spec.composition.values().sum();
            if spec.composition.is_empty()
                || spec.composition.values().any(|w| w.is_nan() || *w < 0.0)
                || total.is_nan()
                || total <= 0.0
            {
                return Err(bad(
                    "composition needs non-negative weights, not all zero".into()
                ));
            }
            let mut cum = 0.0;
            let types: Vec<(VehicleType, f64)> = spec
                .composition
                .iter()
                .filter(|(_, w)| **w > 0.0)
                .map(|(t, w)| {
                    cum += w / total;
                    (*t, cum)
                })
                .collect();
            let offset = spec
                .offset_from_kerb
                .clone()
                .or_else(|| p.parking.offset_from_kerb.clone())
                .ok_or_else(|| unset("offset_from_kerb", "road.parking.offset_from_kerb"))?;
            let double = spec
                .double_parking_probability
                .or(p.parking.double_parking_probability)
                .ok_or_else(|| {
                    unset(
                        "double_parking_probability",
                        "road.parking.double_parking_probability",
                    )
                })?;
            let per_reference_s = match spec.manoeuvres_per_100m_h {
                Some(per_h) => per_h / crate::demand::SECONDS_PER_HOUR,
                None => p
                    .parking
                    .manoeuvre_rate
                    .ok_or_else(|| unset("manoeuvres_per_100m_h", "road.parking.manoeuvre_rate"))?,
            };
            let cruising_share = spec
                .cruising_share
                .or(p.parking.cruising_share)
                .ok_or_else(|| unset("cruising_share", "road.parking.cruising_share"))?;
            if !(0.0..=1.0).contains(&double) || !(0.0..=1.0).contains(&cruising_share) {
                return Err(bad(
                    "double_parking_probability and cruising_share must be between 0 and 1".into(),
                ));
            }
            if per_reference_s.is_nan() || per_reference_s < 0.0 {
                return Err(bad("the manoeuvre rate must not be negative".into()));
            }
            let rate = per_reference_s * (spec.s_to - spec.s_from) / RATE_REFERENCE_KERB_M;
            if rate > 0.0 && p.parking.manoeuvre_duration.is_none() {
                return Err(bad(
                    "there are manoeuvres but the register's `road.parking.manoeuvre_duration` \
                     is unset: set it in the scenario's overrides"
                        .into(),
                ));
            }
            if cruising_share > 0.0 {
                parking.cruising_speed = p.parking.cruising_speed.ok_or_else(|| {
                    bad("some vehicles look for a space but the register's \
                         `road.parking.cruising_speed` is unset: set it in the scenario's overrides"
                        .into())
                })?;
                parking.any_cruising = true;
            }
            let mut cruising_class = [false; N_CLASSES];
            for (t, _) in &types {
                cruising_class[t.class().index()] = true;
            }
            let mut zone = Zone {
                id: spec.id.clone(),
                ribbon,
                s_from: spec.s_from,
                s_to: spec.s_to,
                left: spec.side == RoadSide::Left,
                offset,
                types,
                rate,
                duration: p.parking.manoeuvre_duration.clone(),
                cruising_share,
                cruising_class,
                bays: Vec::new(),
                next_serial: 0,
                events: 0,
                next_event_t: f64::INFINITY,
                queue: Vec::new(),
            };
            parking.fill(&mut zone, z, spec.occupancy, double, net, p, objects)?;
            if zone.rate > 0.0 {
                zone.next_event_t = parking.event_rng(z, 0).exponential() / zone.rate;
            }
            parking.zones.push(zone);
        }
        Ok(parking)
    }

    fn event_rng(&self, z: usize, k: u64) -> Rng {
        Rng(StreamRng::at(
            self.seed,
            StreamKind::Objects,
            z as u64,
            EVENT_BASE + k * DRAW_BLOCK,
        ))
    }

    /// Park vehicles along the kerb until `occupancy` of its length is taken.
    #[allow(clippy::too_many_arguments)]
    fn fill(
        &self,
        zone: &mut Zone,
        z: usize,
        occupancy: f64,
        double: f64,
        net: &Network,
        p: &CoreParams,
        objects: &mut Objects,
    ) -> Result<(), String> {
        let kerb = zone.s_to - zone.s_from;
        // The vehicles, in order along the kerb, and the ones beside them.
        let mut drawn = Vec::new();
        let mut taken = 0.0;
        loop {
            let n = drawn.len() as u64;
            let mut rng = StreamRng::at(self.seed, StreamKind::Objects, z as u64, n * DRAW_BLOCK);
            let v = draw_vehicle(&mut rng, &zone.types, &zone.offset, p);
            if taken + v.1 > occupancy * kerb {
                break;
            }
            taken += v.1;
            let beside = (rng.uniform() < double)
                .then(|| draw_vehicle(&mut rng, &zone.types, &zone.offset, p));
            drawn.push((v, beside));
        }
        // The free length is shared out at random between them (a uniform
        // random division: exponential spacings scaled to the free length).
        let mut rng = Rng(StreamRng::at(
            self.seed,
            StreamKind::Objects,
            z as u64,
            SPACING_BASE,
        ));
        let spacing: Vec<f64> = (0..=drawn.len()).map(|_| rng.exponential()).collect();
        let scale = (kerb - taken) / spacing.iter().sum::<f64>();
        let mut at = zone.s_from;
        for (k, ((vtype, length, width, offset), beside)) in drawn.into_iter().enumerate() {
            at += spacing[k] * scale;
            let s = at + 0.5 * length;
            at += length;
            let y = zone.inward(zone.kerb(net, s, length), offset + 0.5 * width);
            let serial = zone.next_serial;
            zone.next_serial += 1;
            let mut bay = Bay {
                serial,
                s,
                y,
                length,
                width,
                vtype,
                state: BayState::Parked,
            };
            if let Some((vtype2, length2, width2, offset2)) = beside {
                bay.state = BayState::BoxedIn;
                let rib_len = net.ribbon(zone.ribbon).length();
                let other = Bay {
                    serial,
                    s: s.clamp(0.5 * length2, rib_len - 0.5 * length2),
                    y: zone.inward(y, 0.5 * width + offset2 + 0.5 * width2),
                    length: length2,
                    width: width2,
                    vtype: vtype2,
                    state: BayState::BoxedIn,
                };
                let spec = zone.object(
                    net,
                    zone.object_id('d', serial),
                    ObjectKind::ParkedVehicle,
                    &other,
                    other.y,
                );
                objects.add(&spec, net, None)?;
            }
            let spec = zone.object(
                net,
                zone.object_id('p', serial),
                ObjectKind::ParkedVehicle,
                &bay,
                bay.y,
            );
            objects.add(&spec, net, None)?;
            zone.bays.push(bay);
        }
        Ok(())
    }

    /// Whether any vehicle may be looking for a space.
    #[inline]
    pub fn any_cruising(&self) -> bool {
        self.any_cruising
    }

    /// The speed cap of a vehicle of `class` at station `s` of ribbon `r`
    /// whose cruising draw is `u`, if it is looking for a space there.
    #[inline]
    pub fn cruising_cap(&self, r: usize, s: f64, class: usize, u: f64) -> Option<f64> {
        self.zones
            .iter()
            .any(|z| {
                z.ribbon == r
                    && s >= z.s_from
                    && s <= z.s_to
                    && z.cruising_class[class]
                    && u < z.cruising_share
            })
            .then_some(self.cruising_speed)
    }

    /// Number of vehicles parked at the kerb of stretch `id` now (not
    /// counting those standing beside a parked vehicle).
    pub fn parked(&self, id: &str) -> Option<usize> {
        self.zones.iter().find(|z| z.id == id).map(|z| {
            z.bays
                .iter()
                .filter(|b| b.state != BayState::Arriving)
                .count()
        })
    }

    /// Advance to time `t`: end the manoeuvres that are over, queue the ones
    /// that fall due, and begin every queued one for which the strip beside
    /// its space is clear. Changes take effect in the step that starts at
    /// `t`. Calls `started` for each manoeuvre that begins.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        t: f64,
        a: &AgentStore,
        orders: &[Vec<u32>],
        net: &Network,
        prev: &[Option<usize>],
        p: &CoreParams,
        objects: &mut Objects,
        mut started: impl FnMut(ManoeuvreStarted),
    ) {
        for z in 0..self.zones.len() {
            if self.zones[z].rate <= 0.0 {
                continue;
            }
            self.finish(z, t, a, orders, net, prev, p, objects);
            while self.zones[z].next_event_t <= t {
                self.fall_due(z, net, p);
            }
            let zone = &mut self.zones[z];
            for m in 0..zone.queue.len() {
                if zone.queue[m].ends.is_some() {
                    continue;
                }
                let bay = zone.bays[zone.bay(zone.queue[m].bay)].clone();
                let y = zone.inward(bay.y, bay.width);
                if !strip_is_clear(zone.ribbon, &bay, y, a, orders, net, prev, p) {
                    continue;
                }
                let man = &mut zone.queue[m];
                man.ends = Some(t + man.duration);
                let (park, duration, waited) = (man.park, man.duration, t - man.due);
                if !park {
                    // The vehicle is out of its space from now on.
                    let _ = objects.remove(&zone.object_id('p', bay.serial), t);
                }
                let id = zone.object_id(if park { 'a' } else { 'l' }, bay.serial);
                let spec = zone.object(net, id.clone(), ObjectKind::ParkingManoeuvre, &bay, y);
                if objects.add(&spec, net, Some(t)).is_ok() {
                    started(ManoeuvreStarted {
                        zone: zone.id.clone(),
                        object: id,
                        ribbon: zone.ribbon,
                        s: bay.s,
                        y,
                        park,
                        duration,
                        waited,
                    });
                }
            }
        }
    }

    /// End the manoeuvres of stretch `z` that are over by `t`. A vehicle
    /// that is parking stays in the strip until its space is clear (traffic
    /// may be using a gap in the parked row).
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        z: usize,
        t: f64,
        a: &AgentStore,
        orders: &[Vec<u32>],
        net: &Network,
        prev: &[Option<usize>],
        p: &CoreParams,
        objects: &mut Objects,
    ) {
        let zone = &mut self.zones[z];
        let mut m = 0;
        while m < zone.queue.len() {
            let man = &zone.queue[m];
            let k = zone.bay(man.bay);
            let over = man.ends.is_some_and(|end| end <= t)
                && (!man.park
                    || strip_is_clear(
                        zone.ribbon,
                        &zone.bays[k],
                        zone.bays[k].y,
                        a,
                        orders,
                        net,
                        prev,
                        p,
                    ));
            if !over {
                m += 1;
                continue;
            }
            let man = zone.queue.remove(m);
            let serial = zone.bays[k].serial;
            let _ = objects.remove(&zone.object_id(if man.park { 'a' } else { 'l' }, serial), t);
            if man.park {
                zone.bays[k].state = BayState::Parked;
                let bay = &zone.bays[k];
                let spec = zone.object(
                    net,
                    zone.object_id('p', serial),
                    ObjectKind::ParkedVehicle,
                    bay,
                    bay.y,
                );
                let _ = objects.add(&spec, net, Some(t));
            } else {
                zone.bays.remove(k);
            }
        }
    }

    /// Queue the next manoeuvre of stretch `z` and draw the time of the one
    /// after it. A vehicle leaves or one arrives with equal probability; if
    /// the kind drawn is not possible (nobody free to leave, no room to
    /// park) the other happens, and if neither is, nothing does.
    fn fall_due(&mut self, z: usize, net: &Network, p: &CoreParams) {
        let k = self.zones[z].events;
        let mut rng = self.event_rng(z, k + 1);
        let zone = &mut self.zones[z];
        let due = zone.next_event_t;
        zone.events += 1;
        zone.next_event_t = due + rng.exponential() / zone.rate;
        let duration = zone
            .duration
            .as_ref()
            .map_or(0.0, |d| positive(d.sample(&mut rng.0), d.mean()));
        let (want_park, pick, place) = (rng.0.uniform() < 0.5, rng.0.uniform(), rng.0.uniform());
        let (vtype, length, width, offset) = draw_vehicle(&mut rng.0, &zone.types, &zone.offset, p);
        let leavers: Vec<u64> = zone
            .bays
            .iter()
            .filter(|b| b.state == BayState::Parked)
            .map(|b| b.serial)
            .collect();
        let rooms: Vec<(f64, f64)> = zone
            .gaps()
            .filter(|(from, to)| to - from >= length)
            .collect();
        let park = match (want_park, leavers.is_empty(), rooms.is_empty()) {
            (true, _, false) | (false, true, false) => true,
            (false, false, _) | (true, false, true) => false,
            _ => return,
        };
        let index = |n: usize| ((pick * n as f64) as usize).min(n - 1);
        let bay = if park {
            let (from, to) = rooms[index(rooms.len())];
            let s = from + place * (to - from - length) + 0.5 * length;
            let serial = zone.next_serial;
            zone.next_serial += 1;
            let bay = Bay {
                serial,
                s,
                y: zone.inward(zone.kerb(net, s, length), offset + 0.5 * width),
                length,
                width,
                vtype,
                state: BayState::Arriving,
            };
            let at = zone.bays.partition_point(|b| b.s < s);
            zone.bays.insert(at, bay);
            serial
        } else {
            let serial = leavers[index(leavers.len())];
            let k = zone.bay(serial);
            zone.bays[k].state = BayState::Leaving;
            serial
        };
        zone.queue.push(Manoeuvre {
            bay,
            park,
            duration,
            due,
            ends: None,
        });
    }
}

/// A stream with the exponential draw the layout and the event times need.
struct Rng(StreamRng);

impl Rng {
    /// A standard exponential variate.
    fn exponential(&mut self) -> f64 {
        -libm::log(self.0.uniform_open())
    }
}

/// Whether a vehicle may move into the strip beside `bay` (centre `y`) now:
/// nobody is on it, and nobody coming up to it would have to brake harder
/// than the safe rate (the safety criterion of a lateral move, ADR-0010 §3,
/// applied to the vehicle that manoeuvres).
#[allow(clippy::too_many_arguments)]
fn strip_is_clear(
    r: usize,
    bay: &Bay,
    y: f64,
    a: &AgentStore,
    orders: &[Vec<u32>],
    net: &Network,
    prev: &[Option<usize>],
    p: &CoreParams,
) -> bool {
    let (rear, front) = (bay.s - 0.5 * bay.length, bay.s + 0.5 * bay.length);
    let class = Some(bay.vtype.class().index());
    let clear_of = |ribbon: usize, offset: f64| -> bool {
        let order = &orders[ribbon];
        let from = order.partition_point(|&i| a.s[i as usize] + offset < rear - p.lookahead);
        for &i in &order[from..] {
            let i = i as usize;
            let (s, half) = (a.s[i] + offset, 0.5 * a.length[i]);
            if s - half >= front {
                if s - p.lookahead > front {
                    break;
                }
                continue;
            }
            let c_min =
                a.clearance_scale[i] * p.object_clearance(usize::from(a.class[i]), class, a.v[i]).1;
            if edge_gap(a.y[i], a.half_width[i], y, 0.5 * bay.width) > c_min {
                continue;
            }
            let gap = rear - (s + half);
            if gap <= 0.0 || a.v[i] * a.v[i] > 2.0 * p.safe_decel * gap {
                return false;
            }
        }
        true
    };
    clear_of(r, 0.0)
        && (rear - p.lookahead >= 0.0
            || prev[r].is_none_or(|q| clear_of(q, -net.ribbon(q).length())))
}
