//! Python extension module `marg._core`.

use indexmap::IndexMap;
use marg_params::{ParamValue, Scenario};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::path::PathBuf;

fn runtime<E: std::fmt::Display>(e: E) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

fn value_err<E: std::fmt::Display>(e: E) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Version of the simulator core.
#[pyfunction]
fn version() -> &'static str {
    marg_core::VERSION
}

/// Run a scenario given as YAML text and write the run folder to `out`.
/// Returns the manifest as a JSON string.
#[pyfunction]
#[pyo3(signature = (scenario_yaml, seed, out, threads = 1, params_dir = None, overrides_json = None, calibration_dir = None))]
#[allow(clippy::too_many_arguments)]
fn run_yaml(
    py: Python<'_>,
    scenario_yaml: &str,
    seed: u64,
    out: PathBuf,
    threads: usize,
    params_dir: Option<PathBuf>,
    overrides_json: Option<&str>,
    calibration_dir: Option<PathBuf>,
) -> PyResult<String> {
    let overrides = parse_overrides(overrides_json)?;
    let opts = marg_io::RunOptions {
        seed,
        threads,
        params_dir,
        overrides,
        calibration_dir,
        ..Default::default()
    };
    let text = scenario_yaml.to_string();
    let manifest = py
        .detach(move || marg_io::run::run_scenario_text(&text, &opts, &out))
        .map_err(runtime)?;
    Ok(marg_io::manifest::to_json(&manifest))
}

fn parse_overrides(json: Option<&str>) -> PyResult<IndexMap<String, ParamValue>> {
    match json {
        None => Ok(IndexMap::new()),
        Some(j) => serde_json::from_str(j).map_err(value_err),
    }
}

/// The register (and frozen calibration stages) loaded once, for expanding
/// experiments: run hashes and validation without running anything.
#[pyclass(frozen)]
struct Resolver {
    inputs: marg_io::Inputs,
}

#[pymethods]
impl Resolver {
    #[new]
    #[pyo3(signature = (params_dir = None, calibration_dir = None))]
    fn new(params_dir: Option<PathBuf>, calibration_dir: Option<PathBuf>) -> PyResult<Self> {
        let inputs = marg_io::Inputs::load(params_dir.as_deref(), calibration_dir.as_deref())
            .map_err(value_err)?;
        Ok(Self { inputs })
    }

    /// The register as JSON: `{path: entry}` in register order.
    fn register_json(&self) -> PyResult<String> {
        let map: IndexMap<&String, &marg_params::ParamEntry> =
            self.inputs.register.iter().collect();
        serde_json::to_string(&map).map_err(runtime)
    }

    /// Frozen calibration stages applied by this resolver.
    fn calibration_stages(&self) -> Vec<u8> {
        self.inputs.calibration.stages.clone()
    }

    /// The run hash of `scenario_yaml` with experiment overrides (JSON object,
    /// register units) and `seed`. Raises ValueError if the run is invalid
    /// (unknown path, out of range, `fix` override, …).
    #[pyo3(signature = (scenario_yaml, overrides_json, seed, allow_fix_override = false))]
    fn run_hash(
        &self,
        scenario_yaml: &str,
        overrides_json: &str,
        seed: u64,
        allow_fix_override: bool,
    ) -> PyResult<String> {
        let scenario = Scenario::from_yaml(scenario_yaml).map_err(value_err)?;
        let overrides = parse_overrides(Some(overrides_json))?;
        let spec = self
            .inputs
            .resolve(&scenario, &overrides, allow_fix_override)
            .map_err(value_err)?;
        Ok(spec.run_hash(seed, marg_core::VERSION))
    }

    /// The resolved parameters (SI, provenance) as JSON.
    #[pyo3(signature = (scenario_yaml, overrides_json = "{}", allow_fix_override = false))]
    fn resolve_json(
        &self,
        scenario_yaml: &str,
        overrides_json: &str,
        allow_fix_override: bool,
    ) -> PyResult<String> {
        let scenario = Scenario::from_yaml(scenario_yaml).map_err(value_err)?;
        let overrides = parse_overrides(Some(overrides_json))?;
        let spec = self
            .inputs
            .resolve(&scenario, &overrides, allow_fix_override)
            .map_err(value_err)?;
        serde_json::to_string(&spec.params).map_err(runtime)
    }
}

/// Mean of a parameter value given as JSON (a number or a distribution).
#[pyfunction]
fn value_mean(value_json: &str) -> PyResult<f64> {
    let v: ParamValue = serde_json::from_str(value_json).map_err(value_err)?;
    v.validate().map_err(value_err)?;
    Ok(v.mean())
}

/// A parameter value scaled by `factor` (every location and scale field of a
/// distribution, so its shape and coefficient of variation are kept), as JSON.
#[pyfunction]
fn value_scaled(value_json: &str, factor: f64) -> PyResult<String> {
    let v: ParamValue = serde_json::from_str(value_json).map_err(value_err)?;
    let out = match v {
        ParamValue::Scalar(x) => ParamValue::Scalar(x * factor),
        ParamValue::Dist(d) => ParamValue::Dist(d.scaled(factor)),
    };
    out.validate().map_err(value_err)?;
    serde_json::to_string(&out).map_err(runtime)
}

/// Quantile `p` of a distribution value given as JSON (None if the form has no
/// closed-form quantile).
#[pyfunction]
fn value_quantile(value_json: &str, p: f64) -> PyResult<Option<f64>> {
    let v: ParamValue = serde_json::from_str(value_json).map_err(value_err)?;
    Ok(v.as_dist().quantile(p))
}

/// Run a selection of an experiment's run list (see `marg batch`). Returns the
/// summary as JSON.
#[pyfunction]
#[pyo3(signature = (root, start = None, count = None, ids = None, threads = 1, force = false, params_dir = None))]
#[allow(clippy::too_many_arguments)]
fn run_batch(
    py: Python<'_>,
    root: PathBuf,
    start: Option<usize>,
    count: Option<usize>,
    ids: Option<Vec<u64>>,
    threads: usize,
    force: bool,
    params_dir: Option<PathBuf>,
) -> PyResult<String> {
    let selection = match (start, ids) {
        (Some(start), _) => marg_io::Selection::Range {
            start,
            count: count.unwrap_or(usize::MAX),
        },
        (None, Some(ids)) => marg_io::Selection::Ids(ids),
        (None, None) => marg_io::Selection::All,
    };
    let opts = marg_io::BatchOptions {
        params_dir,
        selection,
        threads,
        force,
        ..Default::default()
    };
    let summary = py
        .detach(move || marg_io::run_batch(&root, &opts))
        .map_err(runtime)?;
    serde_json::to_string(&summary).map_err(runtime)
}

/// Validate an experiment's runs and write `index.parquet`. Returns the report
/// as JSON.
#[pyfunction]
#[pyo3(signature = (root, clean_partials = false))]
fn collect(py: Python<'_>, root: PathBuf, clean_partials: bool) -> PyResult<String> {
    let report = py
        .detach(move || marg_io::collect(&root, clean_partials))
        .map_err(runtime)?;
    serde_json::to_string(&report).map_err(runtime)
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(run_yaml, m)?)?;
    m.add_function(wrap_pyfunction!(value_mean, m)?)?;
    m.add_function(wrap_pyfunction!(value_scaled, m)?)?;
    m.add_function(wrap_pyfunction!(value_quantile, m)?)?;
    m.add_function(wrap_pyfunction!(run_batch, m)?)?;
    m.add_function(wrap_pyfunction!(collect, m)?)?;
    m.add_class::<Resolver>()?;
    Ok(())
}
