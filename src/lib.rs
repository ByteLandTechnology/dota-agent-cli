//! Shared library for the dota-agent-cli encyclopedia skill.

pub mod context;
pub mod encyclopedia;
pub mod help;
pub mod match_commands;
pub mod providers;
pub mod repl;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;

/// Output format for structured stdout/stderr payloads.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Yaml,
    Json,
    Toml,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Json => "json",
            Self::Toml => "toml",
        }
    }
}

/// Shared daemon lifecycle states for the cli-forge compatibility surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonLifecycleState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl DaemonLifecycleState {
    pub fn as_recommended_action(&self) -> &'static str {
        match self {
            Self::Stopped => "start",
            Self::Starting | Self::Stopping | Self::Running => "status",
            Self::Failed => "restart",
        }
    }
}

/// Stable structured error payload used by leaf commands.
#[derive(Debug, Clone, Serialize)]
pub struct StructuredError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, String>>,
    pub source: String,
    pub format: String,
}

impl StructuredError {
    pub fn new(code: &str, message: impl Into<String>, source: &str, format: Format) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            details: None,
            source: source.to_string(),
            format: format.as_str().to_string(),
        }
    }

    pub fn with_detail(mut self, key: &str, value: impl Into<String>) -> Self {
        self.details
            .get_or_insert_with(BTreeMap::new)
            .insert(key.to_string(), value.into());
        self
    }
}

/// Builder-style structured error context independent of the output format.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    code: String,
    message: String,
    source: String,
    details: BTreeMap<String, String>,
}

impl ErrorContext {
    pub fn new(code: &str, message: impl Into<String>, source: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            source: source.to_string(),
            details: BTreeMap::new(),
        }
    }

    pub fn with_detail(mut self, key: &str, value: impl Into<String>) -> Self {
        self.details.insert(key.to_string(), value.into());
        self
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn into_structured(self, format: Format) -> StructuredError {
        StructuredError {
            code: self.code,
            message: self.message,
            details: (!self.details.is_empty()).then_some(self.details),
            source: self.source,
            format: format.as_str().to_string(),
        }
    }
}

/// Structured daemon status payload returned by the daemon contract.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatusOutput {
    pub state: DaemonLifecycleState,
    pub readiness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub recommended_next_action: String,
    pub instance_model: String,
    pub instance_id: String,
}

/// Structured daemon control payload returned by lifecycle commands.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonCommandOutput {
    pub action: String,
    pub result: String,
    pub state: DaemonLifecycleState,
    pub message: String,
    pub recommended_next_action: String,
    pub instance_model: String,
    pub instance_id: String,
}

/// Serialize a value to the requested format.
pub fn serialize_value<W: Write, T: Serialize>(
    writer: &mut W,
    value: &T,
    format: Format,
) -> Result<()> {
    match format {
        Format::Yaml => {
            let serialized = serde_yaml::to_string(value).context("failed to serialize as YAML")?;
            writer.write_all(serialized.as_bytes())?;
        }
        Format::Json => {
            serde_json::to_writer_pretty(&mut *writer, value)
                .context("failed to serialize as JSON")?;
            writeln!(writer)?;
        }
        Format::Toml => {
            let serialized =
                toml::to_string_pretty(value).context("failed to serialize as TOML")?;
            writer.write_all(serialized.as_bytes())?;
            writeln!(writer)?;
        }
    }

    Ok(())
}

/// Serialize a structured error using the selected output format.
pub fn write_structured_error<W: Write>(
    writer: &mut W,
    error: &StructuredError,
    format: Format,
) -> Result<()> {
    serialize_value(writer, error, format)
}
