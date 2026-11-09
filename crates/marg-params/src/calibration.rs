//! Calibration stage files: `calibration/stage_<k>.yaml` (plan P3.8, G§8.3).
//!
//! A stage file holds the values fitted in stage `k`. Only `frozen` files are
//! applied, as the `calibration` layer between the register and the scenario.
//! The freeze rule is enforced here as well as in the Python API:
//!
//! - a stage file may only set parameters with `role: fit` and `stage: k`;
//! - frozen stages must form an unbroken sequence from the first stage that has
//!   `fit` parameters in the register (stage `k` cannot be frozen on top of an
//!   unfrozen stage `k − 1`).

use crate::param::{ParamValue, Role};
use crate::register::Register;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// Whether a stage is still being fitted or has been frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Draft,
    Frozen,
}

/// One fitted parameter. The Python API writes more fields (estimates,
/// intervals, diagnostics); the core reads only the value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibratedParam {
    /// The fitted value, in the register entry's unit.
    pub value: ParamValue,
}

/// The parts of a stage file the core needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageFile {
    pub stage: u8,
    pub status: StageStatus,
    pub params: IndexMap<String, CalibratedParam>,
}

/// Errors loading calibration stages.
#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Yaml {
        path: String,
        source: serde_norway::Error,
    },
    #[error("{path}: file name says stage {named}, contents say stage {inside}")]
    StageMismatch { path: String, named: u8, inside: u8 },
    #[error("stage {stage}: `{param}` is not in the register")]
    Unknown { stage: u8, param: String },
    #[error(
        "stage {stage} may only set `fit` parameters of stage {stage}; `{param}` is {role}{}",
        .param_stage.map(|s| format!(" (stage {s})")).unwrap_or_default()
    )]
    WrongStage {
        stage: u8,
        param: String,
        role: &'static str,
        param_stage: Option<u8>,
    },
    #[error("stage {stage}: `{param}`: {message}")]
    Invalid {
        stage: u8,
        param: String,
        message: String,
    },
    #[error("stage {frozen} is frozen but stage {missing} is not: stages are frozen in order")]
    Gap { frozen: u8, missing: u8 },
}

/// The frozen calibration stages that apply to a run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Calibration {
    /// Fitted values by path, in stage order (register units).
    pub values: IndexMap<String, ParamValue>,
    /// The frozen stages applied.
    pub stages: Vec<u8>,
}

impl Calibration {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Load every `stage_<k>.yaml` in `dir` and keep the frozen ones.
    pub fn load_dir(dir: &Path, register: &Register) -> Result<Self, CalibrationError> {
        let io = |source| CalibrationError::Io {
            path: dir.display().to_string(),
            source,
        };
        let mut files = Vec::new();
        for item in std::fs::read_dir(dir).map_err(io)? {
            let path = item.map_err(io)?.path();
            let Some(named) = stage_of_file_name(&path) else {
                continue;
            };
            let text = std::fs::read_to_string(&path).map_err(|source| CalibrationError::Io {
                path: path.display().to_string(),
                source,
            })?;
            let file: StageFile =
                serde_norway::from_str(&text).map_err(|source| CalibrationError::Yaml {
                    path: path.display().to_string(),
                    source,
                })?;
            if file.stage != named {
                return Err(CalibrationError::StageMismatch {
                    path: path.display().to_string(),
                    named,
                    inside: file.stage,
                });
            }
            files.push(file);
        }
        Self::from_files(files, register)
    }

    /// Check and combine stage files (any order).
    pub fn from_files(
        mut files: Vec<StageFile>,
        register: &Register,
    ) -> Result<Self, CalibrationError> {
        files.sort_by_key(|f| f.stage);
        for f in &files {
            check_stage(f, register)?;
        }
        let frozen: BTreeSet<u8> = files
            .iter()
            .filter(|f| f.status == StageStatus::Frozen)
            .map(|f| f.stage)
            .collect();
        if let Some(&last) = frozen.iter().next_back() {
            for s in fit_stages(register).into_iter().filter(|&s| s < last) {
                if !frozen.contains(&s) {
                    return Err(CalibrationError::Gap {
                        frozen: last,
                        missing: s,
                    });
                }
            }
        }
        let mut out = Calibration::default();
        for f in files
            .into_iter()
            .filter(|f| f.status == StageStatus::Frozen)
        {
            out.stages.push(f.stage);
            for (path, p) in f.params {
                out.values.insert(path, p.value);
            }
        }
        Ok(out)
    }
}

/// The stages that have `fit` parameters in the register.
pub fn fit_stages(register: &Register) -> BTreeSet<u8> {
    register
        .iter()
        .filter(|(_, e)| e.role == Role::Fit)
        .filter_map(|(_, e)| e.stage)
        .collect()
}

fn stage_of_file_name(path: &Path) -> Option<u8> {
    let name = path.file_name()?.to_str()?;
    name.strip_prefix("stage_")?
        .strip_suffix(".yaml")?
        .parse()
        .ok()
}

fn check_stage(f: &StageFile, register: &Register) -> Result<(), CalibrationError> {
    for (path, p) in &f.params {
        let e = register
            .get(path)
            .ok_or_else(|| CalibrationError::Unknown {
                stage: f.stage,
                param: path.clone(),
            })?;
        if e.role != Role::Fit || e.stage != Some(f.stage) {
            return Err(CalibrationError::WrongStage {
                stage: f.stage,
                param: path.clone(),
                role: e.role.label(),
                param_stage: e.stage,
            });
        }
        e.check_value(&p.value)
            .map_err(|err| CalibrationError::Invalid {
                stage: f.stage,
                param: path.clone(),
                message: err.to_string(),
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REG: &str = r#"
group: t
entries:
  speed:
    value: 10
    unit: "1"
    grade: A
    role: fit
    stage: 1
    source: test
    guide: "§0"
    range: [0, 100]
  gap:
    value: 1
    unit: "1"
    grade: A
    role: fit
    stage: 2
    source: test
    guide: "§0"
  size:
    value: 2
    unit: "1"
    grade: A
    role: fix
    source: test
    guide: "§0"
"#;

    fn reg() -> Register {
        let mut r = Register::default();
        r.add_yaml(REG, "test").unwrap();
        r
    }

    fn file(stage: u8, status: StageStatus, params: &[(&str, f64)]) -> StageFile {
        StageFile {
            stage,
            status,
            params: params
                .iter()
                .map(|(p, v)| {
                    (
                        p.to_string(),
                        CalibratedParam {
                            value: ParamValue::Scalar(*v),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn frozen_stages_apply_drafts_do_not() {
        let c = Calibration::from_files(
            vec![
                file(2, StageStatus::Draft, &[("t.gap", 1.5)]),
                file(1, StageStatus::Frozen, &[("t.speed", 12.0)]),
            ],
            &reg(),
        )
        .unwrap();
        assert_eq!(c.stages, vec![1]);
        assert_eq!(c.values.get("t.speed"), Some(&ParamValue::Scalar(12.0)));
        assert!(!c.values.contains_key("t.gap"));
    }

    #[test]
    fn a_stage_may_only_set_its_own_fit_parameters() {
        let r = reg();
        let wrong =
            Calibration::from_files(vec![file(2, StageStatus::Draft, &[("t.speed", 12.0)])], &r);
        assert!(matches!(wrong, Err(CalibrationError::WrongStage { .. })));
        let fix =
            Calibration::from_files(vec![file(1, StageStatus::Draft, &[("t.size", 3.0)])], &r);
        assert!(matches!(fix, Err(CalibrationError::WrongStage { .. })));
        let range =
            Calibration::from_files(vec![file(1, StageStatus::Draft, &[("t.speed", 500.0)])], &r);
        assert!(matches!(range, Err(CalibrationError::Invalid { .. })));
    }

    #[test]
    fn stages_freeze_in_order() {
        let gap = Calibration::from_files(
            vec![
                file(1, StageStatus::Draft, &[("t.speed", 12.0)]),
                file(2, StageStatus::Frozen, &[("t.gap", 1.5)]),
            ],
            &reg(),
        );
        assert!(matches!(
            gap,
            Err(CalibrationError::Gap {
                frozen: 2,
                missing: 1
            })
        ));
    }
}
