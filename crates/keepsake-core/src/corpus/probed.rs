//! Probe a corpus card and describe each asset the way `expected.toml` does.

use std::path::Path;

use crate::corpus::types::{AssetGroup, FileEntry};
use crate::grouping;
use crate::probe::Prober;
use crate::relpath::RelPath;
use crate::xmp::format_date;

/// One asset as probed: its sorted file list plus the values `expected.toml`
/// records, in the same text form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedGroup {
    pub files: Vec<String>,
    /// `xmp::format_date` form, e.g. `2026-10-14T17:56:12.285-05:00`; empty if unknown.
    pub capture_ts: String,
    pub camera_model: String,
    pub duration_ms: Option<u64>,
    /// Problems and capture-time notes worth showing a human.
    pub notes: Vec<String>,
}

/// Group and probe one camera card of the corpus.
pub fn probe_card(card_root: &Path, files: &[FileEntry], prober: &Prober) -> Vec<ProbedGroup> {
    let rels: Vec<RelPath> = files
        .iter()
        .filter_map(|f| RelPath::parse(&f.rel_path).ok())
        .collect();
    let (layout, _) = grouping::read_avchd_layout(card_root, &rels);
    let g = grouping::group(&rels, layout.as_ref());
    let report = prober.probe(card_root, &rels, &g);
    g.groups
        .iter()
        .zip(report.assets)
        .map(|(grp, a)| {
            let mut files: Vec<String> = grp
                .members
                .iter()
                .map(|m| rels[m.index].as_str().to_owned())
                .collect();
            files.sort();
            let mut notes = a.problems.clone();
            if let Some(c) = &a.capture {
                notes.push(format!("capture: {} ({})", c.source, c.confidence));
                notes.extend(c.notes.iter().cloned());
            }
            ProbedGroup {
                files,
                capture_ts: a
                    .capture
                    .as_ref()
                    .map(|c| format_date(c.time))
                    .unwrap_or_default(),
                camera_model: a.model.unwrap_or_default(),
                duration_ms: a.duration_ms.and_then(|d| u64::try_from(d).ok()),
                notes,
            }
        })
        .collect()
}

/// Fill blank, unconfirmed entries from a probe and mark them `prefilled`.
/// Confirmed entries are never touched.
pub fn prefill(entries: &mut [AssetGroup], probed: &[ProbedGroup]) -> usize {
    let mut n = 0;
    for e in entries.iter_mut().filter(|e| !e.confirmed && !e.removed) {
        let mut key = e.files.clone();
        key.sort();
        if let Some(p) = probed.iter().find(|p| p.files == key) {
            e.capture_ts = p.capture_ts.clone();
            e.camera_model = p.camera_model.clone();
            e.duration_ms = p.duration_ms;
            e.prefilled = true;
            n += 1;
        }
    }
    n
}
