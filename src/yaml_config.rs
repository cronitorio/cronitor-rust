//! `cronitor.yaml` helpers: read, validate, apply and generate.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::{Client, Error, PutOptions, Result};

/// Default config path used by [`Client::generate_config`] when none is set.
pub const DEFAULT_CONFIG_PATH: &str = "./cronitor.yaml";

/// Counts of monitors returned by [`Client::validate_config`] and
/// [`Client::apply_config`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigSummary {
    /// Number of monitors under the `jobs` key.
    pub jobs: usize,
    /// Number of monitors under the `checks` key.
    pub checks: usize,
    /// Number of monitors under the `heartbeats` key.
    pub heartbeats: usize,
}

impl ConfigSummary {
    /// Total number of monitors.
    pub fn total(&self) -> usize {
        self.jobs + self.checks + self.heartbeats
    }

    fn from_yaml(doc: &serde_yaml::Value) -> Self {
        let count = |key: &str| {
            doc.get(key)
                .and_then(|v| v.as_mapping())
                .map_or(0, |m| m.len())
        };
        Self {
            jobs: count("jobs"),
            checks: count("checks"),
            heartbeats: count("heartbeats"),
        }
    }
}

impl fmt::Display for ConfigSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} monitor{} ({} jobs, {} checks, {} heartbeats)",
            self.total(),
            if self.total() == 1 { "" } else { "s" },
            self.jobs,
            self.checks,
            self.heartbeats
        )
    }
}

impl Client {
    /// The path to `cronitor.yaml`, from [`crate::Config`], [`Client::read_config`]
    /// or [`Client::set_config_path`].
    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Set the path to `cronitor.yaml` used by the config helpers.
    pub fn set_config_path(&self, path: impl Into<PathBuf>) {
        *self.config_path.lock().unwrap_or_else(|e| e.into_inner()) = Some(path.into());
    }

    /// Parse the YAML config file at `path` and remember the path for
    /// [`Client::validate_config`], [`Client::apply_config`] and
    /// [`Client::generate_config`].
    pub fn read_config(&self, path: impl AsRef<Path>) -> Result<serde_yaml::Value> {
        let path = path.as_ref();
        self.set_config_path(path);
        read_yaml(path)
    }

    /// Send the config file to Cronitor for validation without applying it.
    pub fn validate_config(&self) -> Result<ConfigSummary> {
        self.sync_config(true)
    }

    /// Sync the monitors in the config file to Cronitor.
    pub fn apply_config(&self) -> Result<ConfigSummary> {
        self.sync_config(false)
    }

    /// Write the account's monitors (see [`Client::as_yaml`]) to the config
    /// path, defaulting to `./cronitor.yaml`. Returns the path written.
    pub fn generate_config(&self) -> Result<PathBuf> {
        let path = self
            .config_path()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
        let yaml = self.as_yaml()?;
        std::fs::write(&path, yaml)?;
        log::info!("wrote monitor config to {}", path.display());
        Ok(path)
    }

    fn sync_config(&self, rollback: bool) -> Result<ConfigSummary> {
        let path = self.config_path().ok_or_else(|| {
            Error::ConfigValidation(
                "Must set a path to config file e.g. Config::with_config_path(\"./cronitor.yaml\") or Client::read_config"
                    .to_string(),
            )
        })?;
        let doc = read_yaml(&path)?;
        let result = self.put_yaml(&doc, PutOptions::new().rollback(rollback))?;
        let summary = ConfigSummary::from_yaml(&result);
        log::info!(
            "{} {}",
            summary,
            if rollback { "validated." } else { "synced." }
        );
        Ok(summary)
    }
}

fn read_yaml(path: &Path) -> Result<serde_yaml::Value> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        Error::ConfigValidation(format!(
            "Could not read config file {}: {err}",
            path.display()
        ))
    })?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&text)?;
    if !doc.is_mapping() {
        return Err(Error::ConfigValidation(format!(
            "Config file {} must be a mapping with jobs/checks/heartbeats keys",
            path.display()
        )));
    }
    Ok(doc)
}
