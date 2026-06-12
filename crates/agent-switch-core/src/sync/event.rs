use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEventKind {
    Imported,
    Generated,
    Removed,
    Copied,
    Warning,
    Merged,
    Drift,
    SyncedNoChanges,
}

impl SyncEventKind {
    pub(crate) const fn sort_order(self) -> u8 {
        match self {
            Self::Imported => 0,
            Self::Generated => 1,
            Self::Removed => 2,
            Self::Copied => 3,
            Self::Warning => 4,
            Self::Merged => 5,
            Self::Drift => 6,
            Self::SyncedNoChanges => 7,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::Generated => "generated",
            Self::Removed => "removed",
            Self::Copied => "copied",
            Self::Warning => "warning",
            Self::Merged => "merged",
            Self::Drift => "drift",
            Self::SyncedNoChanges => "synced_no_changes",
        }
    }
}

impl fmt::Display for SyncEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SyncEventKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "imported" => Ok(Self::Imported),
            "generated" => Ok(Self::Generated),
            "removed" => Ok(Self::Removed),
            "copied" => Ok(Self::Copied),
            "warning" => Ok(Self::Warning),
            "merged" => Ok(Self::Merged),
            "drift" => Ok(Self::Drift),
            "synced_no_changes" | "syncednochanges" => Ok(Self::SyncedNoChanges),
            _ => Err(Error::Config(format!(
                "unknown sync event filter: {value}; supported: {}",
                Self::all()
                    .iter()
                    .map(|kind| kind.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
}

impl SyncEventKind {
    pub(crate) fn all() -> &'static [Self] {
        const ALL: [SyncEventKind; 8] = [
            SyncEventKind::Imported,
            SyncEventKind::Generated,
            SyncEventKind::Removed,
            SyncEventKind::Copied,
            SyncEventKind::Warning,
            SyncEventKind::Merged,
            SyncEventKind::Drift,
            SyncEventKind::SyncedNoChanges,
        ];
        &ALL
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "details", rename_all = "snake_case")]
pub(super) enum SyncEvent {
    Imported {
        dest: String,
        src: String,
        conflict: bool,
    },
    Generated {
        dest: String,
    },
    Removed {
        path: String,
    },
    Copied {
        from: String,
        to: String,
    },
    Warning {
        message: String,
    },
    Merged {
        path: String,
    },
    Drift,
    SyncedNoChanges,
}

impl SyncEvent {
    pub(super) fn as_line(&self) -> String {
        match self {
            Self::Imported {
                dest,
                src,
                conflict,
            } => {
                if *conflict {
                    format!("imported(conflict, tool-side wins): {dest} -> {src}")
                } else {
                    format!("imported: {dest} -> {src}")
                }
            }
            Self::Generated { dest } => format!("generated: {dest}"),
            Self::Removed { path } => format!("removed: {path}"),
            Self::Copied { from, to } => format!("copied: {from} -> {to}"),
            Self::Warning { message } => format!("warning: {message}"),
            Self::Merged { path } => format!("merged: {path}"),
            Self::Drift => "--check: drift detected; run `ags sync`".to_string(),
            Self::SyncedNoChanges => "synced, no changes.".to_string(),
        }
    }

    pub(crate) fn kind(&self) -> SyncEventKind {
        match self {
            Self::Imported { .. } => SyncEventKind::Imported,
            Self::Generated { .. } => SyncEventKind::Generated,
            Self::Removed { .. } => SyncEventKind::Removed,
            Self::Copied { .. } => SyncEventKind::Copied,
            Self::Warning { .. } => SyncEventKind::Warning,
            Self::Merged { .. } => SyncEventKind::Merged,
            Self::Drift => SyncEventKind::Drift,
            Self::SyncedNoChanges => SyncEventKind::SyncedNoChanges,
        }
    }
}

pub(crate) fn parse_event_filter(raw: &[String]) -> Result<Vec<SyncEventKind>, Error> {
    let mut filter = Vec::new();

    for raw_filter in raw {
        for entry in raw_filter.split(',') {
            let token = entry.trim();
            if token.is_empty() {
                continue;
            }
            let kind = SyncEventKind::from_str(token)?;
            if !filter.contains(&kind) {
                filter.push(kind);
            }
        }
    }

    if filter.is_empty() {
        return Err(Error::Config(
            "--event-filter requires at least one event kind".into(),
        ));
    }

    Ok(filter)
}
