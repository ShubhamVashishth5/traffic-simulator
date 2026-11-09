//! Resolver: register ← scenario overrides ← experiment values ← live commands,
//! producing the resolved run spec whose hash is the run's identity.

use crate::dist::Dist;
use crate::param::{EntryError, Grade, ParamEntry, ParamValue, Role};
use crate::register::Register;
use crate::spec::Scenario;
use crate::units::Unit;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::Mutex;

/// Which layer last set a parameter's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Base,
    Scenario,
    Experiment,
    Live,
}

impl Layer {
    pub fn label(self) -> &'static str {
        match self {
            Layer::Base => "base",
            Layer::Scenario => "scenario",
            Layer::Experiment => "experiment",
            Layer::Live => "live",
        }
    }
}

/// A parameter after all overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedParam {
    /// Value in the register unit (`None` = unset).
    pub value: Option<ParamValue>,
    /// Value in SI, with lognormal forms canonicalised.
    pub si: Option<ParamValue>,
    pub unit: Unit,
    pub grade: Grade,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<u8>,
    pub source: String,
    pub guide: String,
    pub set_by: Layer,
    /// True when a `fix` parameter was overridden with `allow_fix_override`.
    #[serde(default)]
    pub fix_overridden: bool,
}

/// Options for [`resolve`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolveOptions {
    /// Allow overriding `fix` (measured) parameters. Recorded per parameter.
    pub allow_fix_override: bool,
}

/// Errors from resolving a run spec or reading a parameter.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ResolveError {
    #[error("unknown parameter `{0}` (not in the register)")]
    Unknown(String),
    #[error("parameter `{path}`: {source}")]
    Invalid { path: String, source: EntryError },
    #[error(
        "parameter `{0}` has role `fix` (measured, never calibrated); pass --allow-fix-override to change it"
    )]
    FixOverride(String),
    #[error(
        "parameter `{path}` has no value (grade {grade}, role {role}): set it in the scenario overrides or an experiment"
    )]
    Unset {
        path: String,
        grade: &'static str,
        role: &'static str,
    },
    #[error("parameter `{path}` must be a scalar, found a distribution")]
    NotScalar { path: String },
}

/// The fully resolved input of one run.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResolvedSpec {
    /// The scenario with its overrides folded into `params`.
    pub scenario: Scenario,
    pub params: IndexMap<String, ResolvedParam>,
    /// Paths read through the accessors, to find register entries nothing uses.
    #[serde(skip)]
    accessed: Mutex<BTreeSet<String>>,
}

impl Clone for ResolvedSpec {
    fn clone(&self) -> Self {
        Self {
            scenario: self.scenario.clone(),
            params: self.params.clone(),
            accessed: Mutex::new(self.accessed.lock().map(|a| a.clone()).unwrap_or_default()),
        }
    }
}

/// Resolve `scenario` against `register`, applying the scenario's overrides and
/// then `experiment` overrides (both in register units).
pub fn resolve(
    register: &Register,
    scenario: &Scenario,
    experiment: &IndexMap<String, ParamValue>,
    opts: ResolveOptions,
) -> Result<ResolvedSpec, ResolveError> {
    let mut params: IndexMap<String, ResolvedParam> = register
        .iter()
        .map(|(path, e)| (path.clone(), base_param(e)))
        .collect();
    for (layer, overrides) in [
        (Layer::Scenario, &scenario.overrides),
        (Layer::Experiment, experiment),
    ] {
        for (path, value) in overrides {
            apply(register, &mut params, path, value, layer, opts)?;
        }
    }
    let mut scenario = scenario.clone();
    scenario.overrides.clear();
    Ok(ResolvedSpec {
        scenario,
        params,
        accessed: Mutex::new(BTreeSet::new()),
    })
}

fn base_param(e: &ParamEntry) -> ResolvedParam {
    ResolvedParam {
        value: e.value.clone(),
        si: e.value.as_ref().map(|v| canonical(v.to_si(e.unit))),
        unit: e.unit,
        grade: e.grade,
        role: e.role,
        stage: e.stage,
        source: e.source.clone(),
        guide: e.guide.clone(),
        set_by: Layer::Base,
        fix_overridden: false,
    }
}

fn canonical(v: ParamValue) -> ParamValue {
    match v {
        ParamValue::Dist(d) => ParamValue::Dist(d.canonical()),
        s => s,
    }
}

fn apply(
    register: &Register,
    params: &mut IndexMap<String, ResolvedParam>,
    path: &str,
    value: &ParamValue,
    layer: Layer,
    opts: ResolveOptions,
) -> Result<(), ResolveError> {
    let entry = register
        .get(path)
        .ok_or_else(|| ResolveError::Unknown(path.to_string()))?;
    entry
        .check_value(value)
        .map_err(|e| ResolveError::Invalid {
            path: path.to_string(),
            source: e,
        })?;
    let p = params.get_mut(path).expect("params mirror the register");
    if entry.role == Role::Fix {
        if !opts.allow_fix_override {
            return Err(ResolveError::FixOverride(path.to_string()));
        }
        p.fix_overridden = true;
    }
    p.value = Some(value.clone());
    p.si = Some(canonical(value.to_si(entry.unit)));
    p.set_by = layer;
    Ok(())
}

impl ResolvedSpec {
    pub fn get(&self, path: &str) -> Option<&ResolvedParam> {
        self.params.get(path)
    }

    fn mark(&self, path: &str) {
        if let Ok(mut a) = self.accessed.lock() {
            a.insert(path.to_string());
        }
    }

    /// The SI value of a parameter, recording the access.
    pub fn value_si(&self, path: &str) -> Result<&ParamValue, ResolveError> {
        self.mark(path);
        let p = self
            .params
            .get(path)
            .ok_or_else(|| ResolveError::Unknown(path.to_string()))?;
        p.si.as_ref().ok_or_else(|| ResolveError::Unset {
            path: path.to_string(),
            grade: p.grade.label(),
            role: p.role.label(),
        })
    }

    /// A scalar parameter in SI.
    pub fn scalar_si(&self, path: &str) -> Result<f64, ResolveError> {
        match self.value_si(path)? {
            ParamValue::Scalar(x) => Ok(*x),
            ParamValue::Dist(Dist::Constant { value }) => Ok(*value),
            ParamValue::Dist(_) => Err(ResolveError::NotScalar {
                path: path.to_string(),
            }),
        }
    }

    /// A parameter in SI as a (canonical) distribution; scalars become constants.
    pub fn dist_si(&self, path: &str) -> Result<Dist, ResolveError> {
        Ok(self.value_si(path)?.as_dist())
    }

    /// Whether a path exists (does not count as an access).
    pub fn contains(&self, path: &str) -> bool {
        self.params.contains_key(path)
    }

    /// Paths read so far through the accessors.
    pub fn accessed(&self) -> BTreeSet<String> {
        self.accessed.lock().map(|a| a.clone()).unwrap_or_default()
    }

    /// Register paths never read through the accessors.
    pub fn unused(&self) -> Vec<String> {
        let acc = self.accessed();
        self.params
            .keys()
            .filter(|k| !acc.contains(*k))
            .cloned()
            .collect()
    }

    /// Canonical JSON of what is simulated: the scenario and every parameter's
    /// SI value, with sorted keys and normalised floats.
    pub fn canonical_json(&self) -> String {
        let values: IndexMap<&String, &Option<ParamValue>> =
            self.params.iter().map(|(k, p)| (k, &p.si)).collect();
        let doc = serde_json::json!({ "scenario": self.scenario, "params": values });
        canonicalise(doc).to_string()
    }

    /// The run's identity: blake3 of the canonical spec, the seed and the
    /// simulator version (first 16 hex characters).
    pub fn run_hash(&self, seed: u64, version: &str) -> String {
        let mut h = blake3::Hasher::new();
        h.update(self.canonical_json().as_bytes());
        h.update(&seed.to_le_bytes());
        h.update(version.as_bytes());
        h.finalize().to_hex()[..16].to_string()
    }

    /// The resolved spec as YAML (for `spec.resolved.yaml`).
    pub fn to_yaml(&self) -> Result<String, serde_norway::Error> {
        serde_norway::to_string(self)
    }
}

/// Sort object keys recursively and turn `-0.0` into `0.0`.
fn canonicalise(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut items: Vec<(String, Value)> = map.into_iter().collect();
            items.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                items
                    .into_iter()
                    .map(|(k, v)| (k, canonicalise(v)))
                    .collect(),
            )
        }
        Value::Array(xs) => Value::Array(xs.into_iter().map(canonicalise).collect()),
        Value::Number(n) => match n.as_f64() {
            Some(x) if x == 0.0 && x.is_sign_negative() => serde_json::json!(0.0),
            _ => Value::Number(n),
        },
        other => other,
    }
}
