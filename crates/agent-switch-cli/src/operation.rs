//! Command effect classification used before dispatch.

use super::{Commands, MappingsCommand, MappingsSubcommand};

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
        Commands::Version(args) => ("version", false, args.json),
    };

    CommandEffect {
        operation,
        mutates_files,
        json_output,
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

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
}
