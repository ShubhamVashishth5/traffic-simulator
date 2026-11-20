//! The parameter register: every number the model uses, loaded from
//! `params/base/*.yaml`.

use crate::param::{EntryError, Grade, ParamEntry, ParamValue, Role};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The register directory's folder of presets.
pub const PRESETS_DIR: &str = "presets";

/// `*.yaml` and `*.yml` files directly in `dir`, sorted by name.
fn yaml_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, RegisterError> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| RegisterError::Io {
            path: dir.display().to_string(),
            source: e,
        })?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
        .collect();
    files.sort();
    Ok(files)
}

/// One register file: a group of entries sharing a path prefix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterFile {
    /// Path prefix of every entry in the file, e.g. `interaction`.
    pub group: String,
    #[serde(default)]
    pub description: String,
    /// Entries keyed by their path below the group, e.g. `following.time_gap.car`.
    pub entries: IndexMap<String, ParamEntry>,
}

/// A preset (`params/base/presets/<name>.yaml`): a named set of values a
/// scenario applies with `presets: [<name>]`, as the `preset` layer between
/// calibration and the scenario's own overrides (sandbox track S3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    /// The name scenarios use; must match the file name.
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Values by register path, in the register entry's unit.
    pub values: IndexMap<String, ParamValue>,
}

/// Errors loading or querying the register.
#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
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
    #[error("duplicate parameter `{0}`")]
    Duplicate(String),
    #[error("parameter `{path}`: {source}")]
    Entry { path: String, source: EntryError },
    #[error("no register files in {0}")]
    Empty(String),
    #[error("preset {path}: {message}")]
    Preset { path: String, message: String },
}

/// All register entries keyed by full path (`group.key`), in file order.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Register {
    entries: IndexMap<String, ParamEntry>,
    /// Presets by name (the `presets/` folder of the register directory).
    #[serde(skip)]
    presets: IndexMap<String, Preset>,
}

/// Filter for [`Register::filter`].
#[derive(Debug, Clone, Default)]
pub struct RegisterFilter {
    pub grade: Option<Grade>,
    pub role: Option<Role>,
    pub stage: Option<u8>,
    pub prefix: Option<String>,
}

impl Register {
    /// Load every `*.yaml` file in `dir`, in file-name order, and the
    /// presets in `dir/presets/`, if there is such a folder.
    pub fn load_dir(dir: &Path) -> Result<Self, RegisterError> {
        let mut reg = Register::default();
        for file in Self::load_files(dir)? {
            reg.add_file(file)?;
        }
        let presets = dir.join(PRESETS_DIR);
        if presets.is_dir() {
            for path in yaml_files(&presets)? {
                let origin = path.display().to_string();
                let text = std::fs::read_to_string(&path).map_err(|e| RegisterError::Io {
                    path: origin.clone(),
                    source: e,
                })?;
                let preset: Preset =
                    serde_norway::from_str(&text).map_err(|e| RegisterError::Yaml {
                        path: origin.clone(),
                        source: e,
                    })?;
                if path.file_stem().and_then(|s| s.to_str()) != Some(preset.name.as_str()) {
                    return Err(RegisterError::Preset {
                        path: origin,
                        message: format!("name `{}` must match the file name", preset.name),
                    });
                }
                reg.add_preset(preset)
                    .map_err(|message| RegisterError::Preset {
                        path: origin,
                        message,
                    })?;
            }
        }
        Ok(reg)
    }

    /// Add a preset, checking every value against its register entry.
    pub fn add_preset(&mut self, preset: Preset) -> Result<(), String> {
        for (path, value) in &preset.values {
            let entry = self
                .entries
                .get(path)
                .ok_or_else(|| format!("unknown parameter `{path}`"))?;
            entry
                .check_value(value)
                .map_err(|e| format!("parameter `{path}`: {e}"))?;
        }
        if self.presets.contains_key(&preset.name) {
            return Err(format!("duplicate preset `{}`", preset.name));
        }
        self.presets.insert(preset.name.clone(), preset);
        Ok(())
    }

    /// The preset called `name`.
    pub fn preset(&self, name: &str) -> Option<&Preset> {
        self.presets.get(name)
    }

    /// Every preset, in file-name order.
    pub fn presets(&self) -> impl Iterator<Item = &Preset> {
        self.presets.values()
    }

    /// Parse every `*.yaml` file in `dir`, in file-name order, keeping the
    /// grouping (the live viewer's parameter panel is grouped like this).
    pub fn load_files(dir: &Path) -> Result<Vec<RegisterFile>, RegisterError> {
        let io = |e| RegisterError::Io {
            path: dir.display().to_string(),
            source: e,
        };
        let files = yaml_files(dir).map_err(|e| match e {
            RegisterError::Io { source, .. } => io(source),
            e => e,
        })?;
        if files.is_empty() {
            return Err(RegisterError::Empty(dir.display().to_string()));
        }
        files
            .iter()
            .map(|path| {
                let origin = path.display().to_string();
                let text = std::fs::read_to_string(path).map_err(|e| RegisterError::Io {
                    path: origin.clone(),
                    source: e,
                })?;
                serde_norway::from_str(&text).map_err(|e| RegisterError::Yaml {
                    path: origin,
                    source: e,
                })
            })
            .collect()
    }

    /// Add the entries of one register file given as YAML text.
    pub fn add_yaml(&mut self, text: &str, origin: &str) -> Result<(), RegisterError> {
        let file: RegisterFile = serde_norway::from_str(text).map_err(|e| RegisterError::Yaml {
            path: origin.to_string(),
            source: e,
        })?;
        self.add_file(file)
    }

    /// Add the entries of a parsed register file.
    pub fn add_file(&mut self, file: RegisterFile) -> Result<(), RegisterError> {
        for (key, entry) in file.entries {
            let path = format!("{}.{}", file.group, key);
            entry.validate().map_err(|e| RegisterError::Entry {
                path: path.clone(),
                source: e,
            })?;
            if self.entries.contains_key(&path) {
                return Err(RegisterError::Duplicate(path));
            }
            self.entries.insert(path, entry);
        }
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&ParamEntry> {
        self.entries.get(path)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All entries in load order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ParamEntry)> {
        self.entries.iter()
    }

    /// Entries matching every set field of `f`.
    pub fn filter<'a>(
        &'a self,
        f: &'a RegisterFilter,
    ) -> impl Iterator<Item = (&'a String, &'a ParamEntry)> {
        self.entries.iter().filter(move |(path, e)| {
            f.grade.is_none_or(|g| e.grade == g)
                && f.role.is_none_or(|r| e.role == r)
                && f.stage.is_none_or(|s| e.stage == Some(s))
                && f.prefix.as_deref().is_none_or(|p| path.starts_with(p))
        })
    }
}
