//! The gridlock indicator (plan/04-data-outputs-analytics.md §3, G§8.4).
//!
//! Every `area` detector is a monitored area. At each step its space-mean
//! speed is the mean speed of the vehicles whose centre is inside it (Edie's
//! definition over one step); an empty area is not slow. An area is
//! gridlocked once it has been slower than `observers.gridlock.speed` for
//! `observers.gridlock.duration` without a break, and stays so until it is
//! no longer slow. The indicator reports, over the measured window, the
//! share of time during which some area was gridlocked, and the start of
//! the first slow spell that became a gridlock (the onset).
//!
//! Plan 04 also asks for spillback into an upstream junction box. Corridor
//! scenarios have no box upstream of their areas, so that condition is not
//! applied: an area is placed where a queue would block what feeds it.
//!
//! It observes; it changes nothing.

use crate::agents::AgentStore;
use marg_params::spec::DetectorSpec;

#[derive(Debug, Clone)]
struct Area {
    id: String,
    ribbon: usize,
    s_from: f64,
    s_to: f64,
    /// Start of the current slow spell (NaN: not slow).
    slow_since: f64,
    locked: bool,
}

/// The onset of a gridlock in one area.
#[derive(Debug, Clone, Copy)]
pub struct Onset<'a> {
    pub area: &'a str,
    pub ribbon: usize,
    pub s_from: f64,
    /// When its slow spell began (s).
    pub since: f64,
}

/// All monitored areas and what they have shown.
#[derive(Debug, Clone, Default)]
pub struct Gridlock {
    areas: Vec<Area>,
    speed: f64,
    duration: f64,
    warmup: f64,
    /// Measured time during which some area was gridlocked (s).
    locked_time: f64,
    /// Start of the first slow spell that became a gridlock (NaN: none).
    onset: f64,
}

impl Gridlock {
    pub fn new(
        detectors: &[DetectorSpec],
        ribbon_of: impl Fn(&str) -> Option<usize>,
        speed: f64,
        duration: f64,
        warmup: f64,
    ) -> Self {
        let areas = detectors
            .iter()
            .filter_map(|d| match d {
                DetectorSpec::Area {
                    id,
                    ribbon,
                    s_from,
                    s_to,
                    ..
                } => ribbon_of(ribbon).map(|r| Area {
                    id: id.clone(),
                    ribbon: r,
                    s_from: *s_from,
                    s_to: *s_to,
                    slow_since: f64::NAN,
                    locked: false,
                }),
                DetectorSpec::Screenline { .. } => None,
            })
            .collect();
        Self {
            areas,
            speed,
            duration,
            warmup,
            locked_time: 0.0,
            onset: f64::NAN,
        }
    }

    /// Observe the step `[t0, t0 + dt)`; `on_onset` is called for each area
    /// that becomes gridlocked.
    pub fn observe(
        &mut self,
        a: &AgentStore,
        orders: &[Vec<u32>],
        t0: f64,
        dt: f64,
        mut on_onset: impl FnMut(Onset<'_>),
    ) {
        if self.areas.is_empty() {
            return;
        }
        let mut any = false;
        for area in &mut self.areas {
            let order = &orders[area.ribbon];
            let from = order.partition_point(|&k| a.s[k as usize] < area.s_from);
            let (mut n, mut sum) = (0usize, 0.0);
            for &k in &order[from..] {
                let k = k as usize;
                if a.s[k] >= area.s_to {
                    break;
                }
                n += 1;
                sum += a.v[k];
            }
            let slow = n > 0 && sum < self.speed * n as f64;
            if !slow {
                area.slow_since = f64::NAN;
                area.locked = false;
                continue;
            }
            if area.slow_since.is_nan() {
                area.slow_since = t0;
            }
            if !area.locked && t0 + dt - area.slow_since >= self.duration {
                area.locked = true;
                if self.onset.is_nan() {
                    self.onset = area.slow_since;
                }
                on_onset(Onset {
                    area: &area.id,
                    ribbon: area.ribbon,
                    s_from: area.s_from,
                    since: area.slow_since,
                });
            }
            any |= area.locked;
        }
        if any && t0 >= self.warmup {
            self.locked_time += dt;
        }
    }

    /// Share of the measured window `[warmup, t_end)` during which some area
    /// was gridlocked (NaN: no monitored area).
    pub fn share(&self, t_end: f64) -> f64 {
        if self.areas.is_empty() {
            return f64::NAN;
        }
        self.locked_time / (t_end - self.warmup).max(f64::MIN_POSITIVE)
    }

    /// Start of the first slow spell that became a gridlock (s; NaN: none).
    pub fn onset(&self) -> f64 {
        self.onset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::NewAgent;

    #[test]
    fn a_slow_spell_becomes_a_gridlock_after_the_duration() {
        let det = vec![DetectorSpec::Area {
            id: "q".into(),
            ribbon: "main".into(),
            s_from: 100.0,
            s_to: 200.0,
            interval_s: 60.0,
        }];
        let mut g = Gridlock::new(&det, |_| Some(0), 5.0 / 3.6, 60.0, 30.0);
        let mut a = AgentStore::default();
        let i = a.push(NewAgent {
            id: 1,
            vtype: 0,
            class: 0,
            entry: 0,
            ribbon: 0,
            s: 150.0,
            y: 0.0,
            v: 0.5,
            length: 4.0,
            width: 1.8,
            v0: 10.0,
            v0_base: 10.0,
            v0_bonus: 0.0,
            time_gap: 1.0,
            standstill_gap: 2.0,
            max_accel: 1.0,
            comfortable_decel: 1.0,
            kappa: 0.0,
            clearance_scale: 1.0,
            cruising: 0.0,
            hump_speed: 5.0,
            t_arrival: 0.0,
            t_entry: 0.0,
            critical_gap: 5.0,
            heavy_factor: 1.0,
            aware_distance: f64::INFINITY,
            late_realiser: false,
        }) as usize;
        let orders = vec![vec![i as u32]];
        let mut onsets = Vec::new();
        let dt = 1.0;
        for k in 0..120 {
            let t0 = k as f64;
            // Free for the first 10 s, slow from then on.
            a.v[i] = if k < 10 { 10.0 } else { 0.5 };
            g.observe(&a, &orders, t0, dt, |o| onsets.push(o.since));
        }
        // Slow from 10 s, gridlocked once 60 s have passed (at 69 + dt).
        assert_eq!(onsets, vec![10.0]);
        assert_eq!(g.onset(), 10.0);
        // Locked from t0 = 69 to 119 inclusive: 51 s of the 90 s measured.
        assert!(
            (g.share(120.0) - 51.0 / 90.0).abs() < 1e-12,
            "{}",
            g.share(120.0)
        );
        // An empty area is not slow.
        let mut g2 = Gridlock::new(&det, |_| Some(0), 5.0 / 3.6, 60.0, 0.0);
        for k in 0..200 {
            g2.observe(&a, &[vec![]], k as f64, dt, |_| panic!("empty area"));
        }
        assert_eq!(g2.share(200.0), 0.0);
        assert!(g2.onset().is_nan());
    }
}
