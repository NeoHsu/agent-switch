//! Command effect classification used before dispatch.

use super::{Commands, MappingsCommand, MappingsSubcommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandEffect {
    pub(crate) mutates_files: bool,
    pub(crate) json_output: bool,
}

pub(crate) fn classify(command: &Commands) -> CommandEffect {
    let (mutates_files, json_output) = match command {
        Commands::Init(_) => (true, false),
        Commands::Migrate(args) => (!args.check, false),
        Commands::Setup(args) => (!args.check, false),
        Commands::Sync(args) => (!args.check, args.json),
        Commands::Doctor(args) => (false, args.json),
        Commands::Mappings(MappingsCommand {
            command: MappingsSubcommand::Validate(args),
        }) => (false, args.json),
        Commands::Version(args) => (false, args.json),
    };

    CommandEffect {
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
                mutates_files: true,
                json_output: false,
            }
        );
        assert_eq!(
            effect(&["ags", "sync", "--check", "--json"]),
            CommandEffect {
                mutates_files: false,
                json_output: true,
            }
        );
        assert_eq!(
            effect(&["ags", "doctor", "--json"]),
            CommandEffect {
                mutates_files: false,
                json_output: true,
            }
        );
    }
}
