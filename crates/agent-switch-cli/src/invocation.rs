//! Per-invocation state shared by CLI command dispatch.

use std::path::{Path, PathBuf};

use agent_switch_core::{
    config::{self, Config},
    fs::RepositoryLock,
    tool::Tool,
};
use anyhow::Result;

/// Immutable verbosity settings for one CLI invocation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Verbosity {
    pub(crate) verbose: bool,
    pub(crate) debug: bool,
}

/// Resolved runtime state for one command.
///
/// Keeping root/config/tool resolution and the mutation lock together prevents
/// individual handlers from accidentally using different repository context.
#[derive(Debug)]
pub(crate) struct Invocation {
    pub(crate) root: PathBuf,
    config_path: Option<PathBuf>,
    tools: Option<Vec<Tool>>,
    pub(crate) verbosity: Verbosity,
    _repository_lock: Option<RepositoryLock>,
}

impl Invocation {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_cli(
        root_arg: Option<&Path>,
        config_path: Option<PathBuf>,
        tool_filter: Option<&str>,
        init_tools: Option<&str>,
        mutates_files: bool,
        verbose: bool,
        debug: bool,
    ) -> Result<Self> {
        let root = config::find_root(root_arg)?;
        let tools = tool_filter.map(config::parse_tools).transpose()?;
        if let Some(init_tools) = init_tools {
            config::parse_tools(init_tools)?;
        }
        let repository_lock = if mutates_files {
            Some(RepositoryLock::acquire(&root)?)
        } else {
            None
        };

        Ok(Self {
            root,
            config_path,
            tools,
            verbosity: Verbosity {
                verbose: verbose || debug,
                debug,
            },
            _repository_lock: repository_lock,
        })
    }

    pub(crate) fn tools(&self) -> Option<&[Tool]> {
        self.tools.as_deref()
    }

    pub(crate) fn config_path_arg(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    pub(crate) fn config_path(&self) -> PathBuf {
        config::resolve_config_path(&self.root, self.config_path_arg())
    }

    pub(crate) fn load_config(&self) -> Result<(Config, PathBuf)> {
        config::load_config(&self.root, self.config_path.as_deref())
    }
}
