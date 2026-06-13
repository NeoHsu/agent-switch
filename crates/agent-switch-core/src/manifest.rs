use std::{collections::BTreeMap, path::Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    Error, TOOL_VERSION,
    fs::{atomic_write, io_error, read_text},
};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub generated: BTreeMap<String, GeneratedEntry>,
    #[serde(default)]
    pub links: BTreeMap<String, String>,
    #[serde(default)]
    pub meta: ManifestMeta,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneratedEntry {
    pub hash: String,
    pub src: String,
    pub src_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestMeta {
    pub version: u32,
    pub tool: String,
    pub tool_version: String,
}

impl Default for ManifestMeta {
    fn default() -> Self {
        Self {
            version: 1,
            tool: "agent-switch".into(),
            tool_version: TOOL_VERSION.into(),
        }
    }
}

pub fn load(path: &Path) -> Result<Manifest> {
    if !path.exists() {
        return Ok(Manifest::default());
    }
    let content = read_text(path).map_err(|err| io_error("read manifest", path, err))?;
    let mut manifest: Manifest = serde_json::from_str(&content).map_err(|err| {
        Error::Config(format!(
            "manifest is not parseable: {}: {err}. Run `ags sync --reset-manifest` to rebuild it.",
            path.display()
        ))
    })?;
    if manifest.meta.tool.is_empty() {
        manifest.meta = ManifestMeta::default();
    }
    Ok(manifest)
}

pub fn save(path: &Path, manifest: &mut Manifest) -> Result<()> {
    manifest.meta = ManifestMeta::default();
    let text = serde_json::to_string_pretty(manifest)?;
    let text = format!("{text}\n");
    atomic_write(path, text.as_bytes())?;
    Ok(())
}

pub fn sha256_text(text: &str) -> String {
    sha256_bytes(text.as_bytes())
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
