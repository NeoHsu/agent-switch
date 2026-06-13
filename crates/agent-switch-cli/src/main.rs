use std::{path::PathBuf, process};

use agent_switch_core::{
    CommandOutput, Error, ExitCode, TOOL_VERSION, config, diagnostics, init, setup, sync,
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
    /// Path to .agent-switch.yaml. Used by setup, sync, doctor, and mappings validate.
    #[arg(long, global = true, env = "AGENT_SWITCH_CONFIG")]
    config: Option<PathBuf>,
    /// Comma-separated setup/sync tool filter (e.g. `claude,copilot`).
    #[arg(long, global = true, env = "AGENT_SWITCH_TOOLS")]
    tool: Option<String>,
    /// Suppress normal output while preserving exit status.
    #[arg(long, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create starter config, canonical directories, sample files, and .gitignore entries.
    Init(InitArgs),
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

    let mut out = match cli.command {
        Commands::Init(args) => {
            if let Some(raw) = args.tools.as_deref() {
                config::parse_tools(raw)?;
            }
            init::run(&root, args.tools.as_deref(), args.force)?
        }
        Commands::Setup(args) => {
            let (cfg, _) = config::load_config(&root, config_path.as_deref())?;
            setup::run(
                &root,
                &cfg,
                tools_ref,
                setup::SetupOptions {
                    no_sync: args.no_sync,
                    check: args.check,
                    force: args.force,
                    prune: args.prune,
                },
            )?
        }
        Commands::Sync(args) => {
            let (cfg, _) = config::load_config(&root, config_path.as_deref())?;
            let event_filter = if args.event_filter.is_empty() {
                None
            } else {
                Some(sync::parse_event_filter(&args.event_filter)?)
            };

            sync::run(
                &root,
                &cfg,
                tools_ref,
                sync::SyncOptions {
                    check: args.check,
                    import_only: args.import_only,
                    export_only: args.export_only,
                    json: args.json,
                    event_filter,
                },
            )?
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
            diagnostics::doctor(&root, cfg.as_ref(), args.json)?
        }
        Commands::Mappings(cmd) => match cmd.command {
            MappingsSubcommand::Validate(args) => {
                let (cfg, _) = config::load_config(&root, config_path.as_deref())?;
                diagnostics::validate_mappings(&cfg, args.json)?
            }
        },
        Commands::Version(args) => version_output(args.json)?,
    };

    if cli.quiet {
        out.lines.clear();
    }
    Ok(out)
}

fn version_output(json_output: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    if json_output {
        out.push(serde_json::to_string_pretty(&serde_json::json!({
            "version": TOOL_VERSION,
            "commit": option_env!("GIT_SHA").unwrap_or("unknown"),
            "target": option_env!("TARGET").unwrap_or("unknown"),
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
