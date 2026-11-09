//! Lateral relations used by following (plan/03-model-design.md §2–3). The
//! lateral *movement* model arrives in phase P5.

/// Required lateral clearance between two vehicles at mean pair speed `v_mean`
/// (Budhkar & Maurya 2017): `c = intercept + slope · v̄` (SI: m, s).
#[inline]
pub fn clearance(intercept: f64, slope: f64, v_mean: f64) -> f64 {
    intercept + slope * v_mean
}

/// Lateral weight of a leader: 1 when the lateral gap is within the
/// clearance, decaying as `exp(−d/λ)` beyond it.
#[inline]
pub fn leader_weight(excess: f64, braking_scale: f64) -> f64 {
    if excess <= 0.0 {
        1.0
    } else {
        libm::exp(-excess / braking_scale)
    }
}

/// Edge-to-edge lateral gap between two bodies centred at `ya`, `yb` with
/// widths `wa`, `wb` (negative when they overlap laterally).
#[inline]
pub fn lateral_gap(ya: f64, wa: f64, yb: f64, wb: f64) -> f64 {
    (ya - yb).abs() - 0.5 * (wa + wb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearance_matches_budhkar_maurya() {
        // C[cm] = 0.615 v[km/h] + 120.9: 1.46 m at 40 km/h, 1.33 m at 20 km/h.
        let (i, s) = (1.209, 0.615 * 0.036);
        assert!((clearance(i, s, 40.0 / 3.6) - 1.455).abs() < 1e-3);
        assert!((clearance(i, s, 20.0 / 3.6) - 1.332).abs() < 1e-3);
    }

    #[test]
    fn weight_decays_beyond_clearance() {
        assert_eq!(leader_weight(-0.3, 0.15), 1.0);
        assert_eq!(leader_weight(0.0, 0.15), 1.0);
        assert!((leader_weight(0.15, 0.15) - (-1.0f64).exp()).abs() < 1e-12);
    }

    #[test]
    fn gaps() {
        assert_eq!(lateral_gap(0.0, 2.0, 3.0, 2.0), 1.0);
        assert!(lateral_gap(0.0, 2.0, 1.0, 2.0) < 0.0);
    }
}
