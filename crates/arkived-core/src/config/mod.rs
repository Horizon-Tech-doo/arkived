//! Preferences-only TOML config. Zero credentials or connection metadata.

use serde::{Deserialize, Serialize};
use crate::Error;
use std::path::Path;

/// User/project-level preferences. Connection metadata lives in the
/// [`Store`](crate::store::Store), never here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArkivedConfig {
    /// Default CLI output format when `--format` is not given.
    pub default_format: OutputFormat,
    /// Default log level — parsed as `tracing_subscriber::EnvFilter`.
    pub default_log_level: String,
    /// Default Azure environment (public, china, usgov, custom).
    pub default_environment: String,
    /// Default policy confirmation mode for destructive actions.
    pub default_confirm: ConfirmMode,
}

impl Default for ArkivedConfig {
    fn default() -> Self {
        Self {
            default_format: OutputFormat::Table,
            default_log_level: "info".into(),
            default_environment: "azure".into(),
            default_confirm: ConfirmMode::Ask,
        }
    }
}

/// CLI output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// JSON — stable schema for machine consumers.
    Json,
    /// YAML — human-friendly machine format.
    Yaml,
    /// Table — default for TTY output.
    Table,
    /// Tab-separated values — for `awk`/`sort`/`uniq` pipelines.
    Tsv,
}

/// Policy confirmation mode for non-interactive / scripted runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmMode {
    /// Prompt interactively (default).
    Ask,
    /// Deny every destructive action unless explicitly allowed per-invocation.
    Auto,
    /// Approve all destructive actions without prompting (loud-logged; dangerous).
    Yes,
}

impl ArkivedConfig {
    /// Parse a config from a TOML string.
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

impl ArkivedConfig {
    /// Discovery order: project (`<project>/.arkived.toml`) → user (`<user>/config.toml`) → defaults.
    /// Missing files are silently skipped.
    pub fn discover(project_dir: Option<&Path>, user_dir: Option<&Path>) -> Result<Self, Error> {
        if let Some(p) = project_dir {
            let path = p.join(".arkived.toml");
            if path.exists() {
                let s = std::fs::read_to_string(&path)?;
                return Self::from_toml_str(&s).map_err(|e| Error::Other(e.into()));
            }
        }
        if let Some(u) = user_dir {
            let path = u.join("config.toml");
            if path.exists() {
                let s = std::fs::read_to_string(&path)?;
                return Self::from_toml_str(&s).map_err(|e| Error::Other(e.into()));
            }
        }
        Ok(Self::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let c = ArkivedConfig::default();
        assert_eq!(c.default_format, OutputFormat::Table);
        assert_eq!(c.default_log_level, "info");
        assert_eq!(c.default_environment, "azure");
        assert_eq!(c.default_confirm, ConfirmMode::Ask);
    }

    #[test]
    fn empty_toml_yields_defaults() {
        let c = ArkivedConfig::from_toml_str("").unwrap();
        assert_eq!(c, ArkivedConfig::default());
    }

    #[test]
    fn partial_override_preserves_unset_defaults() {
        let c = ArkivedConfig::from_toml_str(r#"default_format = "json""#).unwrap();
        assert_eq!(c.default_format, OutputFormat::Json);
        assert_eq!(c.default_log_level, "info");
    }

    #[test]
    fn full_toml_roundtrip() {
        let src = r#"
default_format = "yaml"
default_log_level = "debug"
default_environment = "china"
default_confirm = "auto"
"#;
        let c = ArkivedConfig::from_toml_str(src).unwrap();
        assert_eq!(c.default_format, OutputFormat::Yaml);
        assert_eq!(c.default_log_level, "debug");
        assert_eq!(c.default_environment, "china");
        assert_eq!(c.default_confirm, ConfirmMode::Auto);
    }

    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn discover_finds_project_file_first() {
        let project = tempdir().unwrap();
        let user = tempdir().unwrap();
        fs::write(project.path().join(".arkived.toml"), r#"default_format = "json""#).unwrap();
        fs::write(
            user.path().join("config.toml"),
            r#"default_format = "tsv""#,
        ).unwrap();

        let c = ArkivedConfig::discover(Some(project.path()), Some(user.path())).unwrap();
        assert_eq!(c.default_format, OutputFormat::Json);
    }

    #[test]
    fn discover_falls_back_to_user_then_defaults() {
        let user = tempdir().unwrap();
        fs::write(user.path().join("config.toml"), r#"default_log_level = "trace""#).unwrap();

        let c = ArkivedConfig::discover(None, Some(user.path())).unwrap();
        assert_eq!(c.default_log_level, "trace");
        assert_eq!(c.default_format, OutputFormat::Table);

        let c = ArkivedConfig::discover(None, None).unwrap();
        assert_eq!(c, ArkivedConfig::default());
    }
}
