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

use crate::params::{CoreParams, N_CLASSES};
use crate::rng::{StreamKind, StreamRng};
use marg_net::Network;
use marg_params::Dist;
use marg_params::spec::AttractorSpec;

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
}

impl Stopping {
    pub fn new(
        specs: &[AttractorSpec],
        net: &Network,
        p: &CoreParams,
        seed: u64,
    ) -> Result<Self, String> {
        let mut s = Self {
            seed,
            dwell: p.stopping.dwell.clone(),
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
        Ok(s)
    }

    /// Whether any class stops anywhere.
    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled
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
