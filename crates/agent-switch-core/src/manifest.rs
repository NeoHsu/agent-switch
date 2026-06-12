use std::{collections::BTreeMap, fs, path::Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::TOOL_VERSION;

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
    let content = fs::read_to_string(path)?;
    let mut manifest: Manifest = serde_json::from_str(&content)?;
    if manifest.meta.tool.is_empty() {
        manifest.meta = ManifestMeta::default();
    }
    Ok(manifest)
}

pub fn save(path: &Path, manifest: &mut Manifest) -> Result<()> {
    manifest.meta = ManifestMeta::default();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(manifest)?;
    fs::write(path, format!("{text}\n"))?;
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
