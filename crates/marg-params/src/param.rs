//! A register entry: one parameter with its value, unit, trust grade, role and
//! source (G§9, plan/03-model-design.md).

use crate::dist::{Dist, DistError};
use crate::units::Unit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How far a value can be trusted (G: "give every parameter a trust grade").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Grade {
    /// Measured in Indian mixed traffic.
    A,
    /// Measured elsewhere, or only partly in India.
    B,
    /// Never measured: an experimental factor to sweep.
    C,
    #[serde(rename = "A/B")]
    AB,
    #[serde(rename = "B/C")]
    BC,
}

impl Grade {
    pub fn label(self) -> &'static str {
        match self {
            Grade::A => "A",
            Grade::B => "B",
            Grade::C => "C",
            Grade::AB => "A/B",
            Grade::BC => "B/C",
        }
    }
}

/// What may be done with a parameter (G§8.1 three-way split).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Measured directly: never calibrated.
    Fix,
    /// Calibrated in the stage given by `stage`.
    Fit,
    /// Experimental factor: varied, never fitted.
    Sweep,
    /// Set per scenario (bundles, switches); neither fitted nor swept by default.
    Scenario,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Fix => "fix",
            Role::Fit => "fit",
            Role::Sweep => "sweep",
            Role::Scenario => "scenario",
        }
    }
}

/// A parameter value: a plain number or a distribution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ParamValue {
    Scalar(f64),
    Dist(Dist),
}

impl ParamValue {
    /// The value in SI, given the unit it is written in.
    pub fn to_si(&self, unit: Unit) -> ParamValue {
        let f = unit.si_factor();
        match self {
            ParamValue::Scalar(x) => ParamValue::Scalar(x * f),
            ParamValue::Dist(d) => ParamValue::Dist(d.scaled(f)),
        }
    }

    /// Mean of the value (the value itself for scalars).
    pub fn mean(&self) -> f64 {
        match self {
            ParamValue::Scalar(x) => *x,
            ParamValue::Dist(d) => d.mean(),
        }
    }

    /// The value as a distribution (scalars become constants), canonicalised.
    pub fn as_dist(&self) -> Dist {
        match self {
            ParamValue::Scalar(x) => Dist::Constant { value: *x },
            ParamValue::Dist(d) => d.canonical(),
        }
    }

    pub fn validate(&self) -> Result<(), DistError> {
        match self {
            ParamValue::Scalar(x) if !x.is_finite() => {
                Err(DistError("value must be finite".into()))
            }
            ParamValue::Scalar(_) => Ok(()),
            ParamValue::Dist(d) => d.validate(),
        }
    }

    /// Compact one-line description, e.g. `normal(49.19, 12.43)`.
    pub fn describe(&self) -> String {
        match self {
            ParamValue::Scalar(x) => format!("{x}"),
            ParamValue::Dist(d) => {
                let json = serde_json::to_value(d).unwrap_or_default();
                let mut kind = String::new();
                let mut args = Vec::new();
                if let serde_json::Value::Object(map) = json {
                    for (k, v) in map {
                        if k == "dist" {
                            kind = v.as_str().unwrap_or("").to_string();
                        } else {
                            args.push(format!("{k}={v}"));
                        }
                    }
                }
                format!("{kind}({})", args.join(", "))
            }
        }
    }
}

/// One register entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParamEntry {
    /// The starting value or distribution, in `unit`. `null` means "not yet
    /// known": the parameter must be measured, or set by a scenario or sweep,
    /// before any module may use it.
    pub value: Option<ParamValue>,
    /// Unit the value (and `range`) are written in.
    pub unit: Unit,
    /// Trust grade.
    pub grade: Grade,
    /// Fix, Fit, Sweep or Scenario.
    pub role: Role,
    /// Calibration stage 0–7 (G§8.3). Required for `fit`, absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<u8>,
    /// Where the value comes from.
    pub source: String,
    /// Guide section(s), e.g. `"§2.1"`.
    pub guide: String,
    /// Which row of the guide's master register (§9) this entry implements,
    /// e.g. `"9.2/desired_time_gap"`. Used to check every row is covered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub register_row: Option<String>,
    /// Hard bounds `[min, max]` for overrides and sweeps, in `unit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f64; 2]>,
    /// Whether the live viewer may change it during a run.
    #[serde(default)]
    pub live: bool,
    /// Assumptions, caveats, how a range was turned into a distribution.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

/// A malformed register entry.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EntryError {
    #[error("{0}")]
    Dist(#[from] DistError),
    #[error("role `fit` needs a calibration stage")]
    FitWithoutStage,
    #[error("stage is only allowed for role `fit`")]
    StageWithoutFit,
    #[error("stage must be 0–7, got {0}")]
    BadStage(u8),
    #[error("range min must be below max")]
    BadRange,
    #[error("value {value} is outside range [{min}, {max}]")]
    OutOfRange { value: f64, min: f64, max: f64 },
    #[error("source must not be empty")]
    NoSource,
}

impl ParamEntry {
    /// Check internal consistency.
    pub fn validate(&self) -> Result<(), EntryError> {
        if let Some(v) = &self.value {
            v.validate()?;
        }
        match (self.role, self.stage) {
            (Role::Fit, None) => return Err(EntryError::FitWithoutStage),
            (Role::Fit, Some(s)) if s > 7 => return Err(EntryError::BadStage(s)),
            (Role::Fix | Role::Sweep | Role::Scenario, Some(_)) => {
                return Err(EntryError::StageWithoutFit);
            }
            _ => {}
        }
        if self.source.trim().is_empty() {
            return Err(EntryError::NoSource);
        }
        if let Some([min, max]) = self.range {
            if min >= max {
                return Err(EntryError::BadRange);
            }
            if let Some(v) = &self.value {
                check_in_range(v, min, max)?;
            }
        }
        Ok(())
    }

    /// Check a candidate value (e.g. an override) against this entry's range.
    pub fn check_value(&self, v: &ParamValue) -> Result<(), EntryError> {
        v.validate()?;
        if let Some([min, max]) = self.range {
            check_in_range(v, min, max)?;
        }
        Ok(())
    }
}

fn check_in_range(v: &ParamValue, min: f64, max: f64) -> Result<(), EntryError> {
    let x = v.mean();
    if x < min || x > max {
        return Err(EntryError::OutOfRange { value: x, min, max });
    }
    Ok(())
}
