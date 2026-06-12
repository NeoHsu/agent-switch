use std::path::{Path, PathBuf};

use crate::{config::Config, fs::abs, tool::Tool};

#[derive(Debug)]
pub(super) struct SyncContext<'a> {
    pub root: &'a Path,
    pub cfg: &'a Config,
    pub tools: Option<&'a [Tool]>,
    pub check: bool,
}

impl<'a> SyncContext<'a> {
    pub(crate) fn new(
        root: &'a Path,
        cfg: &'a Config,
        tools: Option<&'a [Tool]>,
        check: bool,
    ) -> Self {
        Self {
            root,
            cfg,
            tools,
            check,
        }
    }

    pub(crate) fn abs(&self, path: &Path) -> PathBuf {
        abs(self.root, path)
    }
}
