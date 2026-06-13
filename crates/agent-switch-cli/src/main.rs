use std::{
    path::{Path, PathBuf},
    process,
};

use agent_switch_core::{
    CommandOutput, Error, ExitCode, TOOL_VERSION, config, diagnostics, fs, init, migrate, setup,
    sync, tool::Tool,
};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ags",
    version,
    about = "Synchronize canonical .agents files with coding agent native formats."
)]
struct Cli {
    /// Repository root. Defaults to the nearest directory containing .agent-switch.yaml, .agents, or .git.
    #[arg(long, global = true, env = "AGENT_SWITCH_ROOT")]
    root: Option<PathBuf>,
    /// Path to .agent-switch.yaml. Used by migrate, setup, sync, doctor, and mappings validate.
    #[arg(long, global = true, env = "AGENT_SWITCH_CONFIG")]
    config: Option<PathBuf>,
    /// Comma-separated migrate/setup/sync tool filter (e.g. `claude,copilot`).
    #[arg(long, global = true, env = "AGENT_SWITCH_TOOLS")]
    tool: Option<String>,
    /// Suppress normal output while preserving exit status.
    #[arg(long, global = true)]
    quiet: bool,
    /// Print command diagnostics to stderr.
    #[arg(long, short = 'v', global = true)]
    verbose: bool,
    /// Print detailed diagnostics to stderr. Implies --verbose.
    #[arg(long, global = true)]
    debug: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create starter config, canonical directories, sample files, and .gitignore entries.
    Init(InitArgs),
    /// Import existing native coding-agent files into canonical .agents files.
    Migrate(MigrateArgs),
    /// Create or repair native tool links/copies, then run sync unless --no-sync is set.
    Setup(SetupArgs),
    /// Import native changes, export canonical files, merge config, and update the manifest.
    Sync(SyncArgs),
    /// Inspect config, links, manifest, and generated-file drift.
    Doctor(DoctorArgs),
    /// Validate configured symlink/generate/merge mappings.
    Mappings(MappingsCommand),
    /// Print build version metadata.
    Version(VersionArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    /// Write default mappings only for this comma-separated tool list.
    #[arg(long)]
    tools: Option<String>,
    /// Overwrite existing starter files and config.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct MigrateArgs {
    /// Report what would be imported, backed up, or linked without writing files.
    #[arg(long)]
    check: bool,
    /// Overwrite conflicting canonical files and repair incorrect managed symlinks.
    #[arg(long)]
    force: bool,
    /// Keep native files/directories in place, and skip the final setup pass.
    #[arg(long)]
    keep_native: bool,
    /// Skip the final setup/sync pass after imports and backups.
    #[arg(long)]
    no_setup: bool,
}

#[derive(Debug, Args)]
struct SetupArgs {
    /// Only create/repair links and copy fallbacks; skip the automatic sync step.
    #[arg(long)]
    no_sync: bool,
    /// Report drift without writing files. Exits with the drift code when changes are needed.
    #[arg(long)]
    check: bool,
    /// Repair incorrect managed symlinks. Real files and directories are still preserved.
    #[arg(long)]
    force: bool,
    /// Remove managed links/copies for unselected tools when --tool is used.
    #[arg(long)]
    prune: bool,
}

#[derive(Debug, Args)]
struct SyncArgs {
    /// Report generated-file drift without writing files. Exits with the drift code on changes.
    #[arg(long)]
    check: bool,
    /// Import native generated files back into canonical .agents files only.
    #[arg(long, conflicts_with = "export_only")]
    import_only: bool,
    /// Export canonical .agents files to native tool formats only.
    #[arg(long, conflicts_with = "import_only")]
    export_only: bool,
    /// Ignore the existing sync manifest and rebuild it from current files.
    #[arg(long)]
    reset_manifest: bool,
    /// Emit a deterministic machine-readable sync report.
    #[arg(long)]
    json: bool,
    /// Comma-separated event types to include in sync output (e.g. `generated,merged`).
    #[arg(long, value_delimiter = ',')]
    event_filter: Vec<String>,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    /// Emit diagnostics as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum MappingsSubcommand {
    /// Validate config mapping sections without running setup or sync.
    Validate(JsonArg),
}

#[derive(Debug, Args)]
struct MappingsCommand {
    #[command(subcommand)]
    command: MappingsSubcommand,
}

#[derive(Debug, Args)]
struct JsonArg {
    /// Emit validation output as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct VersionArgs {
    /// Emit version metadata as JSON.
    #[arg(long)]
    json: bool,
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(out) => {
            if !out.diagnostics.is_empty() {
                eprintln!("{}", out.diagnostics.join("\n"));
            }
            if !out.lines.is_empty() {
                println!("{}", out.lines.join("\n"));
            }
            process::exit(out.exit().code());
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            process::exit(classify_error(&err).code());
        }
    }
}

fn run(cli: Cli) -> Result<CommandOutput> {
    let root = config::find_root(cli.root.as_deref())?;
    let config_path = cli.config;
    let tools = cli.tool.as_deref().map(config::parse_tools).transpose()?;
    let tools_ref = tools.as_deref();
    let verbosity = Verbosity {
        verbose: cli.verbose || cli.debug,
        debug: cli.debug,
    };

    let mut out = match cli.command {
        Commands::Init(args) => {
            if let Some(raw) = args.tools.as_deref() {
                config::parse_tools(raw)?;
            }
            let mut out = init::run(&root, args.tools.as_deref(), args.force)?;
            add_basic_diagnostics(&mut out, verbosity, "init", &root);
            if verbosity.verbose {
                out.diagnostic(format!(
                    "verbose: init tools: {}",
                    args.tools.as_deref().unwrap_or("all")
                ));
            }
            out
        }
        Commands::Migrate(args) => {
            let mut out = migrate::run(
                &root,
                config_path.as_deref(),
                tools_ref,
                migrate::MigrateOptions {
                    check: args.check,
                    force: args.force,
                    keep_native: args.keep_native,
                    no_setup: args.no_setup,
                },
            )?;
            let loaded_config_path = config::resolve_config_path(&root, config_path.as_deref());
            let cfg = if loaded_config_path.exists() {
                config::load_config(&root, config_path.as_deref())
                    .ok()
                    .map(|(cfg, _)| cfg)
            } else {
                None
            };
            if let Some(cfg) = cfg.as_ref() {
                add_config_diagnostics(
                    &mut out,
                    verbosity,
                    "migrate",
                    &root,
                    &loaded_config_path,
                    cfg,
                    tools_ref,
                );
            } else {
                add_basic_diagnostics(&mut out, verbosity, "migrate", &root);
            }
            add_migrate_diagnostics(&mut out, verbosity, &args);
            out
        }
        Commands::Setup(args) => {
            let (cfg, loaded_config_path) = config::load_config(&root, config_path.as_deref())?;
            let mut out = setup::run(
                &root,
                &cfg,
                tools_ref,
                setup::SetupOptions {
                    no_sync: args.no_sync,
                    check: args.check,
                    force: args.force,
                    prune: args.prune,
                },
            )?;
            add_config_diagnostics(
                &mut out,
                verbosity,
                "setup",
                &root,
                &loaded_config_path,
                &cfg,
                tools_ref,
            );
            add_setup_diagnostics(&mut out, verbosity, &args);
            out
        }
        Commands::Sync(args) => {
            let (cfg, loaded_config_path) = config::load_config(&root, config_path.as_deref())?;
            let event_filter = if args.event_filter.is_empty() {
                None
            } else {
                Some(sync::parse_event_filter(&args.event_filter)?)
            };

            let mut out = sync::run(
                &root,
                &cfg,
                tools_ref,
                sync::SyncOptions {
                    check: args.check,
                    import_only: args.import_only,
                    export_only: args.export_only,
                    reset_manifest: args.reset_manifest,
                    json: args.json,
                    event_filter,
                },
            )?;
            add_config_diagnostics(
                &mut out,
                verbosity,
                "sync",
                &root,
                &loaded_config_path,
                &cfg,
                tools_ref,
            );
            add_sync_diagnostics(&mut out, verbosity, &args);
            out
        }
        Commands::Doctor(args) => {
            let path = config::resolve_config_path(&root, config_path.as_deref());
            let cfg = if path.exists() || config_path.is_some() {
                match config::load_config(&root, config_path.as_deref()) {
                    Ok((cfg, _)) => Some(cfg),
                    Err(err) => {
                        return diagnostics::doctor_config_error(&root, &path, &err, args.json);
                    }
                }
            } else {
                None
            };
            let mut out = diagnostics::doctor(&root, cfg.as_ref(), args.json)?;
            add_basic_diagnostics(&mut out, verbosity, "doctor", &root);
            if let Some(cfg) = cfg.as_ref() {
                add_config_selection_diagnostics(&mut out, verbosity, cfg, tools_ref);
            }
            out
        }
        Commands::Mappings(cmd) => match cmd.command {
            MappingsSubcommand::Validate(args) => {
                let (cfg, loaded_config_path) = config::load_config(&root, config_path.as_deref())?;
                let mut out = diagnostics::validate_mappings(&cfg, args.json)?;
                add_config_diagnostics(
                    &mut out,
                    verbosity,
                    "mappings validate",
                    &root,
                    &loaded_config_path,
                    &cfg,
                    tools_ref,
                );
                out
            }
        },
        Commands::Version(args) => {
            let mut out = version_output(args.json)?;
            add_basic_diagnostics(&mut out, verbosity, "version", &root);
            out
        }
    };

    if cli.quiet {
        out.lines.clear();
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy)]
struct Verbosity {
    verbose: bool,
    debug: bool,
}

fn add_basic_diagnostics(
    out: &mut CommandOutput,
    verbosity: Verbosity,
    command: &str,
    root: &Path,
) {
    if !verbosity.verbose {
        return;
    }
    out.diagnostic(format!("verbose: command: {command}"));
    out.diagnostic(format!("verbose: root: {}", root.display()));
}

fn add_config_diagnostics(
    out: &mut CommandOutput,
    verbosity: Verbosity,
    command: &str,
    root: &Path,
    config_path: &Path,
    cfg: &config::Config,
    tools: Option<&[Tool]>,
) {
    add_basic_diagnostics(out, verbosity, command, root);
    if !verbosity.verbose {
        return;
    }
    out.diagnostic(format!(
        "verbose: config: {}",
        display_path(root, config_path)
    ));
    out.diagnostic(format!(
        "verbose: manifest: {}",
        fs::repo_path(&cfg.manifest)
    ));
    out.diagnostic(format!(
        "verbose: tool filter: {}",
        tool_filter_label(tools)
    ));
    add_config_selection_diagnostics(out, verbosity, cfg, tools);
}

fn add_config_selection_diagnostics(
    out: &mut CommandOutput,
    verbosity: Verbosity,
    cfg: &config::Config,
    tools: Option<&[Tool]>,
) {
    if !verbosity.verbose {
        return;
    }

    let selected_symlinks = cfg
        .symlinks
        .iter()
        .filter(|(link, spec)| config::symlink_selected(link, spec, tools))
        .count();
    let selected_generate = cfg
        .generate
        .values()
        .filter(|spec| config::generate_selected(spec, tools))
        .count();
    let selected_merge = cfg
        .merge
        .iter()
        .filter(|(id, spec)| config::merge_selected(id, spec, tools))
        .count();

    out.diagnostic(format!(
        "verbose: selected symlinks: {selected_symlinks}/{}",
        cfg.symlinks.len()
    ));
    out.diagnostic(format!(
        "verbose: selected generate specs: {selected_generate}/{}",
        cfg.generate.len()
    ));
    out.diagnostic(format!(
        "verbose: selected merge specs: {selected_merge}/{}",
        cfg.merge.len()
    ));

    if verbosity.debug {
        out.diagnostic(format!(
            "debug: selected symlinks: {}",
            selected_keys(cfg.symlinks.iter(), |link, spec| config::symlink_selected(
                link, spec, tools
            ))
        ));
        out.diagnostic(format!(
            "debug: selected generate specs: {}",
            selected_keys(cfg.generate.iter(), |_, spec| {
                config::generate_selected(spec, tools)
            })
        ));
        out.diagnostic(format!(
            "debug: selected merge specs: {}",
            selected_keys(cfg.merge.iter(), |id, spec| config::merge_selected(
                id, spec, tools
            ))
        ));
    }
}

fn add_sync_diagnostics(out: &mut CommandOutput, verbosity: Verbosity, args: &SyncArgs) {
    if !verbosity.verbose {
        return;
    }
    out.diagnostic(format!(
        "verbose: sync stages: {}",
        sync_stage_labels(args).join(", ")
    ));
    out.diagnostic(format!("verbose: reset manifest: {}", args.reset_manifest));
    if verbosity.debug {
        out.diagnostic(format!("debug: import only: {}", args.import_only));
        out.diagnostic(format!("debug: export only: {}", args.export_only));
        out.diagnostic(format!("debug: check mode: {}", args.check));
        out.diagnostic(format!(
            "debug: event filter: {}",
            if args.event_filter.is_empty() {
                "all".to_string()
            } else {
                args.event_filter.join(",")
            }
        ));
    }
}

fn add_migrate_diagnostics(out: &mut CommandOutput, verbosity: Verbosity, args: &MigrateArgs) {
    if !verbosity.verbose {
        return;
    }
    out.diagnostic(format!(
        "verbose: no setup: {}",
        args.no_setup || args.keep_native
    ));
    out.diagnostic(format!("verbose: keep native: {}", args.keep_native));
    if verbosity.debug {
        out.diagnostic(format!("debug: check mode: {}", args.check));
        out.diagnostic(format!("debug: force overwrite: {}", args.force));
    }
}

fn add_setup_diagnostics(out: &mut CommandOutput, verbosity: Verbosity, args: &SetupArgs) {
    if !verbosity.verbose {
        return;
    }
    out.diagnostic(format!("verbose: no sync: {}", args.no_sync));
    out.diagnostic(format!("verbose: prune: {}", args.prune));
    if verbosity.debug {
        out.diagnostic(format!("debug: check mode: {}", args.check));
        out.diagnostic(format!("debug: force repair: {}", args.force));
    }
}

fn sync_stage_labels(args: &SyncArgs) -> Vec<&'static str> {
    if args.import_only {
        vec!["import"]
    } else if args.export_only {
        vec!["export", "remove-stale", "sync-links", "merge"]
    } else {
        vec!["import", "export", "remove-stale", "sync-links", "merge"]
    }
}

fn selected_keys<'a, T, I, F>(iter: I, selected: F) -> String
where
    T: 'a,
    I: IntoIterator<Item = (&'a String, &'a T)>,
    F: Fn(&str, &T) -> bool,
{
    let keys = iter
        .into_iter()
        .filter(|(key, spec)| selected(key, spec))
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    if keys.is_empty() {
        "(none)".to_string()
    } else {
        keys.join(", ")
    }
}

fn tool_filter_label(tools: Option<&[Tool]>) -> String {
    match tools {
        Some(tools) => tools
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
        None => "all".to_string(),
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(fs::repo_path)
        .unwrap_or_else(|_| path.display().to_string())
}

fn version_output(json_output: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    if json_output {
        out.push(serde_json::to_string_pretty(&serde_json::json!({
            "version": TOOL_VERSION,
            "commit": option_env!("GIT_SHA").unwrap_or("unknown"),
            "target": option_env!("TARGET").unwrap_or("unknown"),
            "rustc": option_env!("RUSTC_VERSION").unwrap_or("unknown"),
            "cargo_lock_sha256": option_env!("CARGO_LOCK_SHA256").unwrap_or("unknown"),
            "build_date": option_env!("BUILD_DATE").unwrap_or("unknown"),
        }))?);
    } else {
        out.push(format!("ags {TOOL_VERSION}"));
        out.push(format!(
            "commit: {}",
            option_env!("GIT_SHA").unwrap_or("unknown")
        ));
        out.push(format!(
            "target: {}",
            option_env!("TARGET").unwrap_or("unknown")
        ));
        out.push(format!(
            "rustc: {}",
            option_env!("RUSTC_VERSION").unwrap_or("unknown")
        ));
        out.push(format!(
            "cargo lock sha256: {}",
            option_env!("CARGO_LOCK_SHA256").unwrap_or("unknown")
        ));
        out.push(format!(
            "build date: {}",
            option_env!("BUILD_DATE").unwrap_or("unknown")
        ));
    }
    Ok(out)
}

fn classify_error(err: &anyhow::Error) -> ExitCode {
    match err.downcast_ref::<Error>() {
        Some(Error::Config(_)) => ExitCode::Config,
        Some(Error::Unsupported(_)) => ExitCode::Unsupported,
        None => ExitCode::Io,
    }
}
