//! Scenario specification (expanded in phase P1).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A scenario: what to simulate, for how long, and what to record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Short identifier, used in run folder names.
    pub name: String,
    /// Free-text description.
    #[serde(default)]
    pub description: String,
    /// Simulated duration including warm-up, in seconds.
    pub duration_s: f64,
    /// Time step in seconds.
    #[serde(default = "default_dt")]
    pub dt_s: f64,
    /// Placeholder demand for the phase-P0 skeleton: Poisson arrivals per hour.
    pub toy_arrivals_per_h: f64,
}

fn default_dt() -> f64 {
    0.1
}

impl Scenario {
    /// Parse a scenario from YAML text.
    pub fn from_yaml(text: &str) -> Result<Self, serde_norway::Error> {
        serde_norway::from_str(text)
    }
}
