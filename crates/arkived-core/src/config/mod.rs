//! Preferences-only TOML config. Zero credentials or connection metadata.

use serde::{Deserialize, Serialize};

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
}
