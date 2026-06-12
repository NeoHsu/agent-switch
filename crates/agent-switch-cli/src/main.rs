use std::{env, path::PathBuf, process};

use agent_switch_core::{
    config, diagnostics, init, setup, sync, CommandOutput, ExitCode, TOOL_VERSION,
};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "agent-switch",
    version,
    about = "Synchronize canonical .agents files with coding agent native formats."
)]
struct Cli {
    #[arg(long, global = true, env = "AGENT_SWITCH_ROOT")]
    root: Option<PathBuf>,
    #[arg(long, global = true, env = "AGENT_SWITCH_CONFIG")]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    tool: Option<String>,
    #[arg(long, global = true, alias = "target")]
    target: Option<String>,
    #[arg(long, global = true)]
    verbose: bool,
    #[arg(long, global = true)]
    quiet: bool,
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init(InitArgs),
    Setup(SetupArgs),
    Sync(SyncArgs),
    Doctor(DoctorArgs),
    Mappings(MappingsCommand),
    Version(VersionArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(long)]
    tools: Option<String>,
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct SetupArgs {
    #[arg(long)]
    no_sync: bool,
    #[arg(long)]
    check: bool,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    prune: bool,
}

#[derive(Debug, Args)]
struct SyncArgs {
    #[arg(long)]
    check: bool,
    #[arg(long)]
    import_only: bool,
    #[arg(long)]
    export_only: bool,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum MappingsSubcommand {
    Validate(JsonArg),
}

#[derive(Debug, Args)]
struct MappingsCommand {
    #[command(subcommand)]
    command: MappingsSubcommand,
}

#[derive(Debug, Args)]
struct JsonArg {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct VersionArgs {
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
            eprintln!("{err:#}");
            process::exit(classify_error(&err).code());
        }
    }
}

fn run(cli: Cli) -> Result<CommandOutput> {
    let _ = (cli.verbose, cli.no_color);
    let root = config::find_root(cli.root.as_deref())?;
    let config_path = cli
        .config
        .or_else(|| env::var_os("AGENTSTITCH_CONFIG").map(PathBuf::from));
    let tools = config::parse_tools(cli.tool.as_deref(), cli.target.as_deref())?;
    let tools_ref = tools.as_deref();

    let mut out = match cli.command {
        Commands::Init(args) => init::run(&root, args.tools.as_deref(), args.force)?,
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
            sync::run(
                &root,
                &cfg,
                tools_ref,
                sync::SyncOptions {
                    check: args.check,
                    import_only: args.import_only,
                    export_only: args.export_only,
                },
            )?
        }
        Commands::Doctor(args) => {
            let cfg = config::load_config(&root, config_path.as_deref())
                .ok()
                .map(|(cfg, _)| cfg);
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
            "target": env::var("TARGET").unwrap_or_else(|_| "unknown".into()),
            "build_date": option_env!("BUILD_DATE").unwrap_or("unknown"),
        }))?);
    } else {
        out.push(format!("agent-switch {TOOL_VERSION}"));
        out.push(format!(
            "commit: {}",
            option_env!("GIT_SHA").unwrap_or("unknown")
        ));
        out.push(format!(
            "target: {}",
            env::var("TARGET").unwrap_or_else(|_| "unknown".into())
        ));
        out.push(format!(
            "build date: {}",
            option_env!("BUILD_DATE").unwrap_or("unknown")
        ));
    }
    Ok(out)
}

fn classify_error(err: &anyhow::Error) -> ExitCode {
    let text = format!("{err:#}");
    if text.contains("unsupported") {
        ExitCode::Unsupported
    } else if text.contains("config")
        || text.contains("schema")
        || text.contains("unknown tool")
        || text.contains("--tool")
    {
        ExitCode::Config
    } else {
        ExitCode::Io
    }
}
