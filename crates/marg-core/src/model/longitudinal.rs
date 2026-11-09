//! Longitudinal following: the Intelligent Driver Model (Treiber, Hennecke &
//! Helbing 2000) with the lane-free leader set of plan/03-model-design.md §2.

/// One driver's IDM parameters (SI).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Idm {
    /// Desired speed `v0` (m/s).
    pub v0: f64,
    /// Desired time gap `T` (s).
    pub time_gap: f64,
    /// Standstill gap `s0` (m).
    pub standstill_gap: f64,
    /// Maximum acceleration `a` (m/s²).
    pub max_accel: f64,
    /// Comfortable deceleration `b` (m/s²).
    pub comfortable_decel: f64,
    /// Acceleration exponent `δ`.
    pub exponent: f64,
}

impl Idm {
    /// `(v / v0)^δ`, with a fast path for the standard δ = 4.
    #[inline]
    fn speed_term(&self, v: f64) -> f64 {
        let r = v / self.v0;
        if self.exponent == 4.0 {
            // allow-literal: fast path for the standard IDM exponent
            let r2 = r * r;
            r2 * r2
        } else {
            libm::pow(r.max(0.0), self.exponent)
        }
    }

    /// Free-road acceleration `a [1 − (v/v0)^δ]`.
    #[inline]
    pub fn free_accel(&self, v: f64) -> f64 {
        self.max_accel * (1.0 - self.speed_term(v))
    }

    /// Desired dynamic gap `s*` to a leader, with pair multipliers on `T` and `s0`.
    #[inline]
    pub fn desired_gap(&self, v: f64, dv: f64, mult_t: f64, mult_s0: f64) -> f64 {
        let dynamic = v * self.time_gap * mult_t
            + v * dv / (2.0 * (self.max_accel * self.comfortable_decel).sqrt());
        self.standstill_gap * mult_s0 + dynamic.max(0.0)
    }

    /// IDM acceleration behind a leader at bumper gap `gap` (> 0) with
    /// approach rate `dv = v − v_leader`.
    #[inline]
    pub fn accel(&self, v: f64, gap: f64, dv: f64, mult_t: f64, mult_s0: f64) -> f64 {
        let s_star = self.desired_gap(v, dv, mult_t, mult_s0);
        let q = s_star / gap;
        self.max_accel * (1.0 - self.speed_term(v) - q * q)
    }

    /// Highest speed at which a vehicle can enter `gap` behind a leader moving
    /// at `v_leader` without exceeding the desired gap (`s* ≤ gap`), capped at
    /// `v0`. Solves `k v² + (T − k v_l) v − (gap − s0) = 0` with `k = 1/(2√(ab))`.
    pub fn insertion_speed(&self, gap: f64, v_leader: f64, mult_t: f64, mult_s0: f64) -> f64 {
        let free = gap - self.standstill_gap * mult_s0;
        if free <= 0.0 {
            return 0.0;
        }
        let k = 1.0 / (2.0 * (self.max_accel * self.comfortable_decel).sqrt());
        let t = self.time_gap * mult_t;
        let bq = t - k * v_leader;
        let v = (-bq + (bq * bq + 4.0 * k * free).sqrt()) / (2.0 * k); // allow-literal: quadratic formula
        v.clamp(0.0, self.v0)
    }
}

/// Combine one leader into the running acceleration (model design §2):
/// `a ← min(a, a_free + ω (a_IDM − a_free))`.
#[inline]
pub fn combine(current: f64, free: f64, leader: f64, weight: f64) -> f64 {
    current.min(free + weight * (leader - free))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car() -> Idm {
        Idm {
            v0: 15.0,
            time_gap: 1.0,
            standstill_gap: 2.0,
            max_accel: 1.0,
            comfortable_decel: 1.0,
            exponent: 4.0,
        }
    }

    #[test]
    fn free_road_reaches_v0() {
        let m = car();
        assert!(m.free_accel(0.0) == 1.0);
        assert!(m.free_accel(15.0).abs() < 1e-12);
        let general = Idm {
            exponent: 4.0000001,
            ..m
        };
        assert!((general.free_accel(10.0) - m.free_accel(10.0)).abs() < 1e-6);
    }

    #[test]
    fn equilibrium_gap() {
        // At equilibrium with dv = 0, a = 0 ⇔ gap = s* / sqrt(1 − (v/v0)^4).
        let m = car();
        let v = 10.0;
        let s_star = m.desired_gap(v, 0.0, 1.0, 1.0);
        assert!((s_star - 12.0).abs() < 1e-12);
        let gap = s_star / (1.0 - (v / 15.0f64).powi(4)).sqrt();
        assert!(m.accel(v, gap, 0.0, 1.0, 1.0).abs() < 1e-12);
    }

    #[test]
    fn insertion_speed_keeps_desired_gap() {
        let m = car();
        for (gap, vl) in [(30.0, 10.0), (12.0, 0.0), (60.0, 14.0), (5.0, 3.0)] {
            let v = m.insertion_speed(gap, vl, 1.0, 1.0);
            assert!(v <= m.v0 + 1e-12);
            if v < m.v0 - 1e-9 {
                assert!(
                    (m.desired_gap(v, v - vl, 1.0, 1.0) - gap).abs() < 1e-6,
                    "gap {gap}, vl {vl}, v {v}"
                );
            }
        }
        assert_eq!(m.insertion_speed(1.5, 5.0, 1.0, 1.0), 0.0);
    }

    #[test]
    fn combine_takes_the_strongest_weighted_constraint() {
        assert_eq!(combine(1.0, 1.0, -3.0, 1.0), -3.0);
        assert_eq!(combine(1.0, 1.0, -3.0, 0.5), -1.0);
        assert_eq!(combine(-2.0, 1.0, -3.0, 0.5), -2.0);
        assert_eq!(combine(1.0, 1.0, -3.0, 0.0), 1.0);
    }
}
