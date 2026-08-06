//! Typed representation of the canonical MCP document.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Error;

/// Canonical MCP configuration shared by all native-tool adapters.
///
/// Server-specific fields intentionally remain JSON values: each adapter owns
/// the native key mapping, while this boundary guarantees that the document is
/// an object with an object-valued `mcpServers` field and preserves extra
/// top-level metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct CanonicalMcpConfig {
    #[serde(rename = "mcpServers", default)]
    pub(super) servers: Map<String, Value>,
    #[serde(flatten)]
    pub(super) extra: Map<String, Value>,
}

impl CanonicalMcpConfig {
    pub(super) fn from_json(text: &str) -> Result<Self> {
        let value = serde_json::from_str(text).map_err(|err| {
            Error::Config(format!("canonical MCP config is not valid JSON: {err}"))
        })?;
        Self::from_value(value)
    }

    pub(super) fn from_value(value: Value) -> Result<Self> {
        serde_json::from_value(value).map_err(|err| {
            Error::Config(format!(
                "canonical MCP config must contain an object-valued `mcpServers`: {err}"
            ))
            .into()
        })
    }
}
