//! The road surface (G§6.1, G§6.8–6.10; phases P6.5 and P6.6): potholes,
//! humps, standing water, gradients and roughness.
//!
//! None of these is a body. A pothole, a hump and shallow water are things
//! a vehicle drives over at a lower speed, or steers round if it can; a
//! gradient and a rough surface lower desired speeds by class. They act
//! through speed caps and incentives only (ADR-0005).

use crate::objects::{Objects, RoadObject};
use crate::params::{CoreParams, N_CLASSES, POTHOLE_GROUPS, pothole_group};
use crate::rng::{StreamKind, StreamRng};
use marg_net::Network;
use marg_params::VehicleClass;
use marg_params::spec::{NetworkSpec, ObjectKind, ObjectSpec};

/// Standard gravity (m/s²).
const STANDARD_GRAVITY: f64 = 9.806_65;
/// Percent to a fraction.
const PERCENT: f64 = 0.01;
/// Where an agent's pothole decisions start in its decision stream (the
/// stop-anywhere draws are below this).
const POTHOLE_BASE: u64 = 1 << 50;
/// Counter blocks of a pothole field's stream, per pothole.
const DRAW_BLOCK: u64 = 1 << 16;

/// What something on the surface means to one vehicle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    /// Nothing (it straddles a pothole).
    Ignore,
    /// It comes down to `speed` for it, as for a leader moving at that
    /// speed. With `avoid`, lines that miss it are worth more to it.
    Slow { speed: f64, avoid: bool },
    /// It does not respond until it is on it; there `speed` caps it.
    Over { speed: f64 },
}

/// A stretch that lowers desired speeds.
#[derive(Debug, Clone, PartialEq)]
struct SpeedZone {
    ribbon: usize,
    s_from: f64,
    s_to: f64,
    loss: [f64; N_CLASSES],
    cap: [f64; N_CLASSES],
}

/// How vehicles respond to the surface.
#[derive(Debug, Clone, Default)]
pub struct Surface {
    seed: u64,
    detection: f64,
    link: f64,
    /// By pothole group: probability of swerving, slowing, straddling.
    response: [[f64; 3]; 3],
    crossing: [f64; 3],
    group: [usize; N_CLASSES],
    zones: Vec<SpeedZone>,
    potholes: bool,
}

/// The lateral extent `[from, to]` given, or the carriageway's.
fn extent(
    net: &Network,
    ribbon: usize,
    s: f64,
    from: Option<f64>,
    to: Option<f64>,
) -> Result<(f64, f64), String> {
    let (lo, hi) = net.ribbon(ribbon).y_bounds(s);
    let (from, to) = (from.unwrap_or(lo).max(lo), to.unwrap_or(hi).min(hi));
    if to > from {
        Ok((from, to))
    } else {
        Err("its lateral extent is empty or off the carriageway".into())
    }
}

/// Check a stretch `[s_from, s_to]` of a ribbon; returns the ribbon's index.
pub fn stretch(net: &Network, ribbon: &str, s_from: f64, s_to: f64) -> Result<usize, String> {
    let r = net
        .index(ribbon)
        .map_err(|_| format!("unknown ribbon `{ribbon}`"))?;
    let len = net.ribbon(r).length();
    if s_from >= 0.0 && s_to > s_from && s_to <= len {
        Ok(r)
    } else {
        Err(format!(
            "needs 0 <= s_from < s_to <= {len} (the ribbon's length)"
        ))
    }
}

impl Surface {
    /// Read the scenario's surface items; put potholes, humps and water on
    /// the road as objects.
    pub fn new(
        spec: &NetworkSpec,
        net: &Network,
        p: &CoreParams,
        seed: u64,
        objects: &mut Objects,
    ) -> Result<Self, String> {
        let mut s = Self {
            seed,
            ..Self::default()
        };
        for (c, class) in VehicleClass::ALL.iter().enumerate() {
            s.group[c] = pothole_group(*class);
        }
        let unset = |path: &str| {
            format!("the register's `{path}` is unset: set it in the scenario's overrides")
        };
        // Water first, so that the potholes under it are known to be hidden.
        let mut water: Vec<(usize, f64, f64, f64, f64)> = Vec::new();
        for w in &spec.waterlogging {
            let bad = |m: String| format!("waterlogging `{}`: {m}", w.id);
            let r = stretch(net, &w.ribbon, w.s_from, w.s_to).map_err(bad)?;
            let (mid, len) = (0.5 * (w.s_from + w.s_to), w.s_to - w.s_from);
            let (lo, hi) = extent(net, r, mid, w.y_from, w.y_to).map_err(bad)?;
            water.push((r, w.s_from, w.s_to, lo, hi));
            match (w.closed, w.speed_cap_kmh) {
                (true, _) => objects.add(
                    &ObjectSpec {
                        id: w.id.clone(),
                        kind: ObjectKind::Obstacle,
                        ribbon: w.ribbon.clone(),
                        s: mid,
                        y: 0.5 * (lo + hi),
                        length: len,
                        width: hi - lo,
                        clearance_as: None,
                        response: Default::default(),
                        present: Vec::new(),
                    },
                    net,
                    None,
                )?,
                (false, Some(kmh)) if kmh > 0.0 => {
                    let k = objects.add_surface(
                        w.id.clone(),
                        ObjectKind::Water,
                        r,
                        mid,
                        0.5 * (lo + hi),
                        len,
                        hi - lo,
                    );
                    objects.all[k].speed = kmh / crate::demand::SECONDS_PER_HOUR * METRES_PER_KM;
                }
                _ => return Err(bad("needs a positive speed_cap_kmh, or closed: true".into())),
            }
        }
        for h in &spec.humps {
            let bad = |m: String| format!("hump `{}`: {m}", h.id);
            let (from, to) = (h.s - 0.5 * h.length, h.s + 0.5 * h.length);
            let r = stretch(net, &h.ribbon, from, to).map_err(bad)?;
            let (lo, hi) = extent(net, r, h.s, h.y_from, h.y_to).map_err(bad)?;
            if p.road.hump_speed.is_none() {
                return Err(bad(unset("road.hump.crossing_speed")));
            }
            objects.add_surface(
                h.id.clone(),
                ObjectKind::Hump,
                r,
                h.s,
                0.5 * (lo + hi),
                h.length,
                hi - lo,
            );
        }
        for (f, field) in spec.pothole_fields.iter().enumerate() {
            let bad = |m: String| format!("pothole field `{}`: {m}", field.id);
            let r = stretch(net, &field.ribbon, field.s_from, field.s_to).map_err(bad)?;
            if !(0.0..=1.0).contains(&field.kerb_bias) {
                return Err(bad("kerb_bias must be between 0 and 1".into()));
            }
            let density = match field.per_km {
                Some(x) => x / METRES_PER_KM,
                None => p
                    .road
                    .pothole_density
                    .ok_or_else(|| bad(unset("road.pothole.density") + ", or give per_km"))?,
            };
            if density.is_nan() || density < 0.0 {
                return Err(bad("the density must not be negative".into()));
            }
            let need = |x: Option<f64>, path: String| x.ok_or_else(|| bad(unset(&path)));
            s.detection = need(
                p.road.pothole_detection,
                "road.pothole.detection_distance".into(),
            )?;
            s.link = need(
                p.road.pothole_link,
                "road.pothole.aggressiveness_link".into(),
            )?;
            for (g, name) in POTHOLE_GROUPS.iter().enumerate() {
                for (k, what) in ["swerve", "slow", "straddle"].iter().enumerate() {
                    s.response[g][k] = need(
                        p.road.pothole_response[g][k],
                        format!("road.pothole.response.{name}.{what}"),
                    )?;
                }
                if s.response[g].iter().sum::<f64>() > 1.0 + 1e-9 {
                    return Err(bad(format!(
                        "road.pothole.response.{name}.*: the probabilities add up to more than 1"
                    )));
                }
                s.crossing[g] = need(
                    p.road.pothole_speed[g],
                    format!("road.pothole.crossing_speed.{name}"),
                )?;
            }
            let (Some(diameter), Some(depth)) = (&p.road.pothole_diameter, &p.road.pothole_depth)
            else {
                return Err(bad(unset("road.pothole.diameter` or `road.pothole.depth")));
            };
            s.potholes = true;
            // A Poisson process along the stretch.
            let (mut at, mut n) = (field.s_from, 0u64);
            if density > 0.0 {
                loop {
                    let mut rng = StreamRng::at(
                        seed,
                        StreamKind::Objects,
                        POTHOLE_BASE + f as u64,
                        n * DRAW_BLOCK,
                    );
                    at += -libm::log(rng.uniform_open()) / density;
                    if at >= field.s_to {
                        break;
                    }
                    let d = diameter.sample(&mut rng);
                    let d = if d > 0.0 { d } else { diameter.mean() };
                    // Across the road: density rising linearly towards the kerb.
                    let (q, b) = (rng.uniform(), field.kerb_bias);
                    let u = if b > 0.0 {
                        (-(1.0 - b) + ((1.0 - b) * (1.0 - b) + 4.0 * b * q).sqrt()) / (2.0 * b)
                    } else {
                        q
                    };
                    let (lo, hi) = net.ribbon(r).y_bounds(at);
                    let len = net.ribbon(r).length();
                    if hi - lo > d && at - 0.5 * d >= 0.0 && at + 0.5 * d <= len {
                        let y = lo + 0.5 * d + u * (hi - lo - d);
                        let k = objects.add_surface(
                            format!("{}/{n}", field.id),
                            ObjectKind::Pothole,
                            r,
                            at,
                            y,
                            d,
                            d,
                        );
                        objects.all[k].depth = depth.sample(&mut rng).max(0.0);
                        objects.all[k].hidden = water.iter().any(|&(wr, s0, s1, y0, y1)| {
                            wr == r && at >= s0 && at <= s1 && y >= y0 && y <= y1
                        });
                    }
                    n += 1;
                }
            }
        }
        for z in &spec.surfaces {
            let bad = |m: String| format!("surface `{}`: {m}", z.id);
            let ribbon = stretch(net, &z.ribbon, z.s_from, z.s_to).map_err(bad)?;
            let mut zone = SpeedZone {
                ribbon,
                s_from: z.s_from,
                s_to: z.s_to,
                loss: [0.0; N_CLASSES],
                cap: [f64::INFINITY; N_CLASSES],
            };
            let grade = z.gradient_percent.unwrap_or(0.0);
            if grade > 0.0 {
                for (c, class) in VehicleClass::ALL.iter().enumerate() {
                    match (p.road.gradient_loss[c], class.key()) {
                        (Some(per_percent), _) => zone.loss[c] += per_percent * grade,
                        // Not measured: the climbing speed its power allows.
                        (None, "e_rickshaw") => {
                            let power = p
                                .road
                                .e_rickshaw_power
                                .ok_or_else(|| bad(unset("vehicles.e_rickshaw.max_power")))?;
                            let mass = p
                                .road
                                .e_rickshaw_mass
                                .ok_or_else(|| bad(unset("vehicles.e_rickshaw.loaded_mass")))?;
                            zone.cap[c] = power / (mass * STANDARD_GRAVITY * grade * PERCENT);
                        }
                        (None, key) => {
                            return Err(bad(unset(&format!("vehicles.gradient_speed_loss.{key}"))));
                        }
                    }
                }
            }
            let iri = z.roughness_iri.unwrap_or(0.0);
            if iri > 0.0 {
                let car = p
                    .road
                    .roughness_loss_car
                    .ok_or_else(|| bad(unset("road.roughness.speed_loss_per_iri.car")))?;
                let heavy = p
                    .road
                    .roughness_loss_heavy
                    .ok_or_else(|| bad(unset("road.roughness.speed_loss_per_iri.heavy")))?;
                for c in 0..N_CLASSES {
                    zone.loss[c] += iri * [0.0, car, heavy][s.group[c]];
                }
            }
            s.zones.push(zone);
        }
        Ok(s)
    }

    /// Whether the scenario has pothole fields.
    #[inline]
    pub fn any_potholes(&self) -> bool {
        self.potholes
    }

    /// Whether any stretch lowers desired speeds.
    #[inline]
    pub fn any_zones(&self) -> bool {
        !self.zones.is_empty()
    }

    /// The desired speed `v0` of a vehicle of `class` at station `s` of
    /// ribbon `r`, after gradient and roughness, not below `floor`.
    #[inline]
    pub fn desired_speed(&self, r: usize, s: f64, class: usize, v0: f64, floor: f64) -> f64 {
        let mut v = v0;
        for z in &self.zones {
            if z.ribbon == r && s >= z.s_from && s < z.s_to {
                v = (v - z.loss[class]).min(z.cap[class]);
            }
        }
        v.max(floor.min(v0))
    }

    /// The speed over a pothole of a vehicle of `class`.
    #[inline]
    pub fn crossing_speed(&self, class: usize) -> f64 {
        self.crossing[self.group[class]]
    }

    /// What agent `id` chooses on seeing pothole `serial`: 0 swerve, 1 slow,
    /// 2 straddle, 3 hit. The same for every step and thread: it is a
    /// function of the seed, the agent and the pothole.
    pub fn pothole_choice(&self, id: u64, class: usize, kappa: f64, serial: u64) -> usize {
        let p = &self.response[self.group[class]];
        let hit = (1.0 - p[0] - p[1] - p[2]).max(0.0);
        // More aggressive: slows less, hits more.
        let w = [
            p[0],
            p[1] * libm::exp(-self.link * kappa),
            p[2],
            hit * libm::exp(self.link * kappa),
        ];
        let total: f64 = w.iter().sum();
        let u = StreamRng::at(self.seed, StreamKind::Decisions, id, POTHOLE_BASE + serial)
            .uniform()
            * total;
        let mut cum = 0.0;
        for (k, x) in w.iter().enumerate() {
            cum += x;
            if u < cum {
                return k;
            }
        }
        3
    }

    /// What surface object `o` means to an agent whose front is `gap_s`
    /// from it (negative or zero: on it) and that can see the road as far
    /// as `seen_to` ahead (to the rear of the vehicle in front of it).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn role(
        &self,
        o: &RoadObject,
        id: u64,
        class: usize,
        kappa: f64,
        hump_speed: f64,
        gap_s: f64,
        seen_to: f64,
    ) -> Role {
        match o.kind {
            ObjectKind::Hump => Role::Slow {
                speed: hump_speed,
                avoid: true,
            },
            ObjectKind::Water => Role::Slow {
                speed: o.speed,
                avoid: true,
            },
            ObjectKind::Pothole => {
                let speed = self.crossing_speed(class);
                let seen = !o.hidden && gap_s <= self.detection && gap_s < seen_to;
                if !seen {
                    return Role::Over { speed };
                }
                match self.pothole_choice(id, class, kappa, o.serial) {
                    0 => Role::Slow { speed, avoid: true },
                    1 => Role::Slow {
                        speed,
                        avoid: false,
                    },
                    2 => Role::Ignore,
                    _ => Role::Over { speed },
                }
            }
            _ => Role::Ignore,
        }
    }
}

/// Metres per kilometre.
const METRES_PER_KM: f64 = 1000.0;
