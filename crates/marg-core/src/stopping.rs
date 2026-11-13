//! Stop-anywhere events (G§6.2, ADR-0014, phase P6.3): autos, e-rickshaws,
//! cabs and bike taxis that stop to pick up or set down.
//!
//! Each vehicle of a class with a rate draws the distance to its next stop
//! from an exponential (stops per vehicle-kilometre); inside an attractor
//! the distance counts for more. At the event the vehicle either makes for
//! the kerb or holds its line (a probability by class), brakes to a stand at
//! the distance its following model would keep from a standing obstacle,
//! dwells, and drives on. It stays an agent throughout: others pass it as
//! they pass any standing vehicle (ADR-0012).

use crate::objects::Objects;
use crate::params::{CoreParams, N_CLASSES};
use crate::rng::{StreamKind, StreamRng};
use marg_net::Network;
use marg_params::Dist;
use marg_params::spec::{NetworkSpec, ObjectKind, ObjectSpec};

/// Counter blocks of an agent's decision stream used here: three per event.
const EVENT_BLOCK: u64 = 1 << 20;

#[derive(Debug, Clone, PartialEq)]
struct Attractor {
    ribbon: usize,
    s_from: f64,
    s_to: f64,
    multiplier: f64,
}

/// What a stop-anywhere event decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StopDecision {
    pub pull_over: bool,
    /// The probability the choice was drawn against.
    pub pull_over_probability: f64,
    pub dwell: f64,
}

/// The stop-anywhere mechanism of a scenario.
#[derive(Debug, Clone, Default)]
pub struct Stopping {
    enabled: bool,
    seed: u64,
    /// Stops per metre by class (0: the class never stops).
    rate: [f64; N_CLASSES],
    pull_over: [f64; N_CLASSES],
    dwell: Option<Dist>,
    attractors: Vec<Attractor>,
    bus_stops: Vec<BusStop>,
    bus_class: usize,
    bus_dwell: Option<Dist>,
    bus_longitudinal: Option<Dist>,
    bus_lateral: Option<Dist>,
}

/// A kerbside bus stop (G§6.3).
#[derive(Debug, Clone, PartialEq)]
struct BusStop {
    id: String,
    ribbon: usize,
    s: f64,
    share: f64,
    /// Width of carriageway taken by waiting passengers (m).
    passengers: f64,
}

/// Where and for how long a bus stands at a stop.
#[derive(Debug, Clone, PartialEq)]
pub struct BusCall<'a> {
    pub stop: u32,
    pub id: &'a str,
    /// Station at which the bus aims to stand.
    pub s: f64,
    /// Gap it aims to leave between itself and the kerb (m).
    pub offset: f64,
    pub dwell: f64,
}

/// Where a bus's stop decisions start in its decision stream.
const BUS_BASE: u64 = 1 << 51;

impl Stopping {
    pub fn new(
        network: &NetworkSpec,
        net: &Network,
        p: &CoreParams,
        seed: u64,
        objects: &mut Objects,
    ) -> Result<Self, String> {
        let specs = &network.attractors;
        let mut s = Self {
            seed,
            dwell: p.stopping.dwell.clone(),
            bus_class: marg_params::VehicleClass::ALL
                .iter()
                .position(|c| c.key() == "bus")
                .unwrap_or(usize::MAX),
            bus_dwell: p.road.bus_dwell.clone(),
            bus_longitudinal: p.road.bus_offset_longitudinal.clone(),
            bus_lateral: p.road.bus_offset_lateral.clone(),
            ..Self::default()
        };
        for (c, class) in marg_params::VehicleClass::ALL.iter().enumerate() {
            let rate = p.stopping.rate[c].unwrap_or(0.0);
            if rate.is_nan() || rate < 0.0 {
                return Err(format!("road.stop_anywhere.rate.{}: negative", class.key()));
            }
            if rate == 0.0 {
                continue;
            }
            let unset = |what: &str| {
                format!(
                    "{} stop anywhere (`road.stop_anywhere.rate.{}` is set) but `{what}` is \
                     unset: set it in the scenario's overrides (the guide has no number for it)",
                    class.key(),
                    class.key()
                )
            };
            s.rate[c] = rate;
            s.pull_over[c] = p.stopping.pull_over[c].ok_or_else(|| {
                unset(&format!(
                    "road.stop_anywhere.pull_over_probability.{}",
                    class.key()
                ))
            })?;
            if s.dwell.is_none() {
                return Err(unset("road.stop_anywhere.dwell"));
            }
            s.enabled = true;
        }
        for (k, spec) in specs.iter().enumerate() {
            let bad = |m: String| format!("attractor `{}`: {m}", spec.id);
            if spec.id.is_empty() || specs[..k].iter().any(|o| o.id == spec.id) {
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
            if spec.stop_rate_multiplier.is_nan() || spec.stop_rate_multiplier < 0.0 {
                return Err(bad("stop_rate_multiplier must not be negative".into()));
            }
            if !s.enabled {
                return Err(bad(
                    "no class has a stop-anywhere rate (`road.stop_anywhere.rate.*` are unset): \
                     an attractor would do nothing"
                        .into(),
                ));
            }
            s.attractors.push(Attractor {
                ribbon,
                s_from: spec.s_from,
                s_to: spec.s_to,
                multiplier: spec.stop_rate_multiplier,
            });
        }
        for (k, spec) in network.bus_stops.iter().enumerate() {
            let bad = |m: String| format!("bus stop `{}`: {m}", spec.id);
            let unset = |path: &str| {
                bad(format!(
                    "the register's `{path}` is unset: set it in the scenario's overrides"
                ))
            };
            if spec.id.is_empty() || network.bus_stops[..k].iter().any(|o| o.id == spec.id) {
                return Err(bad("ids must be unique and not empty".into()));
            }
            let (from, to) = (spec.s - 0.5 * spec.length, spec.s + 0.5 * spec.length);
            let ribbon = crate::surface::stretch(net, &spec.ribbon, from, to).map_err(bad)?;
            if !(0.0..=1.0).contains(&spec.serving_share) {
                return Err(bad("serving_share must be between 0 and 1".into()));
            }
            if s.bus_dwell.is_none() {
                return Err(unset("road.bus.dwell_time"));
            }
            if s.bus_longitudinal.is_none() {
                return Err(unset("road.bus.stop_offset_longitudinal"));
            }
            if s.bus_lateral.is_none() {
                return Err(unset("road.bus.stop_offset_lateral"));
            }
            let passengers = match spec.waiting_passenger_width {
                Some(w) => w,
                None => p
                    .road
                    .waiting_passenger_width
                    .ok_or_else(|| unset("road.bus.waiting_passenger_width"))?,
            };
            if passengers.is_nan() || passengers < 0.0 {
                return Err(bad("waiting_passenger_width must not be negative".into()));
            }
            if passengers > 0.0 {
                // Passengers waiting on the road: a strip along the platform.
                let kerb = net.ribbon(ribbon).y_bounds(spec.s).1;
                objects.add(
                    &ObjectSpec {
                        id: format!("{}/passengers", spec.id),
                        kind: ObjectKind::WaitingPassengers,
                        ribbon: spec.ribbon.clone(),
                        s: spec.s,
                        y: kerb - 0.5 * passengers,
                        length: spec.length,
                        width: passengers,
                        clearance_as: None,
                        response: Default::default(),
                        present: Vec::new(),
                    },
                    net,
                    None,
                )?;
            }
            if let Some(multiplier) = spec.stop_rate_multiplier {
                if !s.enabled || multiplier.is_nan() || multiplier < 0.0 {
                    return Err(bad("stop_rate_multiplier needs a stop-anywhere rate \
                         (`road.stop_anywhere.rate.*`) and must not be negative"
                        .into()));
                }
                s.attractors.push(Attractor {
                    ribbon,
                    s_from: from,
                    s_to: to,
                    multiplier,
                });
            }
            s.bus_stops.push(BusStop {
                id: spec.id.clone(),
                ribbon,
                s: spec.s,
                share: spec.serving_share,
                passengers,
            });
        }
        Ok(s)
    }

    /// Whether any vehicle may stop: anywhere, or a bus at a bus stop.
    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled || !self.bus_stops.is_empty()
    }

    /// Whether a vehicle of `class` may have a bus stop to serve.
    #[inline]
    pub fn serves_bus_stops(&self, class: usize) -> bool {
        class == self.bus_class && !self.bus_stops.is_empty()
    }

    /// The next stop that bus `id` at station `s` of ribbon `r` serves,
    /// if it is within `reach` m: where it aims to stand and for how long.
    /// `last` is the stop it served last.
    pub fn bus_call(
        &self,
        id: u64,
        r: usize,
        s: f64,
        reach: f64,
        last: u32,
    ) -> Option<BusCall<'_>> {
        let mut best: Option<BusCall> = None;
        for (k, stop) in self.bus_stops.iter().enumerate() {
            if stop.ribbon != r || k as u32 == last {
                continue;
            }
            let rng = |draw: u64| {
                StreamRng::at(
                    self.seed,
                    StreamKind::Decisions,
                    id,
                    BUS_BASE + (4 * k as u64 + draw) * EVENT_BLOCK,
                )
            };
            if rng(0).uniform() >= stop.share {
                continue;
            }
            let sample =
                |d: &Option<Dist>, draw: u64| d.as_ref().map_or(0.0, |d| d.sample(&mut rng(draw)));
            let at = stop.s + sample(&self.bus_longitudinal, 1);
            if at <= s || at - s > reach || best.as_ref().is_some_and(|b| b.s <= at) {
                continue;
            }
            let dwell = sample(&self.bus_dwell, 3);
            best = Some(BusCall {
                stop: k as u32,
                id: &stop.id,
                s: at,
                offset: stop.passengers + sample(&self.bus_lateral, 2).max(0.0),
                dwell: if dwell > 0.0 {
                    dwell
                } else {
                    self.bus_dwell.as_ref().map_or(0.0, Dist::mean)
                },
            });
        }
        best
    }

    /// Stops per metre of a class outside attractors.
    #[inline]
    pub fn rate(&self, class: usize) -> f64 {
        self.rate[class]
    }

    /// Factor on the stop rate at station `s` of ribbon `r`.
    #[inline]
    pub fn multiplier(&self, r: usize, s: f64) -> f64 {
        self.attractors
            .iter()
            .filter(|a| a.ribbon == r && s >= a.s_from && s < a.s_to)
            .map(|a| a.multiplier)
            .product()
    }

    fn rng(&self, id: u64, event: u32, draw: u64) -> StreamRng {
        StreamRng::at(
            self.seed,
            StreamKind::Decisions,
            id,
            (3 * u64::from(event) + draw) * EVENT_BLOCK,
        )
    }

    /// Travel (m, at multiplier 1) until event number `event` of agent `id`;
    /// infinite for a class that never stops.
    pub fn exposure(&self, id: u64, class: usize, event: u32) -> f64 {
        if self.rate[class] > 0.0 {
            -libm::log(self.rng(id, event, 0).uniform_open()) / self.rate[class]
        } else {
            f64::INFINITY
        }
    }

    /// What agent `id` does at its event number `event`.
    pub fn decide(&self, id: u64, class: usize, event: u32) -> StopDecision {
        let p = self.pull_over[class];
        let dwell = self.dwell.as_ref().map_or(0.0, |d| {
            let x = d.sample(&mut self.rng(id, event, 2));
            if x > 0.0 { x } else { d.mean() }
        });
        StopDecision {
            pull_over: self.rng(id, event, 1).uniform() < p,
            pull_over_probability: p,
            dwell,
        }
    }
}
