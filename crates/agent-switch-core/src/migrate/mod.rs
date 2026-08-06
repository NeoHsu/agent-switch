//! Migration command implementation for importing existing native tool files.

use std::{collections::BTreeSet, path::Path};

use anyhow::Result;

use crate::{
    CommandOutput, ExitCode, init,
    setup::{self, SetupOptions},
    tool::Tool,
};

mod adapters;

use adapters::{
    backup_native_paths, ensure_canonical_dirs, ensure_config, import_generated_sources,
    import_legacy_copilot_mcp, import_merge_sources, import_symlink_sources,
    queue_managed_legacy_links_for_backup, with_legacy_import_links,
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

const LEGACY_IMPORT_LINKS: &[(&str, &str, Tool)] = &[
    (".pi/skills", "skills", Tool::Pi),
    (".agent/rules", "rules", Tool::Antigravity),
    (".agent/skills", "skills", Tool::Antigravity),
];

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
    let import_cfg = with_legacy_import_links(&cfg);
    let symlink_outcome = import_symlink_sources(
        root,
        &import_cfg,
        tools,
        opts,
        &mut native_paths_to_backup,
        &mut out,
    )?;
    drift |= symlink_outcome.changed || symlink_outcome.skipped;
    skipped |= symlink_outcome.skipped;
    queue_managed_legacy_links_for_backup(root, &cfg, tools, &mut native_paths_to_backup);
    let merge_outcome = import_merge_sources(root, &cfg, tools, opts, &mut out)?;
    drift |= merge_outcome.changed || merge_outcome.skipped;
    skipped |= merge_outcome.skipped;
    let legacy_copilot_outcome = import_legacy_copilot_mcp(
        root,
        &cfg,
        tools,
        opts,
        &mut native_paths_to_backup,
        &mut out,
    )?;
    drift |= legacy_copilot_outcome.changed || legacy_copilot_outcome.skipped;
    skipped |= legacy_copilot_outcome.skipped;

    if !opts.keep_native {
        drift |= backup_native_paths(root, &native_paths_to_backup, opts.check, &mut out)?;
    }

    if !opts.check {
        init::update_gitignore_for_config(root, &cfg, &mut out)?;
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
