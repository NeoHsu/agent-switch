use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{CommandOutput, ExitCode, output};

use super::{
    SyncOptions,
    event::{SyncEvent, SyncEventKind},
};

#[derive(Debug, Default)]
pub(super) struct SyncReport {
    records: Vec<SyncRecord>,
    max_events: Option<usize>,
}

#[derive(Debug, Clone)]
struct SyncRecord {
    sequence: usize,
    event: SyncEvent,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct SyncSummary {
    pub total_events: usize,
    pub imported: usize,
    pub generated: usize,
    pub removed: usize,
    pub copied: usize,
    pub merged: usize,
    pub warnings: usize,
    pub drift: usize,
    pub synced_no_changes: usize,
}

#[derive(Debug, Serialize)]
struct SyncJsonOptions {
    check: bool,
    import_only: bool,
    export_only: bool,
    reset_manifest: bool,
    json: bool,
    event_filter: Option<Vec<String>>,
    max_files: Option<usize>,
    max_source_bytes: Option<u64>,
    max_output_bytes: Option<u64>,
    max_events: Option<usize>,
}

#[derive(Debug, Serialize)]
struct SyncOutputPayload {
    exit: String,
    exit_code: i32,
    changed: bool,
    summary: SyncSummary,
    options: SyncJsonOptions,
    events: Vec<SyncJsonEvent>,
}

#[derive(Debug, Serialize)]
struct SyncJsonEvent {
    sequence: usize,
    #[serde(flatten)]
    event: SyncEvent,
}

impl SyncReport {
    pub(super) fn with_max_events(max_events: Option<usize>) -> Self {
        Self {
            records: Vec::new(),
            max_events,
        }
    }

    pub(super) fn push(&mut self, event: SyncEvent) -> Result<()> {
        if let Some(max_events) = self.max_events
            && self.records.len() >= max_events
        {
            bail!("sync event limit exceeded: more than {max_events} events would be emitted");
        }
        self.records.push(SyncRecord {
            sequence: self.records.len() + 1,
            event,
        });
        Ok(())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(super) fn into_output(self, filter: Option<&[SyncEventKind]>) -> CommandOutput {
        let mut out = CommandOutput::default();
        for record in self.filtered_records(filter) {
            out.push(record.event.as_line());
        }
        out
    }

    fn summary(events: &[SyncEvent]) -> SyncSummary {
        let mut summary = SyncSummary {
            total_events: events.len(),
            ..SyncSummary::default()
        };

        for event in events {
            match event {
                SyncEvent::Imported { .. } => summary.imported += 1,
                SyncEvent::Generated { .. } => summary.generated += 1,
                SyncEvent::Removed { .. } => summary.removed += 1,
                SyncEvent::Copied { .. } => summary.copied += 1,
                SyncEvent::Warning { .. } => summary.warnings += 1,
                SyncEvent::Merged { .. } => summary.merged += 1,
                SyncEvent::Drift => summary.drift += 1,
                SyncEvent::SyncedNoChanges => summary.synced_no_changes += 1,
            }
        }

        summary
    }

    fn filtered_records(&self, filter: Option<&[SyncEventKind]>) -> Vec<SyncRecord> {
        let mut records = self.records.clone();

        if let Some(filter) = filter {
            let filter: HashSet<SyncEventKind> = filter.iter().copied().collect();
            records.retain(|record| filter.contains(&record.event.kind()));
        }

        records.sort_by(|left, right| {
            let left_key = (
                left.event.kind().sort_order(),
                left.event.as_line(),
                left.sequence,
            );
            let right_key = (
                right.event.kind().sort_order(),
                right.event.as_line(),
                right.sequence,
            );
            left_key.cmp(&right_key)
        });

        records
    }

    pub(super) fn into_json(
        self,
        changed: bool,
        opts: &SyncOptions,
        exit: ExitCode,
    ) -> Result<String> {
        let records = self.filtered_records(opts.event_filter.as_deref());
        let events = records
            .iter()
            .map(|record| record.event.clone())
            .collect::<Vec<_>>();
        let summary = Self::summary(&events);
        let events = records
            .into_iter()
            .map(|record| SyncJsonEvent {
                sequence: record.sequence,
                event: record.event,
            })
            .collect();
        let options = SyncJsonOptions {
            check: opts.check,
            import_only: opts.import_only,
            export_only: opts.export_only,
            reset_manifest: opts.reset_manifest,
            json: opts.json,
            event_filter: opts
                .event_filter
                .as_ref()
                .map(|filter| filter.iter().map(|kind| kind.to_string()).collect()),
            max_files: opts.max_files,
            max_source_bytes: opts.max_source_bytes,
            max_output_bytes: opts.max_output_bytes,
            max_events: opts.max_events,
        };

        let payload = SyncOutputPayload {
            exit: format!("{exit:?}"),
            exit_code: exit.code(),
            changed,
            summary,
            options,
            events,
        };

        output::render_json(&payload).context("failed to serialize sync events")
    }
}
