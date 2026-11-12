//! Typed parameters read once from the resolved spec at start-up (plan P1.6).
//!
//! Every model constant comes from here, and every value here comes from the
//! register through access-tracked accessors, so a missing path fails at
//! start-up and `ResolvedSpec::unused()` lists entries nothing read.

use marg_params::dist::norm_ppf;
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

/// A per-agent parameter drawn at entry. The discriminant is the draw's
/// counter block in the agent's stream (`Draws::sample`); never reuse one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Drawn {
    Length = 0,
    Width = 1,
    V0 = 2,
    TimeGap = 3,
    StandstillGap = 4,
    MaxAccel = 5,
    ComfortableDecel = 6,
}

impl Drawn {
    pub fn key(self) -> &'static str {
        match self {
            Drawn::Length => "length",
            Drawn::Width => "width",
            Drawn::V0 => "v0",
            Drawn::TimeGap => "time_gap",
            Drawn::StandstillGap => "standstill_gap",
            Drawn::MaxAccel => "max_accel",
            Drawn::ComfortableDecel => "comfortable_decel",
        }
    }
}

/// Where one vehicle type's draw of a per-agent parameter comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawSource {
    /// Register path of the distribution (by type, else by class).
    pub path: String,
    pub drawn: Drawn,
    /// Index into `VehicleType::ALL`.
    pub vtype: usize,
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
    /// Desired lateral clearance per pair of classes (ADR-0010 §1):
    /// `c_des(v̄) = intercept + slope · v̄` (m, s), symmetric.
    pub clearance_intercept: [[f64; N_CLASSES]; N_CLASSES],
    pub clearance_slope: [[f64; N_CLASSES]; N_CLASSES],
    /// The pooled clearance line (for bodies without a class of their own).
    pub clearance_pooled: (f64, f64),
    /// `Δ`: the minimum clearance is the desired one minus this (m) …
    pub clearance_min_offset: f64,
    /// … and never below this (m).
    pub clearance_min_floor: f64,
    /// `γ_c`: clearances are scaled by `exp(−γ_c κ)`.
    pub clearance_link: f64,
    /// Aggressiveness κ ~ N(mean, sd), drawn at entry.
    pub kappa_mean: f64,
    pub kappa_sd: f64,
    /// Clearance scale of the squeezing rule when the cooperative twin fixes
    /// κ at a population quantile (ADR-0007); `None` = each agent's own.
    pub squeeze_scale: Option<f64>,
    /// `λ_brake`: lateral decay scale of longitudinal interaction (m).
    pub braking_scale: f64,
    /// `λ_steer`: lateral decay scale of the steering response (m).
    pub steering_scale: f64,
    /// `λ_s`: desired lateral speed per unit of emergency deceleration (s).
    pub steering_sensitivity: f64,
    /// `τ`: time constant of lateral tracking (s).
    pub lateral_time_constant: f64,
    /// `θ_max` (rad) and its tangent.
    pub max_heading: f64,
    pub max_heading_tan: f64,
    pub max_heading_sin: f64,
    /// Kerbside parking (G§6.2). Unset in the register unless a scenario
    /// or a calibration supplies them; a parking stretch may give its own.
    pub parking: ParkingParams,
    /// MOBIL politeness `p0`.
    pub politeness: f64,
    /// MOBIL safe braking limit `b_safe` (m/s²).
    pub safe_decel: f64,
    /// MOBIL switching threshold `Δa_thr` (m/s²).
    pub switch_threshold: f64,
    /// `w_c`: utility weight of a clearance shortfall (m/s²).
    pub comfort_weight: f64,
    /// `c_move`: utility cost per metre of lateral move (m/s² per m).
    pub move_cost: f64,
    /// `c_edge`: desired clearance to the carriageway edge (m).
    pub edge_clearance: f64,
    /// `v_creep` by class (m/s).
    pub creep_speed: [f64; N_CLASSES],
    /// `Δt_dec` (s), `Δy_cand` (m) and `R_cand` (m).
    pub decision_interval: f64,
    pub candidate_step: f64,
    pub candidate_range: f64,
    /// TTC below which a conflict episode is recorded (s).
    pub conflict_ttc: f64,
    /// Whether changes of lateral target are written as events.
    pub log_lateral_decisions: bool,
    pub median_band_width: f64,
    pub median_band_bonus: Dist,
    pub lookahead: f64,
    pub interaction_weight_cutoff: f64,
    pub insertion_attempts_per_step: usize,
    pub stop_speed_below: f64,
    pub stop_rearm_above: f64,
    pub lateral_histogram_bin: f64,
    /// Which register path every per-agent draw of every type reads (for
    /// the live viewer's "resample existing agents").
    pub draw_sources: Vec<DrawSource>,
}

/// The register path a type reads for `prefix`: by type if the register has
/// it, else by class.
fn type_or_class_path(spec: &ResolvedSpec, prefix: &str, t: VehicleType) -> String {
    let by_type = format!("{prefix}.{}", t.key());
    if spec.contains(&by_type) {
        by_type
    } else {
        format!("{prefix}.{}", t.class().key())
    }
}

fn type_or_class(spec: &ResolvedSpec, prefix: &str, t: VehicleType) -> Result<Dist, ResolveError> {
    spec.dist_si(&type_or_class_path(spec, prefix, t))
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

/// Kerbside-parking parameters as the register has them (`None`: unset).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParkingParams {
    /// Gap between the kerb and a parked vehicle (m).
    pub offset_from_kerb: Option<Dist>,
    pub double_parking_probability: Option<f64>,
    /// Manoeuvres per second for the register's reference length of kerb
    /// (100 m: the register's unit is events per 100 m per hour).
    pub manoeuvre_rate: Option<f64>,
    /// How long a manoeuvre blocks the strip beside the space (s).
    pub manoeuvre_duration: Option<Dist>,
    pub cruising_share: Option<f64>,
    /// Speed cap of a vehicle looking for a space (m/s).
    pub cruising_speed: Option<f64>,
}

/// A distribution that may be unset (`null`) in the register.
fn optional_dist(spec: &ResolvedSpec, path: &str) -> Result<Option<Dist>, ResolveError> {
    match spec.dist_si(path) {
        Ok(x) => Ok(Some(x)),
        Err(ResolveError::Unset { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// A scalar that may be unset (`null`) in the register.
fn optional_scalar(spec: &ResolvedSpec, path: &str) -> Result<Option<f64>, ResolveError> {
    match spec.scalar_si(path) {
        Ok(x) => Ok(Some(x)),
        Err(ResolveError::Unset { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// The desired-clearance line of every pair of classes: the pair's own entry
/// (by clearance group, either order) or the pooled line.
fn clearance_matrix(
    spec: &ResolvedSpec,
    what: &str,
) -> Result<[[f64; N_CLASSES]; N_CLASSES], ResolveError> {
    let pooled = spec.scalar_si(&format!("interaction.lateral.clearance.{what}"))?;
    let mut m = [[pooled; N_CLASSES]; N_CLASSES];
    for a in VehicleClass::ALL {
        for b in VehicleClass::ALL {
            let (ga, gb) = (a.clearance_group().key(), b.clearance_group().key());
            for (x, y) in [(ga, gb), (gb, ga)] {
                let path = format!("interaction.lateral.clearance.pair.{what}.{x}_{y}");
                if spec.contains(&path) {
                    m[a.index()][b.index()] = spec.scalar_si(&path)?;
                }
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
        let mut draw_sources = Vec::new();
        for t in VehicleType::ALL {
            for (prefix, drawn) in [
                ("vehicles.length", Drawn::Length),
                ("vehicles.width", Drawn::Width),
                ("vehicles.free_flow_speed", Drawn::V0),
            ] {
                draw_sources.push(DrawSource {
                    path: type_or_class_path(spec, prefix, t),
                    drawn,
                    vtype: t.index(),
                });
            }
            for (name, drawn) in [
                ("time_gap", Drawn::TimeGap),
                ("standstill_gap", Drawn::StandstillGap),
                ("max_accel", Drawn::MaxAccel),
                ("comfortable_decel", Drawn::ComfortableDecel),
            ] {
                draw_sources.push(DrawSource {
                    path: format!("interaction.following.{name}.{}", t.class().key()),
                    drawn,
                    vtype: t.index(),
                });
            }
        }
        let clearance_link = spec.scalar_si("personas.links.clearance")?;
        let kappa_mean = spec.scalar_si("personas.traits.aggressiveness.mean")?;
        let kappa_sd = spec.scalar_si("personas.traits.aggressiveness.sd")?;
        let squeeze_scale = optional_scalar(spec, "personas.cooperative.squeeze_kappa_quantile")?
            .map(|q| libm::exp(-clearance_link * (kappa_mean + kappa_sd * norm_ppf(q))));
        let mut creep_speed = [0.0; N_CLASSES];
        for c in VehicleClass::ALL {
            creep_speed[c.index()] =
                spec.scalar_si(&format!("interaction.lateral.creep_speed.{}", c.key()))?;
        }
        let max_heading = spec.scalar_si("interaction.lateral.max_heading")?;
        Ok(Self {
            types,
            classes,
            draw_sources,
            clearance_pooled: (
                spec.scalar_si("interaction.lateral.clearance.intercept")?,
                spec.scalar_si("interaction.lateral.clearance.slope")?,
            ),
            clearance_intercept: clearance_matrix(spec, "intercept")?,
            clearance_slope: clearance_matrix(spec, "slope")?,
            clearance_min_offset: spec.scalar_si("interaction.lateral.clearance.min_offset")?,
            clearance_min_floor: spec.scalar_si("interaction.lateral.clearance.min_floor")?,
            clearance_link,
            kappa_mean,
            kappa_sd,
            squeeze_scale,
            steering_scale: spec.scalar_si("interaction.lateral.steering_scale")?,
            steering_sensitivity: spec.scalar_si("interaction.lateral.steering_sensitivity")?,
            lateral_time_constant: spec.scalar_si("interaction.lateral.time_constant")?,
            max_heading,
            max_heading_tan: libm::tan(max_heading),
            max_heading_sin: libm::sin(max_heading),
            parking: ParkingParams {
                offset_from_kerb: optional_dist(spec, "road.parking.offset_from_kerb")?,
                double_parking_probability: optional_scalar(
                    spec,
                    "road.parking.double_parking_probability",
                )?,
                manoeuvre_rate: optional_scalar(spec, "road.parking.manoeuvre_rate")?,
                manoeuvre_duration: optional_dist(spec, "road.parking.manoeuvre_duration")?,
                cruising_share: optional_scalar(spec, "road.parking.cruising_share")?,
                cruising_speed: optional_scalar(spec, "road.parking.cruising_speed")?,
            },
            politeness: spec.scalar_si("interaction.lateral.politeness")?,
            safe_decel: spec.scalar_si("interaction.lateral.safe_decel")?,
            switch_threshold: spec.scalar_si("interaction.lateral.switch_threshold")?,
            comfort_weight: spec.scalar_si("interaction.lateral.comfort_weight")?,
            move_cost: spec.scalar_si("interaction.lateral.move_cost")?,
            edge_clearance: spec.scalar_si("interaction.lateral.edge_clearance")?,
            creep_speed,
            decision_interval: spec.scalar_si("core.decision_interval")?,
            candidate_step: spec.scalar_si("core.lateral_candidate_step")?,
            candidate_range: spec.scalar_si("core.lateral_candidate_range")?,
            conflict_ttc: spec.scalar_si("observers.conflict.ttc_threshold")?,
            log_lateral_decisions: spec.scalar_si("observers.lateral.log_decisions")? > 0.5,
            accel_exponent: spec.scalar_si("interaction.following.accel_exponent")?,
            emergency_decel: spec.scalar_si("interaction.following.emergency_decel")?,
            mult_time_gap: pair_matrix(spec, "interaction.following.pair_multiplier.time_gap")?,
            mult_standstill_gap: pair_matrix(
                spec,
                "interaction.following.pair_multiplier.standstill_gap",
            )?,
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

    /// The distribution a type draws `drawn` from.
    pub fn draw_dist(&self, drawn: Drawn, vtype: usize) -> &Dist {
        let t = &self.types[vtype];
        let c = &self.classes[VehicleType::ALL[vtype].class().index()];
        match drawn {
            Drawn::Length => &t.length,
            Drawn::Width => &t.width,
            Drawn::V0 => &t.free_flow_speed,
            Drawn::TimeGap => &c.time_gap,
            Drawn::StandstillGap => &c.standstill_gap,
            Drawn::MaxAccel => &c.max_accel,
            Drawn::ComfortableDecel => &c.comfortable_decel,
        }
    }

    /// The agent-independent part of the clearances between classes `ci`
    /// and `cj` at mean pair speed `v_mean`: `(desired, minimum)` in metres.
    /// Multiply both by the agent's `clearance_scale`.
    #[inline]
    pub fn pair_clearance(&self, ci: usize, cj: usize, v_mean: f64) -> (f64, f64) {
        crate::model::lateral::clearances(
            self.clearance_intercept[ci][cj],
            self.clearance_slope[ci][cj],
            self.clearance_min_offset,
            self.clearance_min_floor,
            v_mean,
        )
    }

    /// [`Self::pair_clearance`] to a road object at mean pair speed
    /// `v_mean`: the line of the pair with the object's clearance class, or
    /// the pooled line if it has none (ADR-0010 §1).
    #[inline]
    pub fn object_clearance(&self, ci: usize, class: Option<usize>, v_mean: f64) -> (f64, f64) {
        match class {
            Some(cj) => self.pair_clearance(ci, cj, v_mean),
            None => crate::model::lateral::clearances(
                self.clearance_pooled.0,
                self.clearance_pooled.1,
                self.clearance_min_offset,
                self.clearance_min_floor,
                v_mean,
            ),
        }
    }

    /// Multipliers on the time gap and standstill gap of a vehicle of class
    /// `ci` stopping behind a road object: those of the pair with the
    /// object's clearance class, or 1.
    #[inline]
    pub fn object_multipliers(&self, ci: usize, class: Option<usize>) -> (f64, f64) {
        match class {
            Some(cj) => (self.mult_time_gap[ci][cj], self.mult_standstill_gap[ci][cj]),
            None => (1.0, 1.0),
        }
    }

    /// The clearance scale `exp(−γ_c κ)` of an agent with aggressiveness `kappa`.
    pub fn clearance_scale(&self, kappa: f64) -> f64 {
        libm::exp(-self.clearance_link * kappa)
    }

    /// Lateral distance beyond the minimum clearance at which the steering
    /// response drops below the weight cutoff.
    pub fn steering_cutoff_distance(&self) -> f64 {
        if self.interaction_weight_cutoff <= 0.0 {
            f64::INFINITY
        } else {
            -self.steering_scale * libm::log(self.interaction_weight_cutoff)
        }
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
