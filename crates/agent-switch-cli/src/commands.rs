use std::path::Path;

use agent_switch_core::{
    CommandOutput, Error, TOOL_VERSION, config, diagnostics, fs, init, migrate, output, setup,
    sync, tool::Tool,
};
use anyhow::Result;

use crate::{
    Cli, Commands, MappingsSubcommand, MigrateArgs, OperationSubcommand, SchemaSubcommand,
    SetupArgs, SyncArgs,
    invocation::{Invocation, Verbosity},
    operation, schema,
};

pub(super) fn run(cli: Cli) -> Result<CommandOutput> {
    let effect = operation::classify(&cli.command);
    let init_tools = match &cli.command {
        Commands::Init(args) => args.tools.as_deref(),
        _ => None,
    };
    let mut invocation = Invocation::from_cli(
        cli.root.as_deref(),
        cli.config,
        effect.operation,
        cli.tool.as_deref(),
        init_tools,
        effect.mutates_files,
        cli.verbose,
        cli.debug,
    )?;
    let root = invocation.root.as_path();
    let config_path = invocation.config_path_arg();
    let tools_ref = invocation.tools();
    let verbosity = invocation.verbosity;

    let mut out = match cli.command {
        Commands::Init(args) => {
            let mut out = init::run(root, args.tools.as_deref(), args.force)?;
            add_basic_diagnostics(&mut out, verbosity, "init", root);
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
                root,
                config_path,
                tools_ref,
                migrate::MigrateOptions {
                    check: args.check,
                    force: args.force,
                    keep_native: args.keep_native,
                    no_setup: args.no_setup,
                },
            )?;
            let loaded_config_path = invocation.config_path();
            let cfg = if loaded_config_path.exists() {
                invocation.load_config().ok().map(|(cfg, _)| cfg)
            } else {
                None
            };
            if let Some(cfg) = cfg.as_ref() {
                add_config_diagnostics(
                    &mut out,
                    verbosity,
                    "migrate",
                    root,
                    &loaded_config_path,
                    cfg,
                    tools_ref,
                );
            } else {
                add_basic_diagnostics(&mut out, verbosity, "migrate", root);
            }
            add_migrate_diagnostics(&mut out, verbosity, &args);
            out
        }
        Commands::Setup(args) => {
            let (cfg, loaded_config_path) = invocation.load_config()?;
            let mut out = setup::run(
                root,
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
                root,
                &loaded_config_path,
                &cfg,
                tools_ref,
            );
            add_setup_diagnostics(&mut out, verbosity, &args);
            out
        }
        Commands::Sync(args) => {
            let (cfg, loaded_config_path) = invocation.load_config()?;
            let event_filter = if args.event_filter.is_empty() {
                None
            } else {
                Some(sync::parse_event_filter(&args.event_filter)?)
            };

            let mut out = sync::run(
                root,
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
                root,
                &loaded_config_path,
                &cfg,
                tools_ref,
            );
            add_sync_diagnostics(&mut out, verbosity, &args, &cfg);
            out
        }
        Commands::Doctor(args) => {
            let path = invocation.config_path();
            let cfg = if path.exists() || config_path.is_some() {
                match invocation.load_config() {
                    Ok((cfg, _)) => Some(cfg),
                    Err(err) => {
                        return diagnostics::doctor_config_error(root, &path, &err, args.json);
                    }
                }
            } else {
                None
            };
            let mut out = diagnostics::doctor_at(root, cfg.as_ref(), &path, args.json)?;
            add_basic_diagnostics(&mut out, verbosity, "doctor", root);
            if let Some(cfg) = cfg.as_ref() {
                add_config_selection_diagnostics(&mut out, verbosity, cfg, tools_ref);
            }
            out
        }
        Commands::Mappings(cmd) => match cmd.command {
            MappingsSubcommand::Validate(args) => {
                let (cfg, loaded_config_path) = invocation.load_config()?;
                let mut out = diagnostics::validate_mappings(&cfg, args.json)?;
                add_config_diagnostics(
                    &mut out,
                    verbosity,
                    "mappings validate",
                    root,
                    &loaded_config_path,
                    &cfg,
                    tools_ref,
                );
                out
            }
        },
        Commands::Operation(command) => match command.command {
            OperationSubcommand::List(args) => {
                let mut out = operation_output(args.json)?;
                add_basic_diagnostics(&mut out, verbosity, "operation list", root);
                out
            }
        },
        Commands::Schema(command) => match command.command {
            SchemaSubcommand::List(args) => {
                let mut out = schema::list(args.json)?;
                add_basic_diagnostics(&mut out, verbosity, "schema list", root);
                out
            }
            SchemaSubcommand::Print(args) => {
                let content = schema::content(&args.name)
                    .ok_or_else(|| Error::Config(format!("unknown schema: {}", args.name)))?;
                let mut out = CommandOutput::default();
                out.push(content);
                add_basic_diagnostics(&mut out, verbosity, "schema print", root);
                out
            }
        },
        Commands::Version(args) => {
            let mut out = version_output(args.json)?;
            add_basic_diagnostics(&mut out, verbosity, "version", root);
            out
        }
    };

    if let Some(record) = invocation.recovered_operation() {
        out.diagnostic(format!(
            "warning: recovered interrupted {} operation (pid {})",
            record.command, record.pid
        ));
    }
    if cli.quiet {
        out.lines.clear();
    }
    invocation.complete()?;
    Ok(out)
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

fn add_sync_diagnostics(
    out: &mut CommandOutput,
    verbosity: Verbosity,
    args: &SyncArgs,
    cfg: &config::Config,
) {
    if !verbosity.verbose {
        return;
    }
    out.diagnostic(format!(
        "verbose: sync stages: {}",
        sync_stage_labels(args, cfg).join(", ")
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

fn sync_stage_labels(args: &SyncArgs, cfg: &config::Config) -> Vec<&'static str> {
    if args.import_only {
        vec!["import"]
    } else if args.export_only {
        vec!["export", "remove-stale", "sync-links", "merge"]
    } else {
        match cfg.sync_mode {
            config::SyncMode::Full => {
                vec!["import", "export", "remove-stale", "sync-links", "merge"]
            }
            config::SyncMode::CanonicalOnly | config::SyncMode::ExportOnly => {
                vec!["export", "remove-stale", "sync-links", "merge"]
            }
            config::SyncMode::ImportOnly => vec!["import"],
        }
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

fn operation_output(json_output: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    if json_output {
        let operations = operation::OPERATION_CATALOG
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": entry.id,
                    "risk": entry.risk.as_str(),
                    "mutates_files": entry.mutates_files,
                    "supports_json": entry.supports_json,
                    "description": entry.description,
                })
            })
            .collect::<Vec<_>>();
        out.push(output::render_json(&serde_json::json!({
            "operations": operations,
        }))?);
    } else {
        for entry in operation::OPERATION_CATALOG {
            out.push(format!(
                "{}\\t{}\\tmutates_files={}\\tjson={}\\t{}",
                entry.id,
                entry.risk.as_str(),
                entry.mutates_files,
                entry.supports_json,
                entry.description,
            ));
        }
    }
    Ok(out)
}

fn version_output(json_output: bool) -> Result<CommandOutput> {
    let mut out = CommandOutput::default();
    if json_output {
        out.push(output::render_json(&serde_json::json!({
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
