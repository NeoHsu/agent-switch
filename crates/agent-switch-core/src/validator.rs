//! Static validation for `.agent-switch.yaml` mappings.

use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

use anyhow::Result;

use crate::{
    Error,
    config::{Config, SymlinkSpec},
    fs::repo_path,
    tool::Tool,
};

pub fn validate_config(cfg: &Config) -> Result<()> {
    if cfg.version != 1 {
        return Err(
            Error::Unsupported(format!("unsupported config version: {}", cfg.version)).into(),
        );
    }

    validate_repo_path("agents_dir", &cfg.agents_dir)?;
    validate_repo_path("manifest", &cfg.manifest)?;

    let mut generate_outputs = BTreeSet::new();
    for (id, spec) in &cfg.generate {
        validate_id("generate spec", id)?;
        validate_repo_path(&format!("generate spec '{id}' from"), &spec.from)?;
        validate_repo_path(&format!("generate spec '{id}' to"), &spec.to)?;
        validate_tool_selection(
            &format!("generate spec '{id}'"),
            spec.tool,
            spec.tools.as_deref(),
        )?;
        if let Some(suffix) = &spec.suffix {
            validate_suffix(&format!("generate spec '{id}' suffix"), suffix)?;
        }
        let output = repo_path(&spec.to);
        if !generate_outputs.insert(output.clone()) {
            return Err(Error::Config(format!(
                "generate spec '{id}': duplicate output directory: {output}"
            ))
            .into());
        }
    }

    for (link, spec) in &cfg.symlinks {
        let link_path = Path::new(link);
        validate_repo_path(&format!("symlink '{link}' path"), link_path)?;
        validate_repo_path(&format!("symlink '{link}' target"), spec.target())?;
        if repo_path(link_path) == repo_path(spec.target()) {
            return Err(Error::Config(format!(
                "symlink '{link}': link path and target must be different"
            ))
            .into());
        }
        if let SymlinkSpec::Detailed(detail) = spec {
            validate_tool_selection(
                &format!("symlink '{link}'"),
                detail.tool,
                detail.tools.as_deref(),
            )?;
        }
    }

    for (id, spec) in &cfg.merge {
        validate_id("merge spec", id)?;
        validate_repo_path(&format!("merge spec '{id}' to"), &spec.to)?;
        validate_tool_selection(
            &format!("merge spec '{id}'"),
            spec.tool,
            spec.tools.as_deref(),
        )?;
    }
    Ok(())
}

fn validate_id(kind: &str, id: &str) -> Result<()> {
    if id.trim().is_empty() {
        return Err(Error::Config(format!("{kind} id cannot be empty")).into());
    }
    Ok(())
}

fn validate_repo_path(label: &str, path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(Error::Config(format!("{label}: path cannot be empty")).into());
    }
    if path.is_absolute() {
        return Err(Error::Config(format!(
            "{label}: path must be repository-relative: {}",
            path.display()
        ))
        .into());
    }
    let raw = path.to_string_lossy();
    if raw.contains('\\') {
        return Err(Error::Config(format!(
            "{label}: use forward slashes for portable paths: {}",
            path.display()
        ))
        .into());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {
                return Err(Error::Config(format!(
                    "{label}: path cannot contain `.` components: {}",
                    path.display()
                ))
                .into());
            }
            Component::ParentDir => {
                return Err(Error::Config(format!(
                    "{label}: path cannot contain `..` components: {}",
                    path.display()
                ))
                .into());
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(Error::Config(format!(
                    "{label}: path must be repository-relative: {}",
                    path.display()
                ))
                .into());
            }
        }
    }
    Ok(())
}

fn validate_suffix(label: &str, suffix: &str) -> Result<()> {
    if suffix.contains('/') || suffix.contains('\\') {
        return Err(Error::Config(format!(
            "{label}: suffix must be a file-name suffix, not a path: {suffix}"
        ))
        .into());
    }
    Ok(())
}

fn validate_tool_selection(label: &str, tool: Option<Tool>, tools: Option<&[Tool]>) -> Result<()> {
    if tool.is_some() && tools.is_some() {
        return Err(
            Error::Config(format!("{label}: use either `tool` or `tools`, not both")).into(),
        );
    }
    if let Some(tools) = tools {
        if tools.is_empty() {
            return Err(
                Error::Config(format!("{label}: `tools` must contain at least one tool")).into(),
            );
        }
        let mut seen = BTreeSet::new();
        for tool in tools {
            if !seen.insert(*tool) {
                return Err(
                    Error::Config(format!("{label}: duplicate tool `{tool}` in `tools`")).into(),
                );
            }
        }
    }
    Ok(())
}
