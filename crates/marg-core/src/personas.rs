//! Personas and the trait space (G§3, ADR-0018 parts 5–6, phase P8.2).
//!
//! Every agent carries the trait vector `(κ, π, ρ, c, η, skill, δ)`. A
//! persona shifts its mean, in SDs of each trait; within a persona the traits
//! are independent normals with the register's means and SDs. Which personas
//! can drive a class follows G§3.2 ([`Persona::eligible`]); the register's
//! shares are renormalised among them. With no share set, personas are off:
//! every agent has the register's trait distributions and no persona label.
//!
//! κ reaches the parameters that have a measured spread (`v0`, `T`, `s0`,
//! `a`, `b`, the critical gap) through a Gaussian copula: an agent draws at
//! normal score `r·z_κ + √(1 − r²)·ε`, so each keeps its register marginal.

use marg_params::{Dist, Persona, ResolveError, ResolvedSpec, Trait, VehicleClass, VehicleType};

/// Number of personas and of trait dimensions.
pub const N_PERSONAS: usize = Persona::ALL.len();
pub const N_TRAITS: usize = Trait::ALL.len();

/// Index of κ in a trait vector.
pub const KAPPA: usize = 0;
/// Index of the distraction trait δ.
pub const DISTRACTION: usize = 6;

/// Counter bases in an agent's `Decisions` stream: distraction periods
/// (two per episode) and the OU normals (one per step).
pub const DISTRACTION_BASE: u64 = 6 << 40;
pub const OU_BASE: u64 = 7 << 40;

/// A persona index meaning "personas are off".
pub const NO_PERSONA: u8 = u8::MAX;

/// Parameters with a measured spread that κ can be linked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linked {
    FreeFlowSpeed = 0,
    TimeGap = 1,
    StandstillGap = 2,
    MaxAccel = 3,
    ComfortableDecel = 4,
    CriticalGap = 5,
}

impl Linked {
    pub const ALL: [Linked; 6] = [
        Linked::FreeFlowSpeed,
        Linked::TimeGap,
        Linked::StandstillGap,
        Linked::MaxAccel,
        Linked::ComfortableDecel,
        Linked::CriticalGap,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Linked::FreeFlowSpeed => "free_flow_speed",
            Linked::TimeGap => "time_gap",
            Linked::StandstillGap => "standstill_gap",
            Linked::MaxAccel => "max_accel",
            Linked::ComfortableDecel => "comfortable_decel",
            Linked::CriticalGap => "critical_gap",
        }
    }
}

/// Persona, trait, link and within-trip parameters as the register has them.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonaParams {
    /// Persona shares (`None`: personas are off). Unset shares are 0 here and
    /// listed in `missing_shares` (a run with some but not all is refused).
    pub shares: Option<[f64; N_PERSONAS]>,
    pub missing_shares: Vec<&'static str>,
    /// Mean shift of each trait by persona, in SDs of the trait.
    pub shift: [[f64; N_PERSONAS]; N_TRAITS],
    /// Population mean and SD of each trait. A trait whose register entry is
    /// not a normal distribution is named in `non_normal`.
    pub trait_mean: [f64; N_TRAITS],
    pub trait_sd: [f64; N_TRAITS],
    pub non_normal: Vec<&'static str>,
    /// Normal-score correlation of each [`Linked`] parameter with κ.
    pub link: [f64; Linked::ALL.len()],
    /// γ of time pressure, risk tolerance, conformity and norm sensitivity
    /// on the violation coefficients (ADR-0018 part 2).
    pub coefficient_link: [f64; 4],
    /// Within-trip κ: OU reversion rate (1/s) and stationary SD; the jump on
    /// delay and its threshold (s). `None` where unset.
    pub reversion_rate: Option<f64>,
    pub within_sd: Option<f64>,
    pub delay_jump: Option<f64>,
    pub delay_threshold: Option<f64>,
    /// Phone users: share of the population, episodes per second, duration.
    pub phone_share: Option<f64>,
    pub episode_rate: Option<f64>,
    pub episode_duration: Option<Dist>,
}

fn optional(spec: &ResolvedSpec, path: &str) -> Result<Option<f64>, ResolveError> {
    match spec.scalar_si(path) {
        Ok(x) => Ok(Some(x)),
        Err(ResolveError::Unset { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

impl PersonaParams {
    pub fn from_spec(
        spec: &ResolvedSpec,
        kappa_mean: f64,
        kappa_sd: f64,
    ) -> Result<Self, ResolveError> {
        let mut shares = [0.0; N_PERSONAS];
        let mut missing = Vec::new();
        let mut any = false;
        for p in Persona::ALL {
            match optional(spec, &format!("personas.shares.{}", p.key()))? {
                Some(x) => {
                    shares[p.index()] = x;
                    any = true;
                }
                None => missing.push(p.key()),
            }
        }
        let mut shift = [[0.0; N_PERSONAS]; N_TRAITS];
        for t in Trait::ALL {
            for p in Persona::ALL {
                shift[t.index()][p.index()] =
                    spec.scalar_si(&format!("personas.shift.{}.{}", p.key(), t.key()))?;
            }
        }
        let (mut trait_mean, mut trait_sd) = ([0.0; N_TRAITS], [0.0; N_TRAITS]);
        let mut non_normal = Vec::new();
        trait_mean[KAPPA] = kappa_mean;
        trait_sd[KAPPA] = kappa_sd;
        for t in &Trait::ALL[1..] {
            match spec.dist_si(&format!("personas.traits.{}", t.key()))? {
                Dist::Normal { mean, sd } => {
                    trait_mean[t.index()] = mean;
                    trait_sd[t.index()] = sd;
                }
                d => {
                    trait_mean[t.index()] = d.mean();
                    non_normal.push(t.key());
                }
            }
        }
        let mut link = [0.0; Linked::ALL.len()];
        for l in Linked::ALL {
            link[l as usize] = spec.scalar_si(&format!("personas.links.{}", l.key()))?;
        }
        let mut coefficient_link = [0.0; 4];
        for (k, key) in [
            "time_pressure",
            "risk_tolerance",
            "conformity",
            "norm_sensitivity",
        ]
        .iter()
        .enumerate()
        {
            coefficient_link[k] = spec.scalar_si(&format!("personas.links.{key}"))?;
        }
        Ok(Self {
            shares: any.then_some(shares),
            missing_shares: if any { missing } else { Vec::new() },
            shift,
            trait_mean,
            trait_sd,
            non_normal,
            link,
            coefficient_link,
            reversion_rate: optional(spec, "personas.traits.within_trip.reversion_rate")?,
            within_sd: optional(spec, "personas.traits.within_trip.sd")?,
            delay_jump: optional(spec, "personas.traits.within_trip.delay_jump")?,
            delay_threshold: optional(spec, "personas.traits.within_trip.delay_threshold")?,
            phone_share: optional(spec, "personas.traits.phone_use_share")?,
            episode_rate: optional(spec, "personas.distraction.episode_rate")?,
            episode_duration: match spec.dist_si("personas.distraction.episode_duration") {
                Ok(d) => Some(d),
                Err(ResolveError::Unset { .. }) => None,
                Err(e) => return Err(e),
            },
        })
    }

    /// Refuse combinations that cannot run, with a message that says what to
    /// set. `dist_of(link, vtype)` gives the distribution a linked parameter
    /// is drawn from for a vehicle type.
    pub fn check(&self, dist_of: impl Fn(Linked, usize) -> Dist) -> Result<(), String> {
        if !self.missing_shares.is_empty() {
            return Err(format!(
                "persona shares are set for some personas but not for {}: set every \
                 `personas.shares.*` (0 is allowed) or none",
                self.missing_shares.join(", ")
            ));
        }
        if let Some(s) = &self.shares
            && s.iter().any(|&x| x.is_nan() || x < 0.0)
        {
            return Err("persona shares must not be negative".into());
        }
        if !self.non_normal.is_empty() {
            return Err(format!(
                "personas.traits.{{{}}} must be normal distributions (persona shifts are in SDs)",
                self.non_normal.join(", ")
            ));
        }
        for l in Linked::ALL {
            if self.link[l as usize] == 0.0 {
                continue;
            }
            for t in VehicleType::ALL {
                let d = dist_of(l, t.index());
                if d.from_normal_score(0.0).is_none() {
                    return Err(format!(
                        "personas.links.{} is set but the {} distribution for {} has no \
                         closed-form quantile ({d:?}): a κ link needs one (ADR-0018 part 5)",
                        l.key(),
                        l.key(),
                        t.key()
                    ));
                }
            }
        }
        let pair = |a: Option<f64>, b: Option<f64>, what: &str| match (a, b) {
            (Some(_), None) | (None, Some(_)) => Err(format!("{what}: set both or neither")),
            _ => Ok(()),
        };
        pair(
            self.reversion_rate,
            self.within_sd,
            "personas.traits.within_trip.reversion_rate and .sd",
        )?;
        pair(
            self.delay_jump,
            self.delay_threshold,
            "personas.traits.within_trip.delay_jump and .delay_threshold",
        )?;
        if self.reversion_rate.is_some_and(|r| r.is_nan() || r <= 0.0) {
            return Err("personas.traits.within_trip.reversion_rate must be positive".into());
        }
        if self.phone_share.is_some_and(|s| s > 0.0) {
            if !self.episode_rate.is_some_and(|r| r > 0.0) {
                return Err("personas.traits.phone_use_share is set but \
                    `personas.distraction.episode_rate` is unset or 0: set it in the \
                    scenario's overrides (the guide has no number for it)"
                    .into());
            }
            if self.episode_duration.is_none() {
                return Err("personas.traits.phone_use_share is set but \
                    `personas.distraction.episode_duration` is unset: set it in the \
                    scenario's overrides (the guide has no number for it)"
                    .into());
            }
        }
        Ok(())
    }

    /// Whether personas are on (some share is set).
    pub fn enabled(&self) -> bool {
        self.shares.is_some()
    }

    /// The persona of an agent of `class`, from a uniform `u`; `None` when
    /// personas are off. Shares are renormalised among the personas eligible
    /// for the class; if all of theirs are 0, the class's default persona.
    pub fn draw(&self, class: VehicleClass, u: f64) -> Option<Persona> {
        let shares = self.shares.as_ref()?;
        let eligible = Persona::eligible(class);
        let total: f64 = eligible.iter().map(|p| shares[p.index()]).sum();
        if total.is_nan() || total <= 0.0 {
            return Some(eligible[0]);
        }
        let mut x = u * total;
        let mut last = eligible[0];
        for &p in eligible {
            let w = shares[p.index()];
            if w <= 0.0 {
                continue;
            }
            if x < w {
                return Some(p);
            }
            x -= w;
            last = p;
        }
        Some(last)
    }

    /// An agent's trait vector from its own normal scores `z` (one per trait)
    /// and persona: `mean + sd · (z + shift)`. Also returns the shifted
    /// scores, which the copula and the phone-user rule read.
    pub fn traits(
        &self,
        persona: Option<Persona>,
        z: [f64; N_TRAITS],
    ) -> ([f64; N_TRAITS], [f64; N_TRAITS]) {
        let mut x = [0.0; N_TRAITS];
        let mut score = z;
        for t in 0..N_TRAITS {
            if let Some(p) = persona {
                score[t] += self.shift[t][p.index()];
            }
            x[t] = self.trait_mean[t] + self.trait_sd[t] * score[t];
        }
        (x, score)
    }

    /// κ's normal score for an agent whose κ at entry was `kappa`.
    pub fn kappa_score(&self, kappa: f64) -> f64 {
        let sd = self.trait_sd[KAPPA];
        if sd > 0.0 {
            (kappa - self.trait_mean[KAPPA]) / sd
        } else {
            0.0
        }
    }

    /// Whether any within-trip process (OU, jump on delay, distraction)
    /// needs the per-step traits phase.
    pub fn within_trip(&self) -> bool {
        self.reversion_rate.is_some() || self.delay_jump.is_some() || self.phone_users()
    }

    /// Whether anybody uses a phone.
    pub fn phone_users(&self) -> bool {
        self.phone_share.is_some_and(|s| s > 0.0)
    }
}

#[cfg(test)]
impl PersonaParams {
    /// Personas off, standard-normal traits, every link 0 (for unit tests).
    pub(crate) fn from_test() -> Self {
        Self {
            shares: None,
            missing_shares: Vec::new(),
            shift: [[0.0; N_PERSONAS]; N_TRAITS],
            trait_mean: [0.0; N_TRAITS],
            trait_sd: [1.0; N_TRAITS],
            non_normal: Vec::new(),
            link: [0.0; Linked::ALL.len()],
            coefficient_link: [0.0; 4],
            reversion_rate: None,
            within_sd: None,
            delay_jump: None,
            delay_threshold: None,
            phone_share: None,
            episode_rate: None,
            episode_duration: None,
        }
    }
}

/// The persona's register key, or `None` for [`NO_PERSONA`].
pub fn persona_key(index: u8) -> Option<&'static str> {
    Persona::ALL.get(usize::from(index)).map(|p| p.key())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> PersonaParams {
        PersonaParams {
            shares: Some([0.5, 0.1, 0.3, 0.1, 0.0, 0.2, 0.1, 0.1, 0.2, 0.1]),
            missing_shares: Vec::new(),
            shift: [[0.0; N_PERSONAS]; N_TRAITS],
            trait_mean: [0.0; N_TRAITS],
            trait_sd: [1.0; N_TRAITS],
            non_normal: Vec::new(),
            link: [0.0; 6],
            coefficient_link: [0.5; 4],
            reversion_rate: None,
            within_sd: None,
            delay_jump: None,
            delay_threshold: None,
            phone_share: None,
            episode_rate: None,
            episode_duration: None,
        }
    }

    #[test]
    fn shares_are_renormalised_within_a_class() {
        let p = params();
        let n = 100_000;
        let mut counts = [0usize; N_PERSONAS];
        for k in 0..n {
            let u = (k as f64 + 0.5) / n as f64;
            counts[p.draw(VehicleClass::TwoWheeler, u).unwrap().index()] += 1;
        }
        // Two-wheelers: average 0.5, rushed 0.1, delivery 0.3, bike taxi 0.1.
        let share = |q: Persona| counts[q.index()] as f64 / n as f64;
        assert!((share(Persona::AverageCommuter) - 0.5).abs() < 1e-3);
        assert!((share(Persona::DeliveryRider) - 0.3).abs() < 1e-3);
        assert_eq!(counts[Persona::CautiousDriver.index()], 0);
        // A class whose only persona has share 0 still gets it.
        assert_eq!(
            p.draw(VehicleClass::ERickshaw, 0.7),
            Some(Persona::ERickshawDriver)
        );
        // Off: no persona.
        let off = PersonaParams {
            shares: None,
            ..params()
        };
        assert_eq!(off.draw(VehicleClass::Car, 0.3), None);
    }

    #[test]
    fn traits_are_shifted_in_sds() {
        let mut p = params();
        p.trait_mean[1] = 2.0;
        p.trait_sd[1] = 0.5;
        p.shift[1][Persona::DeliveryRider.index()] = 1.5;
        let (x, score) = p.traits(
            Some(Persona::DeliveryRider),
            [0.0, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0],
        );
        assert!((score[1] - 1.7).abs() < 1e-12);
        assert!((x[1] - (2.0 + 0.5 * 1.7)).abs() < 1e-12);
        let (x, _) = p.traits(None, [0.0, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!((x[1] - 2.1).abs() < 1e-12);
    }

    #[test]
    fn partial_shares_and_missing_inputs_are_refused() {
        let dist = |_: Linked, _: usize| Dist::Normal { mean: 1.0, sd: 0.1 };
        let mut p = params();
        assert!(p.check(dist).is_ok());
        p.missing_shares = vec!["cyclist"];
        assert!(p.check(dist).unwrap_err().contains("cyclist"));
        let mut p = params();
        p.phone_share = Some(0.2);
        assert!(p.check(dist).unwrap_err().contains("episode_rate"));
        let mut p = params();
        p.reversion_rate = Some(0.01);
        assert!(p.check(dist).unwrap_err().contains("set both"));
        let mut p = params();
        p.link[0] = 0.5;
        let gamma = |_: Linked, _: usize| Dist::Gamma {
            shape: 2.0,
            scale: 1.0,
        };
        assert!(p.check(gamma).unwrap_err().contains("quantile"));
    }
}
