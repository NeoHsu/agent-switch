//! Migration command implementation for importing existing native tool files.

use std::{
    collections::BTreeSet,
    fs as stdfs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::{Map as JsonMap, Value, json};
use walkdir::WalkDir;

use crate::{
    CommandOutput, Error, ExitCode,
    config::{self, Config, GenerateSpec, write_config},
    formats,
    fs::{atomic_write, io_error, is_fake_symlink, read_text, repo_path, write_if_changed},
    init, mcp,
    setup::{self, SetupOptions},
    tool::Tool,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct MigrateOptions {
    /// Report what would change without writing files.
    pub check: bool,
    /// Overwrite conflicting canonical files when safe merge is not possible.
    pub force: bool,
    /// Keep existing native files/directories in place, and skip automatic setup.
    pub keep_native: bool,
    /// Skip the final setup/sync pass after imports and backups.
    pub no_setup: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ImportOutcome {
    changed: bool,
    skipped: bool,
}

pub fn run(
    root: &Path,
    explicit_config: Option<&Path>,
    tools: Option<&[Tool]>,
    opts: MigrateOptions,
) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    let mut drift = false;
    let mut skipped = false;

    let (cfg, config_created) = ensure_config(root, explicit_config, tools, opts.check, &mut out)?;
    drift |= config_created;

    drift |= ensure_canonical_dirs(root, &cfg, opts.check, &mut out)?;

    let generated_outcome = import_generated_sources(root, &cfg, tools, opts, &mut out)?;
    drift |= generated_outcome.changed || generated_outcome.skipped;
    skipped |= generated_outcome.skipped;
    let mut native_paths_to_backup = BTreeSet::new();
    let symlink_outcome = import_symlink_sources(
        root,
        &cfg,
        tools,
        opts,
        &mut native_paths_to_backup,
        &mut out,
    )?;
    drift |= symlink_outcome.changed || symlink_outcome.skipped;
    skipped |= symlink_outcome.skipped;
    let merge_outcome = import_merge_sources(root, &cfg, tools, opts, &mut out)?;
    drift |= merge_outcome.changed || merge_outcome.skipped;
    skipped |= merge_outcome.skipped;

    if !opts.keep_native {
        drift |= backup_native_paths(root, &native_paths_to_backup, opts.check, &mut out)?;
    }

    if !opts.check {
        init::update_gitignore(root, &mut out)?;
    }

    if !opts.no_setup && !opts.keep_native {
        let setup_out = setup::run(
            root,
            &cfg,
            tools,
            SetupOptions {
                no_sync: false,
                check: opts.check,
                force: opts.force,
                prune: false,
            },
        )?;
        if setup_out.exit() == ExitCode::Drift {
            drift = true;
        }
        out.lines.extend(setup_out.lines);
        out.exit = setup_out.exit;
    }

    if opts.check {
        if drift {
            out.exit = Some(ExitCode::Drift);
        }
    } else if skipped && out.exit() != ExitCode::Drift {
        // Imports were left unreconciled (conflicts kept without --force); surface
        // this through the exit code even when the setup pass itself is clean.
        out.exit = Some(ExitCode::Drift);
    }

    Ok(out)
}

fn ensure_config(
    root: &Path,
    explicit_config: Option<&Path>,
    tools: Option<&[Tool]>,
    check: bool,
    out: &mut CommandOutput,
) -> Result<(Config, bool)> {
    let config_path = config::resolve_config_path(root, explicit_config);
    if config_path.exists() {
        let (cfg, _) = config::load_config(root, explicit_config)?;
        return Ok((cfg, false));
    }

    let cfg = filtered_default_config(tools);
    if !check {
        write_config(&config_path, &cfg, false)?;
    }
    out.push(format!("created  {}", display_path(root, &config_path)));
    Ok((cfg, true))
}

fn filtered_default_config(tools: Option<&[Tool]>) -> Config {
    let mut cfg = Config::default();
    if let Some(tools) = tools {
        cfg.symlinks
            .retain(|link, spec| config::symlink_selected(link, spec, Some(tools)));
        cfg.generate
            .retain(|_, spec| config::generate_selected(spec, Some(tools)));
        cfg.merge
            .retain(|id, spec| config::merge_selected(id, spec, Some(tools)));
    }
    cfg
}

fn ensure_canonical_dirs(
    root: &Path,
    cfg: &Config,
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let mut changed = false;
    for rel in [
        cfg.agents_dir.join("agents"),
        cfg.agents_dir.join("commands"),
        cfg.agents_dir.join("rules"),
        cfg.agents_dir.join("skills"),
    ] {
        let abs = root.join(&rel);
        if abs.exists() {
            continue;
        }
        changed = true;
        if !check {
            stdfs::create_dir_all(&abs).map_err(|err| io_error("create directory", &abs, err))?;
        }
        out.push(format!("created  {}", repo_path(&rel)));
    }
    Ok(changed)
}

fn import_symlink_sources(
    root: &Path,
    cfg: &Config,
    tools: Option<&[Tool]>,
    opts: MigrateOptions,
    native_paths_to_backup: &mut BTreeSet<PathBuf>,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    let mut outcome = ImportOutcome::default();
    for (link, spec) in &cfg.symlinks {
        if !config::symlink_selected(link, spec, tools) {
            continue;
        }
        let source_rel = Path::new(link);
        let target_rel = spec.target();
        let source_abs = root.join(source_rel);
        let target_abs = root.join(target_rel);

        if !path_exists_or_symlink(&source_abs) {
            continue;
        }
        if same_repo_path(source_rel, target_rel) {
            continue;
        }
        let target_cfg = spec.target_config();
        if is_managed_native_source(&source_abs, &target_abs, target_rel, &target_cfg) {
            // Native path is already managed by setup (symlink, fake symlink,
            // or managed copy). Re-importing/backing it up would only produce
            // a stray `.bak` copy, so leave it untouched.
            continue;
        }

        let metadata = stdfs::symlink_metadata(&source_abs)
            .map_err(|err| io_error("inspect native path", &source_abs, err))?;
        let file_type = metadata.file_type();

        if target_rel == cfg.agents_dir.join("mcp.json") && source_abs.is_file() {
            let source_outcome = merge_mcp_value(
                root,
                cfg,
                source_rel,
                read_json_mcp_file(&source_abs)?,
                opts.check,
                opts.force,
                out,
            )?;
            outcome.changed |= source_outcome.changed;
            outcome.skipped |= source_outcome.skipped;
            if !source_outcome.skipped {
                native_paths_to_backup.insert(source_rel.to_path_buf());
            }
            continue;
        }

        if file_type.is_dir() {
            let source_outcome =
                import_tree(source_rel, &source_abs, target_rel, &target_abs, opts, out)?;
            outcome.changed |= source_outcome.changed;
            outcome.skipped |= source_outcome.skipped;
            if source_outcome.skipped {
                // At least one file under this directory conflicted, so the
                // native directory is left in place (not backed up or linked).
                // Make the partial state explicit instead of silently leaving
                // duplicate sources behind.
                out.push(format!(
                    "skipped  {}: directory left in place; resolve conflicts or use --force",
                    repo_path(source_rel)
                ));
            } else {
                native_paths_to_backup.insert(source_rel.to_path_buf());
            }
        } else if file_type.is_file() || file_type.is_symlink() {
            if source_abs.is_file() {
                let source_outcome =
                    import_file_bytes(source_rel, &source_abs, target_rel, &target_abs, opts, out)?;
                outcome.changed |= source_outcome.changed;
                outcome.skipped |= source_outcome.skipped;
                if !source_outcome.skipped {
                    native_paths_to_backup.insert(source_rel.to_path_buf());
                }
            } else {
                out.push(format!(
                    "skipped  {}: native symlink target is not a file or directory",
                    repo_path(source_rel)
                ));
            }
        }
    }
    Ok(outcome)
}

fn import_generated_sources(
    root: &Path,
    cfg: &Config,
    tools: Option<&[Tool]>,
    opts: MigrateOptions,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    let mut outcome = ImportOutcome::default();
    for spec in cfg
        .generate
        .values()
        .filter(|spec| config::generate_selected(spec, tools))
    {
        let native_root = root.join(&spec.to);
        if !native_root.exists() {
            continue;
        }
        for entry in WalkDir::new(&native_root).min_depth(1) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let native_abs = entry.path();
            let rel_to_native_root = native_abs.strip_prefix(&native_root)?;
            if !spec.recursive && rel_to_native_root.components().count() > 1 {
                continue;
            }
            let Some(canonical_rel) = generated_canonical_path(spec, rel_to_native_root) else {
                continue;
            };
            let native_rel = spec.to.join(rel_to_native_root);
            let native_text = read_text(native_abs)
                .with_context(|| format!("failed to read {}", repo_path(&native_rel)))?;
            let imported = spec
                .format
                .import(&native_rel, &native_text)
                .with_context(|| {
                    format!("failed to import native file {}", repo_path(&native_rel))
                })?;
            let source_outcome = write_imported_markdown(
                root,
                &native_rel,
                &canonical_rel,
                imported,
                spec.format.tool(),
                opts,
                out,
            )?;
            outcome.changed |= source_outcome.changed;
            outcome.skipped |= source_outcome.skipped;
        }
    }
    Ok(outcome)
}

fn generated_canonical_path(spec: &GenerateSpec, rel_to_native_root: &Path) -> Option<PathBuf> {
    let rel_display = repo_path(rel_to_native_root);
    let suffix = spec.suffix.as_deref().unwrap_or_default();
    let rel = if suffix.is_empty() {
        // Drop only the file extension, preserving any parent directories so
        // recursive sources keep their structure instead of collapsing.
        let mut rel = rel_to_native_root.to_path_buf();
        if rel.extension().is_some() {
            rel.set_extension("md");
        }
        rel
    } else {
        PathBuf::from(format!("{}.md", rel_display.strip_suffix(suffix)?))
    };
    Some(spec.from.join(rel))
}

fn import_merge_sources(
    root: &Path,
    cfg: &Config,
    tools: Option<&[Tool]>,
    opts: MigrateOptions,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    let mut outcome = ImportOutcome::default();
    for (id, spec) in cfg
        .merge
        .iter()
        .filter(|(id, spec)| config::merge_selected(id, spec, tools))
    {
        let native_abs = root.join(&spec.to);
        let Some(imported) = mcp::import_native(spec.format, &native_abs)
            .with_context(|| format!("failed to import MCP config from {}", repo_path(&spec.to)))?
        else {
            continue;
        };
        let source_outcome =
            merge_mcp_value(root, cfg, &spec.to, imported, opts.check, opts.force, out)
                .with_context(|| format!("failed to merge MCP servers for {id}"))?;
        outcome.changed |= source_outcome.changed;
        outcome.skipped |= source_outcome.skipped;
    }
    Ok(outcome)
}

fn import_tree(
    source_rel: &Path,
    source_abs: &Path,
    target_rel: &Path,
    target_abs: &Path,
    opts: MigrateOptions,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    let mut outcome = ImportOutcome::default();
    for entry in WalkDir::new(source_abs).min_depth(1) {
        let entry = entry?;
        let rel_to_source = entry.path().strip_prefix(source_abs)?;
        let canonical_subpath = canonical_markdown_subpath(rel_to_source);
        let dest_abs = target_abs.join(&canonical_subpath);
        let dest_rel = target_rel.join(&canonical_subpath);

        if entry.file_type().is_dir() {
            if !dest_abs.exists() {
                outcome.changed = true;
                if !opts.check {
                    stdfs::create_dir_all(&dest_abs)
                        .map_err(|err| io_error("create directory", &dest_abs, err))?;
                }
            }
            continue;
        }

        if !entry.file_type().is_file() {
            outcome.skipped = true;
            out.push(format!(
                "skipped  {}: native symlink or special file is not imported",
                repo_path(&source_rel.join(rel_to_source))
            ));
            continue;
        }

        let file_outcome = import_file_bytes(
            &source_rel.join(rel_to_source),
            entry.path(),
            &dest_rel,
            &dest_abs,
            opts,
            out,
        )?;
        outcome.changed |= file_outcome.changed;
        outcome.skipped |= file_outcome.skipped;
    }
    Ok(outcome)
}

fn import_file_bytes(
    source_rel: &Path,
    source_abs: &Path,
    target_rel: &Path,
    target_abs: &Path,
    opts: MigrateOptions,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    let mut source =
        stdfs::read(source_abs).map_err(|err| io_error("read native file", source_abs, err))?;
    if let Some(normalized) = normalize_canonical_markdown(target_rel, &source)? {
        source = normalized.into_bytes();
    }
    match stdfs::read(target_abs) {
        Ok(existing) if existing == source => {
            out.push(format!("ok       {}", repo_path(target_rel)));
            Ok(ImportOutcome {
                changed: false,
                skipped: false,
            })
        }
        Ok(existing) if !opts.force && !is_starter_file(target_rel, &existing) => {
            out.push(format!(
                "skipped  {}: already exists; use --force to overwrite",
                repo_path(target_rel)
            ));
            Ok(ImportOutcome {
                changed: false,
                skipped: true,
            })
        }
        Ok(_) => write_file_bytes(source_rel, target_rel, target_abs, &source, opts, out),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            write_file_bytes(source_rel, target_rel, target_abs, &source, opts, out)
        }
        Err(err) => Err(io_error("read existing canonical file", target_abs, err)),
    }
}

fn canonical_markdown_subpath(rel_to_source: &Path) -> PathBuf {
    let mut rel = rel_to_source.to_path_buf();
    let Some(file_name) = rel.file_name().and_then(|name| name.to_str()) else {
        return rel;
    };
    for suffix in [".agent.md", ".prompt.md", ".instructions.md"] {
        if let Some(base) = file_name.strip_suffix(suffix) {
            rel.set_file_name(format!("{base}.md"));
            return rel;
        }
    }
    rel
}

fn normalize_canonical_markdown(target_rel: &Path, source: &[u8]) -> Result<Option<String>> {
    if target_rel
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("md"))
    {
        return Ok(None);
    }
    if !is_named_canonical_markdown(target_rel) {
        return Ok(None);
    }
    let Ok(text) = std::str::from_utf8(source) else {
        return Ok(None);
    };
    let mut doc = formats::markdown::parse(text)?;
    if formats::markdown::str_value(&doc.frontmatter, "name")
        .as_deref()
        .is_some_and(|name| !name.trim().is_empty())
    {
        return Ok(None);
    }
    let Some(name) = target_rel.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(None);
    };
    formats::markdown::set_string(&mut doc.frontmatter, "name", name);
    formats::markdown::render(doc.frontmatter, &doc.body)
        .map(ensure_trailing_newline)
        .map(Some)
}

fn is_named_canonical_markdown(path: &Path) -> bool {
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    components.windows(2).any(|window| {
        matches!(
            window,
            [".agents" | ".agent", "agents"] | [".agents" | ".agent", "commands"]
        )
    })
}

fn write_file_bytes(
    source_rel: &Path,
    target_rel: &Path,
    target_abs: &Path,
    source: &[u8],
    opts: MigrateOptions,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    if !opts.check {
        atomic_write(target_abs, source)?;
    }
    out.push(format!(
        "imported {} -> {}",
        repo_path(source_rel),
        repo_path(target_rel)
    ));
    Ok(ImportOutcome {
        changed: true,
        skipped: false,
    })
}

fn write_imported_markdown(
    root: &Path,
    source_rel: &Path,
    canonical_rel: &Path,
    imported: String,
    current_tool: Tool,
    opts: MigrateOptions,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    let canonical_abs = root.join(canonical_rel);
    let next = match read_text(&canonical_abs) {
        Ok(existing) => {
            if existing == imported {
                out.push(format!("ok       {}", repo_path(canonical_rel)));
                return Ok(ImportOutcome {
                    changed: false,
                    skipped: false,
                });
            }
            match merge_imported_markdown(&existing, &imported, current_tool, opts.force)? {
                Some(merged) => merged,
                None => {
                    out.push(format!(
                        "skipped  {}: body differs from existing canonical file; use --force to overwrite",
                        repo_path(canonical_rel)
                    ));
                    return Ok(ImportOutcome {
                        changed: false,
                        skipped: true,
                    });
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => imported,
        Err(err) => {
            return Err(io_error(
                "read existing canonical file",
                &canonical_abs,
                err,
            ));
        }
    };

    if !opts.check {
        write_if_changed(&canonical_abs, &next)?;
    }
    out.push(format!(
        "imported {} -> {}",
        repo_path(source_rel),
        repo_path(canonical_rel)
    ));
    Ok(ImportOutcome {
        changed: true,
        skipped: false,
    })
}

fn merge_imported_markdown(
    existing: &str,
    imported: &str,
    current_tool: Tool,
    force: bool,
) -> Result<Option<String>> {
    let existing_doc = formats::markdown::parse(existing)?;
    let imported_doc = formats::markdown::parse(imported)?;
    let existing_body = existing_doc.body.trim();
    let imported_body = imported_doc.body.trim();
    if !force
        && !existing_body.is_empty()
        && !imported_body.is_empty()
        && existing_body != imported_body
    {
        return Ok(None);
    }

    let mut fm = imported_doc.frontmatter;
    // OpenCode file names are the only source of the imported `name`, so an
    // explicit name in the existing canonical file must win over the stem.
    let existing_name = (current_tool == Tool::Opencode)
        .then(|| existing_doc.frontmatter.get("name").cloned())
        .flatten();
    for (key, existing_value) in existing_doc.frontmatter {
        let is_current_tool_ns = key.as_str() == Some(current_tool.name());
        match fm.get_mut(&key) {
            Some(imported_value) if is_current_tool_ns => {
                merge_yaml_mapping_values(imported_value, existing_value);
            }
            Some(_) => {}
            None => {
                fm.insert(key, existing_value);
            }
        }
    }
    if let Some(name) = existing_name {
        fm.insert("name".into(), name);
    }
    let body = if imported_body.is_empty() {
        existing_doc.body
    } else {
        imported_doc.body
    };
    formats::markdown::render(fm, &body)
        .map(ensure_trailing_newline)
        .map(Some)
}

fn merge_yaml_mapping_values(target: &mut serde_norway::Value, existing: serde_norway::Value) {
    let (Some(target_map), serde_norway::Value::Mapping(existing_map)) =
        (target.as_mapping_mut(), existing)
    else {
        return;
    };
    for (key, value) in existing_map {
        target_map.entry(key).or_insert(value);
    }
}

fn merge_mcp_value(
    root: &Path,
    cfg: &Config,
    source_rel: &Path,
    imported: Value,
    check: bool,
    force: bool,
    out: &mut CommandOutput,
) -> Result<ImportOutcome> {
    let canonical_rel = cfg.agents_dir.join("mcp.json");
    let canonical_abs = root.join(&canonical_rel);
    let mut canonical = match read_text(&canonical_abs) {
        Ok(existing) => serde_json::from_str::<Value>(&existing).map_err(|err| {
            Error::Config(format!(
                "invalid canonical MCP config {}: {err}",
                repo_path(&canonical_rel)
            ))
        })?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => json!({ "mcpServers": {} }),
        Err(err) => return Err(io_error("read canonical MCP config", &canonical_abs, err)),
    };

    ensure_mcp_object(&mut canonical)?;
    let Some(imported_servers) = imported.get("mcpServers").and_then(Value::as_object) else {
        return Ok(ImportOutcome::default());
    };
    if imported_servers.is_empty() {
        return Ok(ImportOutcome::default());
    }

    let canonical_servers = canonical
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .expect("canonical mcpServers was normalized");
    let mut changed = false;
    let mut skipped = false;
    for (name, server) in imported_servers {
        match canonical_servers.get(name) {
            None => {
                canonical_servers.insert(name.clone(), server.clone());
                changed = true;
            }
            Some(existing) if existing == server => {}
            Some(_) if force => {
                canonical_servers.insert(name.clone(), server.clone());
                changed = true;
            }
            Some(_) => {
                skipped = true;
                out.push(format!(
                    "skipped  {}#mcpServers.{name}: already exists; use --force to overwrite",
                    repo_path(&canonical_rel)
                ));
            }
        }
    }

    if !changed {
        return Ok(ImportOutcome {
            changed: false,
            skipped,
        });
    }

    if !check {
        let text = format!("{}\n", serde_json::to_string_pretty(&canonical)?);
        write_if_changed(&canonical_abs, &text)?;
    }
    out.push(format!(
        "imported {} -> {}",
        repo_path(source_rel),
        repo_path(&canonical_rel)
    ));
    Ok(ImportOutcome {
        changed: true,
        skipped,
    })
}

fn ensure_mcp_object(value: &mut Value) -> Result<()> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| Error::Config("canonical MCP config must be a JSON object".to_string()))?;
    match obj.get_mut("mcpServers") {
        Some(Value::Object(_)) => {}
        Some(_) => {
            return Err(Error::Config(
                "canonical MCP config field `mcpServers` must be an object".to_string(),
            )
            .into());
        }
        None => {
            obj.insert("mcpServers".into(), Value::Object(JsonMap::new()));
        }
    }
    Ok(())
}

fn read_json_mcp_file(path: &Path) -> Result<Value> {
    let text = read_text(path)?;
    let mut value: Value = serde_json::from_str(&text)?;
    ensure_mcp_object(&mut value)?;
    Ok(value)
}

fn backup_native_paths(
    root: &Path,
    native_paths: &BTreeSet<PathBuf>,
    check: bool,
    out: &mut CommandOutput,
) -> Result<bool> {
    let mut changed = false;
    for rel in native_paths {
        let abs = root.join(rel);
        if !path_exists_or_symlink(&abs) {
            continue;
        }
        let backup_abs = next_backup_path(&abs);
        let backup_rel = backup_abs.strip_prefix(root).unwrap_or(&backup_abs);
        changed = true;
        if !check {
            stdfs::rename(&abs, &backup_abs).map_err(|err| {
                io_error(&format!("back up {} to", repo_path(rel)), &backup_abs, err)
            })?;
        }
        out.push(format!(
            "backed up {} -> {}",
            repo_path(rel),
            repo_path(backup_rel)
        ));
    }
    Ok(changed)
}

fn next_backup_path(path: &Path) -> PathBuf {
    let mut idx = 0;
    loop {
        let candidate = if idx == 0 {
            PathBuf::from(format!("{}.bak", path.display()))
        } else {
            PathBuf::from(format!("{}.bak.{idx}", path.display()))
        };
        if !path_exists_or_symlink(&candidate) {
            return candidate;
        }
        idx += 1;
    }
}

fn is_starter_file(path: &Path, bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    match repo_path(path).as_str() {
        "AGENTS.md" => text.trim() == "# Agents",
        ".agents/mcp.json" | ".agent/mcp.json" => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| value.get("mcpServers").and_then(Value::as_object).cloned())
            .is_some_and(|servers| servers.is_empty()),
        ".agents/rules/code-style.md" | ".agent/rules/code-style.md" => {
            text == "---\npaths:\n- \"**/*.rs\"\n---\nUse clear, direct Rust code.\n"
        }
        ".agents/skills/example-skill/SKILL.md" | ".agent/skills/example-skill/SKILL.md" => {
            text == "# Example Skill\n\nUse this as a placeholder skill.\n"
        }
        _ => false,
    }
}

fn path_exists_or_symlink(path: &Path) -> bool {
    path.exists() || path.is_symlink()
}

fn is_managed_native_source(
    source: &Path,
    target: &Path,
    target_rel: &Path,
    target_cfg: &str,
) -> bool {
    if is_managed_symlink(source, target) {
        return true;
    }
    if is_fake_symlink(source, target_rel, target_cfg) {
        return true;
    }
    if !source.is_symlink() && source.is_file() && target.is_file() {
        let source_metadata = match stdfs::metadata(source) {
            Ok(metadata) => metadata,
            Err(_) => return false,
        };
        let target_metadata = match stdfs::metadata(target) {
            Ok(metadata) => metadata,
            Err(_) => return false,
        };
        if source_metadata.len() != target_metadata.len() {
            return false;
        }
        let (Ok(source_bytes), Ok(target_bytes)) = (stdfs::read(source), stdfs::read(target))
        else {
            return false;
        };
        return source_bytes == target_bytes;
    }
    false
}

/// Returns true when `source` is a symlink that already resolves to `target`,
/// i.e. setup has already linked this native path to its canonical destination.
fn is_managed_symlink(source: &Path, target: &Path) -> bool {
    if !source.is_symlink() {
        return false;
    }
    match (stdfs::canonicalize(source), stdfs::canonicalize(target)) {
        (Ok(resolved_source), Ok(resolved_target)) => resolved_source == resolved_target,
        _ => false,
    }
}

fn same_repo_path(left: &Path, right: &Path) -> bool {
    repo_path(left) == repo_path(right)
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(repo_path)
        .unwrap_or_else(|_| path.display().to_string())
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_canonical_path_removes_compound_suffix() {
        let spec = GenerateSpec {
            from: ".agent/agents".into(),
            to: ".github/agents".into(),
            format: crate::tool::Format::CopilotAgent,
            suffix: Some(".agent.md".into()),
            recursive: false,
            tool: None,
            tools: None,
        };

        assert_eq!(
            generated_canonical_path(&spec, Path::new("reviewer.agent.md")).unwrap(),
            PathBuf::from(".agent/agents/reviewer.md")
        );
    }

    #[test]
    fn generated_canonical_path_preserves_dotted_basename() {
        let spec = GenerateSpec {
            from: ".agent/agents".into(),
            to: ".github/agents".into(),
            format: crate::tool::Format::CopilotAgent,
            suffix: Some(".agent.md".into()),
            recursive: false,
            tool: None,
            tools: None,
        };

        assert_eq!(
            generated_canonical_path(&spec, Path::new("speckit.git.commit.agent.md")).unwrap(),
            PathBuf::from(".agent/agents/speckit.git.commit.md")
        );
    }

    #[test]
    fn generated_canonical_path_preserves_subdirs_without_suffix() {
        let spec = GenerateSpec {
            from: ".agent/commands".into(),
            to: ".opencode/commands".into(),
            format: crate::tool::Format::CopilotAgent,
            suffix: None,
            recursive: true,
            tool: None,
            tools: None,
        };

        assert_eq!(
            generated_canonical_path(&spec, Path::new("group/build.md")).unwrap(),
            PathBuf::from(".agent/commands/group/build.md")
        );
    }
}
