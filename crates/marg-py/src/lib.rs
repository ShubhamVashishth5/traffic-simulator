//! Python extension module `marg._core`.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::path::PathBuf;

/// Version of the simulator core.
#[pyfunction]
fn version() -> &'static str {
    marg_core::VERSION
}

/// Run a scenario given as YAML text and write the run folder to `out`.
/// Returns the manifest as a JSON string.
#[pyfunction]
#[pyo3(signature = (scenario_yaml, seed, out, threads = 1))]
fn run_yaml(scenario_yaml: &str, seed: u64, out: PathBuf, threads: usize) -> PyResult<String> {
    let manifest = marg_io::run::run_scenario_text(scenario_yaml, seed, threads, &out)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(marg_io::manifest::to_json(&manifest))
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(run_yaml, m)?)?;
    Ok(())
}
