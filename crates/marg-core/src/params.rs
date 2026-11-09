//! Typed parameters read once from the resolved spec at start-up (plan P1.6).
//!
//! Every model constant comes from here, and every value here comes from the
//! register through access-tracked accessors, so a missing path fails at
//! start-up and `ResolvedSpec::unused()` lists entries nothing read.

use marg_params::{Dist, ResolveError, ResolvedSpec, VehicleClass, VehicleType};

/// Number of vehicle types.
pub const N_TYPES: usize = VehicleType::ALL.len();
/// Number of vehicle classes.
pub const N_CLASSES: usize = VehicleClass::ALL.len();

/// Per-type distributions (SI).
#[derive(Debug, Clone)]
pub struct TypeParams {
    pub length: Dist,
    pub width: Dist,
    pub free_flow_speed: Dist,
}

/// Per-class distributions and values (SI).
#[derive(Debug, Clone)]
pub struct ClassParams {
    pub time_gap: Dist,
    pub standstill_gap: Dist,
    pub max_accel: Dist,
    pub comfortable_decel: Dist,
    pub min_headway: f64,
    pub entry_lateral_position: Dist,
}

/// All parameters the core uses, in SI.
#[derive(Debug, Clone)]
pub struct CoreParams {
    pub types: Vec<TypeParams>,
    pub classes: Vec<ClassParams>,
    /// IDM acceleration exponent.
    pub accel_exponent: f64,
    pub emergency_decel: f64,
    /// Follower × leader multipliers on the time gap and standstill gap.
    pub mult_time_gap: [[f64; N_CLASSES]; N_CLASSES],
    pub mult_standstill_gap: [[f64; N_CLASSES]; N_CLASSES],
    /// Lateral clearance `c(v̄) = intercept + slope · v̄` (m, s).
    pub clearance_intercept: f64,
    pub clearance_slope: f64,
    /// `λ_brake`: lateral decay scale of longitudinal interaction (m).
    pub braking_scale: f64,
    pub median_band_width: f64,
    pub median_band_bonus: Dist,
    pub lookahead: f64,
    pub interaction_weight_cutoff: f64,
    pub insertion_attempts_per_step: usize,
    pub stop_speed_below: f64,
    pub stop_rearm_above: f64,
    pub lateral_histogram_bin: f64,
}

fn type_or_class(spec: &ResolvedSpec, prefix: &str, t: VehicleType) -> Result<Dist, ResolveError> {
    let by_type = format!("{prefix}.{}", t.key());
    if spec.contains(&by_type) {
        spec.dist_si(&by_type)
    } else {
        spec.dist_si(&format!("{prefix}.{}", t.class().key()))
    }
}

fn pair_matrix(
    spec: &ResolvedSpec,
    prefix: &str,
) -> Result<[[f64; N_CLASSES]; N_CLASSES], ResolveError> {
    let default = spec.scalar_si(&format!("{prefix}.default"))?;
    let mut m = [[default; N_CLASSES]; N_CLASSES];
    for f in VehicleClass::ALL {
        for l in VehicleClass::ALL {
            let path = format!("{prefix}.{}.{}", f.key(), l.key());
            if spec.contains(&path) {
                m[f.index()][l.index()] = spec.scalar_si(&path)?;
            }
        }
    }
    Ok(m)
}

impl CoreParams {
    /// Read every core parameter from the resolved spec.
    pub fn from_spec(spec: &ResolvedSpec) -> Result<Self, ResolveError> {
        let types = VehicleType::ALL
            .iter()
            .map(|&t| {
                Ok(TypeParams {
                    length: type_or_class(spec, "vehicles.length", t)?,
                    width: type_or_class(spec, "vehicles.width", t)?,
                    free_flow_speed: type_or_class(spec, "vehicles.free_flow_speed", t)?,
                })
            })
            .collect::<Result<Vec<_>, ResolveError>>()?;
        let classes = VehicleClass::ALL
            .iter()
            .map(|&c| {
                let k = c.key();
                Ok(ClassParams {
                    time_gap: spec.dist_si(&format!("interaction.following.time_gap.{k}"))?,
                    standstill_gap: spec
                        .dist_si(&format!("interaction.following.standstill_gap.{k}"))?,
                    max_accel: spec.dist_si(&format!("interaction.following.max_accel.{k}"))?,
                    comfortable_decel: spec
                        .dist_si(&format!("interaction.following.comfortable_decel.{k}"))?,
                    min_headway: spec.scalar_si(&format!("demand.min_headway.{k}"))?,
                    entry_lateral_position: spec
                        .dist_si(&format!("demand.entry_lateral_position.{k}"))?,
                })
            })
            .collect::<Result<Vec<_>, ResolveError>>()?;
        Ok(Self {
            types,
            classes,
            accel_exponent: spec.scalar_si("interaction.following.accel_exponent")?,
            emergency_decel: spec.scalar_si("interaction.following.emergency_decel")?,
            mult_time_gap: pair_matrix(spec, "interaction.following.pair_multiplier.time_gap")?,
            mult_standstill_gap: pair_matrix(
                spec,
                "interaction.following.pair_multiplier.standstill_gap",
            )?,
            clearance_intercept: spec.scalar_si("interaction.lateral.clearance.intercept")?,
            clearance_slope: spec.scalar_si("interaction.lateral.clearance.slope")?,
            braking_scale: spec.scalar_si("interaction.lateral.braking_scale")?,
            median_band_width: spec.scalar_si("vehicles.median_band.width")?,
            median_band_bonus: spec.dist_si("vehicles.median_band.speed_bonus")?,
            lookahead: spec.scalar_si("core.lookahead_distance")?,
            interaction_weight_cutoff: spec.scalar_si("core.interaction_weight_cutoff")?,
            insertion_attempts_per_step: spec
                .scalar_si("core.insertion_attempts_per_step")?
                .max(1.0) as usize,
            stop_speed_below: spec.scalar_si("observers.stop.speed_below")?,
            stop_rearm_above: spec.scalar_si("observers.stop.rearm_above")?,
            lateral_histogram_bin: spec.scalar_si("observers.lateral_histogram_bin")?,
        })
    }

    /// Lateral distance beyond the clearance at which a leader's weight ω
    /// drops below the cutoff: `d_cut = −λ ln(cutoff)`.
    pub fn interaction_cutoff_distance(&self) -> f64 {
        if self.interaction_weight_cutoff <= 0.0 {
            f64::INFINITY
        } else {
            -self.braking_scale * libm::log(self.interaction_weight_cutoff)
        }
    }
}
