//! Pedestrians crossing midblock (G§6.4, phase P6.7).
//!
//! A pedestrian, or a group walking abreast, is a body that moves across the
//! road at walking speed. Vehicles see it as they see any body on the road
//! (ADR-0012): they brake for it and steer round it. The pedestrian decides
//! by gap acceptance, vehicle by vehicle: it steps into a vehicle's path
//! only if that vehicle is at least its accepted gap away in time, and
//! otherwise waits where it is, at the edge or in the middle of the road
//! (a rolling crossing). It never walks into a vehicle that is level with it.

use crate::agents::AgentStore;
use crate::objects::Objects;
use crate::params::CoreParams;
use crate::rng::{StreamKind, StreamRng};
use marg_net::Network;
use marg_params::Dist;
use marg_params::spec::{ObjectKind, ObjectSpec, PedestrianCrossingSpec};

/// Counter blocks of a crossing stretch's stream, per arrival.
const DRAW_BLOCK: u64 = 1 << 16;
/// Stream entities of pedestrian stretches start here (kerbs and pothole
/// fields use the low numbers of the same kind).
const ENTITY_BASE: u64 = 1 << 52;

#[derive(Debug, Clone)]
struct Zone {
    id: String,
    ribbon: usize,
    s_from: f64,
    s_to: f64,
    /// Crossings per second.
    rate: f64,
    from_left: f64,
    arrivals: u64,
    next_t: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct Walker {
    zone: usize,
    serial: u64,
    s: f64,
    /// +1 walks towards larger `y` (from the right edge to the left).
    dir: f64,
    /// Centre of the body across the road.
    y: f64,
    /// Extent along the road and across it.
    length: f64,
    width: f64,
    group: u32,
    accepted_gap: f64,
    t_arrival: f64,
    /// When it stepped onto the carriageway, and its index in the objects.
    entered: Option<(f64, usize)>,
    waited_on_road: f64,
}

/// A crossing that has just ended (for the event log).
#[derive(Debug, Clone, PartialEq)]
pub struct Crossed {
    pub zone: String,
    pub ribbon: usize,
    pub s: f64,
    pub y: f64,
    pub group: u32,
    pub from_left: bool,
    pub accepted_gap: f64,
    pub waited_at_edge: f64,
    pub waited_on_road: f64,
    pub duration: f64,
}

/// Every pedestrian crossing stretch of a scenario and those crossing now.
#[derive(Debug, Clone, Default)]
pub struct Pedestrians {
    zones: Vec<Zone>,
    walkers: Vec<Walker>,
    seed: u64,
    speed: f64,
    size: f64,
    gap: Option<Dist>,
    group: Option<Dist>,
}

impl Pedestrians {
    pub fn new(
        specs: &[PedestrianCrossingSpec],
        net: &Network,
        p: &CoreParams,
        seed: u64,
    ) -> Result<Self, String> {
        let mut peds = Self {
            seed,
            gap: p.road.pedestrian_gap.clone(),
            group: p.road.pedestrian_group.clone(),
            ..Self::default()
        };
        for (z, spec) in specs.iter().enumerate() {
            let bad = |m: String| format!("pedestrian crossing `{}`: {m}", spec.id);
            let unset = |path: &str| {
                bad(format!(
                    "the register's `{path}` is unset: set it in the scenario's overrides \\
                     (the guide has no number for it)"
                ))
            };
            if spec.id.is_empty() || specs[..z].iter().any(|o| o.id == spec.id) {
                return Err(bad("ids must be unique and not empty".into()));
            }
            let ribbon =
                crate::surface::stretch(net, &spec.ribbon, spec.s_from, spec.s_to).map_err(bad)?;
            if spec.per_hour.is_nan()
                || spec.per_hour < 0.0
                || !(0.0..=1.0).contains(&spec.from_left_share)
            {
                return Err(bad(
                    "per_hour must not be negative and from_left_share must be between 0 and 1"
                        .into(),
                ));
            }
            peds.speed = p
                .road
                .pedestrian_speed
                .ok_or_else(|| unset("road.pedestrian.walking_speed"))?;
            peds.size = p
                .road
                .pedestrian_size
                .ok_or_else(|| unset("road.pedestrian.body_size"))?;
            if peds.gap.is_none() {
                return Err(unset("road.pedestrian.accepted_gap"));
            }
            if peds.group.is_none() {
                return Err(unset("road.pedestrian.group_size"));
            }
            let rate = spec.per_hour / crate::demand::SECONDS_PER_HOUR;
            let mut zone = Zone {
                id: spec.id.clone(),
                ribbon,
                s_from: spec.s_from,
                s_to: spec.s_to,
                rate,
                from_left: spec.from_left_share,
                arrivals: 0,
                next_t: f64::INFINITY,
            };
            if rate > 0.0 {
                zone.next_t = -libm::log(peds.rng(z, 0).uniform_open()) / rate;
            }
            peds.zones.push(zone);
        }
        Ok(peds)
    }

    fn rng(&self, z: usize, arrival: u64) -> StreamRng {
        StreamRng::at(
            self.seed,
            StreamKind::Pedestrians,
            ENTITY_BASE + z as u64,
            arrival * DRAW_BLOCK,
        )
    }

    /// Whether there are no crossing stretches.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Pedestrians and groups on the carriageway or waiting at its edge.
    pub fn crossing_now(&self) -> usize {
        self.walkers.len()
    }

    /// Advance from `t` by `dt`: new pedestrians arrive at the edge, and
    /// each one walks on, waits, or finishes. Calls `crossed` for each
    /// crossing that ends.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        t: f64,
        dt: f64,
        a: &AgentStore,
        orders: &[Vec<u32>],
        net: &Network,
        p: &CoreParams,
        objects: &mut Objects,
        mut crossed: impl FnMut(Crossed),
    ) {
        for z in 0..self.zones.len() {
            while self.zones[z].next_t <= t {
                let k = self.zones[z].arrivals;
                let mut rng = self.rng(z, k + 1);
                let zone = &mut self.zones[z];
                let t_arrival = zone.next_t;
                zone.arrivals += 1;
                zone.next_t = t_arrival - libm::log(rng.uniform_open()) / zone.rate;
                let s = zone.s_from + rng.uniform() * (zone.s_to - zone.s_from);
                let from_left = rng.uniform() < zone.from_left;
                let sample = |d: &Option<Dist>, rng: &mut StreamRng| {
                    let d = d.as_ref().expect("checked when the stretch was read");
                    let x = d.sample(rng);
                    if x > 0.0 { x } else { d.mean() }
                };
                let group = sample(&self.group, &mut rng).round().max(1.0) as u32;
                let accepted_gap = sample(&self.gap, &mut rng);
                let (lo, hi) = net.ribbon(zone.ribbon).y_bounds(s);
                let length = (f64::from(group) * self.size)
                    .min(2.0 * s)
                    .min(2.0 * (net.ribbon(zone.ribbon).length() - s));
                self.walkers.push(Walker {
                    zone: z,
                    serial: k,
                    s,
                    dir: if from_left { -1.0 } else { 1.0 },
                    // Standing just off the carriageway.
                    y: if from_left {
                        hi + 0.5 * self.size
                    } else {
                        lo - 0.5 * self.size
                    },
                    length,
                    width: self.size,
                    group,
                    accepted_gap,
                    t_arrival,
                    entered: None,
                    waited_on_road: 0.0,
                });
            }
        }
        let step = self.speed * dt;
        let mut k = 0;
        while k < self.walkers.len() {
            let w = &self.walkers[k];
            let zone = &self.zones[w.zone];
            let (lo, hi) = net.ribbon(zone.ribbon).y_bounds(w.s);
            let y_new = w.y + w.dir * step;
            // Across the far edge: done.
            if (w.dir > 0.0 && y_new - 0.5 * w.width >= hi)
                || (w.dir < 0.0 && y_new + 0.5 * w.width <= lo)
            {
                let w = self.walkers.remove(k);
                if let Some((t_in, index)) = w.entered {
                    let id = objects.all[index].id.clone();
                    let _ = objects.remove(&id, t);
                    crossed(Crossed {
                        zone: self.zones[w.zone].id.clone(),
                        ribbon: self.zones[w.zone].ribbon,
                        s: w.s,
                        y: w.y,
                        group: w.group,
                        from_left: w.dir < 0.0,
                        accepted_gap: w.accepted_gap,
                        waited_at_edge: t_in - w.t_arrival,
                        waited_on_road: w.waited_on_road,
                        duration: t - t_in,
                    });
                }
                continue;
            }
            let may = may_walk(w, y_new, lo, hi, zone.ribbon, a, orders, p);
            let w = &mut self.walkers[k];
            if may {
                w.y = y_new;
                match w.entered {
                    Some((_, index)) => objects.all[index].y = y_new,
                    None => {
                        // On the carriageway from now on (the part of the
                        // body still off it is cut at the edge).
                        let spec = ObjectSpec {
                            id: format!("{}/{}", self.zones[w.zone].id, w.serial),
                            kind: ObjectKind::Pedestrians,
                            ribbon: net.ribbon(self.zones[w.zone].ribbon).id.clone(),
                            s: w.s,
                            y: y_new,
                            length: w.length,
                            width: w.width,
                            clearance_as: None,
                            response: Default::default(),
                            present: Vec::new(),
                        };
                        if objects.add(&spec, net, Some(t)).is_ok() {
                            w.entered = Some((t, objects.all.len() - 1));
                        }
                    }
                }
            } else if w.entered.is_some() {
                w.waited_on_road += dt;
            }
            k += 1;
        }
    }
}

/// Whether walker `w` may move its centre to `y_new` this step: no vehicle
/// level with it is in the way, and every vehicle whose path it would step
/// into is at least its accepted gap away in time.
#[allow(clippy::too_many_arguments)]
fn may_walk(
    w: &Walker,
    y_new: f64,
    lo: f64,
    hi: f64,
    ribbon: usize,
    a: &AgentStore,
    orders: &[Vec<u32>],
    p: &CoreParams,
) -> bool {
    let half = 0.5 * w.width;
    // The body now and after the step, as far as it is on the carriageway.
    let now = ((w.y - half).max(lo), (w.y + half).min(hi));
    let next = ((y_new - half).max(lo), (y_new + half).min(hi));
    if next.1 <= next.0 {
        return true;
    }
    let (rear, front) = (w.s - 0.5 * w.length, w.s + 0.5 * w.length);
    let order = &orders[ribbon];
    let from = order.partition_point(|&i| a.s[i as usize] < rear - p.lookahead);
    for &i in &order[from..] {
        let i = i as usize;
        let (v_lo, v_hi) = (a.y[i] - a.half_width[i], a.y[i] + a.half_width[i]);
        if a.s[i] - p.lookahead > front {
            break;
        }
        if v_hi <= next.0 || v_lo >= next.1 {
            continue;
        }
        let (v_rear, v_front) = (a.s[i] - 0.5 * a.length[i], a.s[i] + 0.5 * a.length[i]);
        if v_rear >= front {
            continue;
        }
        if v_front > rear {
            // Level with it and in the way.
            return false;
        }
        // Coming up to it. Already in this vehicle's path: keep going.
        let in_path = now.1 > now.0 && v_hi > now.0 && v_lo < now.1;
        if !in_path && a.v[i] * w.accepted_gap > rear - v_front {
            return false;
        }
    }
    true
}
