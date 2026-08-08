use std::{path::PathBuf, process};

use agent_switch_core::{CommandOutput, Error, ExitCode, output};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};

mod commands;
mod invocation;
mod operation;
mod schema;

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
    /// Discover stable CLI operation metadata.
    Operation(OperationCommand),
    /// Print bundled machine-readable schemas.
    Schema(SchemaCommand),
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
    /// Fail closed unless the installed CLI matches this Skill version.
    #[arg(long)]
    skill_version: Option<String>,
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

#[derive(Debug, Subcommand)]
enum OperationSubcommand {
    /// List stable operations and their safety metadata.
    List(JsonArg),
}

#[derive(Debug, Args)]
struct OperationCommand {
    #[command(subcommand)]
    command: OperationSubcommand,
}

#[derive(Debug, Subcommand)]
enum SchemaSubcommand {
    /// List bundled schemas.
    List(JsonArg),
    /// Print one bundled schema without an output envelope.
    Print(SchemaPrintArgs),
}

#[derive(Debug, Args)]
struct SchemaCommand {
    #[command(subcommand)]
    command: SchemaSubcommand,
}

#[derive(Debug, Args)]
struct SchemaPrintArgs {
    /// Schema name, with an optional `.schema.json` suffix.
    name: String,
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
    let json_output = operation::classify(&cli.command).json_output;
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
            let exit = classify_error(&err);
            if json_output {
                match output::render_error(error_kind(&err), &format!("{err:#}"), exit.code()) {
                    Ok(payload) => eprintln!("{payload}"),
                    Err(render_error) => eprintln!("error: {err:#} (JSON error: {render_error})"),
                }
            } else {
                eprintln!("error: {err:#}");
            }
            process::exit(exit.code());
        }
    }
}

fn run(cli: Cli) -> Result<CommandOutput> {
    commands::run(cli)
}

fn classify_error(err: &anyhow::Error) -> ExitCode {
    for cause in err.chain() {
        match cause.downcast_ref::<Error>() {
            Some(Error::Config(_)) => return ExitCode::Config,
            Some(Error::Unsupported(_)) => return ExitCode::Unsupported,
            None => {}
        }
    }
    ExitCode::Io
}

fn error_kind(err: &anyhow::Error) -> &'static str {
    for cause in err.chain() {
        match cause.downcast_ref::<Error>() {
            Some(Error::Config(_)) => return "config",
            Some(Error::Unsupported(_)) => return "unsupported",
            None => {}
        }
    }
    "io"
}

#[cfg(test)]
mod docs_tests {
    use std::{fs, path::Path};

    use anyhow::Result;
    use clap::{Parser, error::ErrorKind};

    use super::Cli;

    #[derive(Debug)]
    struct DocCommand {
        file: String,
        line: usize,
        origin: &'static str,
        command: String,
    }

    #[test]
    fn documented_commands_match_the_clap_parser() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut markdown_files = vec![root.join("README.md")];
        collect_markdown_files(&root.join("docs"), &mut markdown_files)?;
        markdown_files.sort();

        let mut commands = Vec::new();
        for path in markdown_files {
            let markdown = fs::read_to_string(&path)?;
            let file = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            extract_commands(&file, &markdown, &mut commands);
        }

        assert!(
            commands.len() >= 100,
            "expected broad README/docs command coverage, found {}",
            commands.len()
        );
        assert!(commands.iter().any(|command| command.origin == "bash"));
        assert!(commands.iter().any(|command| command.origin == "inline"));

        let mut failures = Vec::new();
        for documented in commands {
            let normalized = documented
                .command
                .replace("<tool>", "codex")
                .replace("<events>", "generated");
            let args = normalized.split_whitespace().collect::<Vec<_>>();
            match Cli::try_parse_from(args) {
                Ok(_) => {}
                Err(err)
                    if matches!(
                        err.kind(),
                        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
                    ) => {}
                Err(err) => failures.push(format!(
                    "{}:{} ({}): `{}`\n{}",
                    documented.file,
                    documented.line,
                    documented.origin,
                    documented.command,
                    err.render().ansi()
                )),
            }
        }

        assert!(
            failures.is_empty(),
            "documented commands rejected by Clap:\n\n{}",
            failures.join("\n\n")
        );
        Ok(())
    }

    fn collect_markdown_files(
        dir: &Path,
        files: &mut Vec<std::path::PathBuf>,
    ) -> std::io::Result<()> {
        let mut entries = fs::read_dir(dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort();
        for path in entries {
            if path.is_dir() {
                collect_markdown_files(&path, files)?;
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
        Ok(())
    }

    fn extract_commands(file: &str, markdown: &str, commands: &mut Vec<DocCommand>) {
        let mut in_fence = false;
        let mut bash_fence = false;

        for (index, line) in markdown.lines().enumerate() {
            let line_number = index + 1;
            let trimmed = line.trim();
            if let Some(info) = trimmed.strip_prefix("```") {
                if in_fence {
                    in_fence = false;
                    bash_fence = false;
                } else {
                    in_fence = true;
                    bash_fence = matches!(info.trim(), "bash" | "sh" | "shell" | "console");
                }
                continue;
            }

            if in_fence {
                if bash_fence {
                    let shell_line = trimmed.strip_prefix("$ ").unwrap_or(trimmed);
                    if shell_line == "ags" || shell_line.starts_with("ags ") {
                        commands.push(DocCommand {
                            file: file.into(),
                            line: line_number,
                            origin: "bash",
                            command: shell_line.into(),
                        });
                    }
                }
                continue;
            }

            for (span_index, span) in line.split('`').enumerate() {
                if span_index % 2 == 1 && span.starts_with("ags ") {
                    commands.push(DocCommand {
                        file: file.into(),
                        line: line_number,
                        origin: "inline",
                        command: span.into(),
                    });
                }
            }
        }
    }
}
