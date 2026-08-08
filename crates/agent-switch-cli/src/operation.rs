//! Command effect classification and stable operation metadata.

use super::{
    Commands, MappingsCommand, MappingsSubcommand, OperationCommand, OperationSubcommand,
    SchemaCommand, SchemaSubcommand,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationRisk {
    Read,
    Write,
    Destructive,
}

impl OperationRisk {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OperationMetadata {
    pub(crate) id: &'static str,
    pub(crate) risk: OperationRisk,
    pub(crate) mutates_files: bool,
    pub(crate) supports_json: bool,
    pub(crate) description: &'static str,
}

pub(crate) const OPERATION_CATALOG: &[OperationMetadata] = &[
    OperationMetadata {
        id: "init",
        risk: OperationRisk::Write,
        mutates_files: true,
        supports_json: false,
        description: "Create canonical directories, starter files, and mappings.",
    },
    OperationMetadata {
        id: "migrate",
        risk: OperationRisk::Write,
        mutates_files: true,
        supports_json: false,
        description: "Import native tool files into canonical .agents files.",
    },
    OperationMetadata {
        id: "setup",
        risk: OperationRisk::Destructive,
        mutates_files: true,
        supports_json: false,
        description: "Create or repair links, copies, and optional managed pruning.",
    },
    OperationMetadata {
        id: "sync",
        risk: OperationRisk::Write,
        mutates_files: true,
        supports_json: true,
        description: "Synchronize canonical files, generated outputs, and MCP settings.",
    },
    OperationMetadata {
        id: "doctor",
        risk: OperationRisk::Read,
        mutates_files: false,
        supports_json: true,
        description: "Inspect repository configuration, links, manifest, and drift.",
    },
    OperationMetadata {
        id: "mappings validate",
        risk: OperationRisk::Read,
        mutates_files: false,
        supports_json: true,
        description: "Validate configured symlink, generate, and merge mappings.",
    },
    OperationMetadata {
        id: "operation list",
        risk: OperationRisk::Read,
        mutates_files: false,
        supports_json: true,
        description: "List stable operation metadata for agents and scripts.",
    },
    OperationMetadata {
        id: "schema list",
        risk: OperationRisk::Read,
        mutates_files: false,
        supports_json: true,
        description: "List bundled machine-readable schemas.",
    },
    OperationMetadata {
        id: "schema print",
        risk: OperationRisk::Read,
        mutates_files: false,
        supports_json: true,
        description: "Print one bundled machine-readable schema.",
    },
    OperationMetadata {
        id: "version",
        risk: OperationRisk::Read,
        mutates_files: false,
        supports_json: true,
        description: "Print build and release metadata.",
    },
];

pub(crate) fn metadata(operation: &str) -> Option<OperationMetadata> {
    OPERATION_CATALOG
        .iter()
        .copied()
        .find(|entry| entry.id == operation)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandEffect {
    pub(crate) operation: &'static str,
    pub(crate) mutates_files: bool,
    pub(crate) json_output: bool,
}

pub(crate) fn classify(command: &Commands) -> CommandEffect {
    let (operation, mutates_files, json_output) = match command {
        Commands::Init(_) => ("init", true, false),
        Commands::Migrate(args) => ("migrate", !args.check, false),
        Commands::Setup(args) => ("setup", !args.check, false),
        Commands::Sync(args) => ("sync", !args.check, args.json),
        Commands::Doctor(args) => ("doctor", false, args.json),
        Commands::Mappings(MappingsCommand {
            command: MappingsSubcommand::Validate(args),
        }) => ("mappings validate", false, args.json),
        Commands::Operation(OperationCommand {
            command: OperationSubcommand::List(args),
        }) => ("operation list", false, args.json),
        Commands::Schema(SchemaCommand { command }) => match command {
            SchemaSubcommand::List(args) => ("schema list", false, args.json),
            SchemaSubcommand::Print(_) => ("schema print", false, true),
        },
        Commands::Version(args) => ("version", false, args.json),
    };

    assert!(
        metadata(operation).is_some(),
        "CLI operation must be registered: {operation}"
    );
    CommandEffect {
        operation,
        mutates_files,
        json_output,
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::*;
    use crate::Cli;

    fn effect(args: &[&str]) -> CommandEffect {
        let cli = Cli::try_parse_from(args).expect("command should parse");
        classify(&cli.command)
    }

    #[test]
    fn classifies_mutation_and_output_modes_before_dispatch() {
        assert_eq!(
            effect(&["ags", "sync"]),
            CommandEffect {
                operation: "sync",
                mutates_files: true,
                json_output: false,
            }
        );
        assert_eq!(
            effect(&["ags", "sync", "--check", "--json"]),
            CommandEffect {
                operation: "sync",
                mutates_files: false,
                json_output: true,
            }
        );
        assert_eq!(
            effect(&["ags", "doctor", "--json"]),
            CommandEffect {
                operation: "doctor",
                mutates_files: false,
                json_output: true,
            }
        );
    }

    #[test]
    fn catalog_has_unique_ids_and_covers_every_cli_leaf() {
        let mut ids = OPERATION_CATALOG
            .iter()
            .map(|entry| entry.id.to_owned())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), OPERATION_CATALOG.len());

        let mut leaves = Vec::new();
        collect_leaf_commands(&Cli::command(), "", &mut leaves);
        leaves.sort_unstable();
        assert_eq!(leaves, ids);
    }

    fn collect_leaf_commands(command: &clap::Command, prefix: &str, leaves: &mut Vec<String>) {
        for subcommand in command.get_subcommands() {
            let path = if prefix.is_empty() {
                subcommand.get_name().to_owned()
            } else {
                format!("{prefix} {}", subcommand.get_name())
            };
            if subcommand.get_subcommands().next().is_none() {
                leaves.push(path);
            } else {
                collect_leaf_commands(subcommand, &path, leaves);
            }
        }
    }

    #[test]
    fn operation_metadata_exposes_safety_and_json_capabilities() {
        let sync = metadata("sync").expect("sync metadata");
        assert_eq!(sync.risk, OperationRisk::Write);
        assert!(sync.mutates_files);
        assert!(sync.supports_json);

        let doctor = metadata("doctor").expect("doctor metadata");
        assert_eq!(doctor.risk, OperationRisk::Read);
        assert!(!doctor.mutates_files);
        assert!(doctor.supports_json);
    }
}
