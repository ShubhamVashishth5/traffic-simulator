//! The parameter register: every number the model uses, loaded from
//! `params/base/*.yaml`.

use crate::param::{EntryError, Grade, ParamEntry, Role};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
}

/// All register entries keyed by full path (`group.key`), in file order.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Register {
    entries: IndexMap<String, ParamEntry>,
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
    /// Load every `*.yaml` file in `dir`, in file-name order.
    pub fn load_dir(dir: &Path) -> Result<Self, RegisterError> {
        let io = |e| RegisterError::Io {
            path: dir.display().to_string(),
            source: e,
        };
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(io)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
            .collect();
        files.sort();
        if files.is_empty() {
            return Err(RegisterError::Empty(dir.display().to_string()));
        }
        let mut reg = Register::default();
        for path in files {
            let text = std::fs::read_to_string(&path).map_err(|e| RegisterError::Io {
                path: path.display().to_string(),
                source: e,
            })?;
            reg.add_yaml(&text, &path.display().to_string())?;
        }
        Ok(reg)
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
