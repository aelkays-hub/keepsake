//! Flag bursts, brackets and interval (time-lapse) runs among stills.
//!
//! Each still stays its own asset; sequences are recorded so a later prompt
//! can stack them in the UI.
//!
//! - From Sony EXIF: `ReleaseMode2` says burst or bracket, `SequenceNumber`
//!   counts frames within the run (a run restarts when it stops increasing).
//! - Without those tags: frames less than [`BURST_GAP_MS`] apart form a
//!   burst; three or more frames spaced evenly (within [`INTERVAL_TOLERANCE`]
//!   of their median gap) and at least a second apart form an interval run.

use serde::{Deserialize, Serialize};

pub use crate::catalog::SequenceKind;

/// Largest gap between burst frames.
pub const BURST_GAP_MS: i64 = 1000;
/// Allowed deviation of interval gaps from their median.
pub const INTERVAL_TOLERANCE: f64 = 0.10;

/// What the detector needs about one still.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Caller's identifier (e.g. index into a group list).
    pub id: usize,
    /// Capture time in milliseconds (any consistent clock).
    pub time_ms: i64,
    /// Camera serial or model, so two cameras never mix.
    pub camera: Option<String>,
    pub release_mode2: Option<i64>,
    pub sequence_number: Option<i64>,
}

/// A detected run. `members` are frame ids in capture order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    pub kind: SequenceKind,
    pub members: Vec<usize>,
}

/// Sony `ReleaseMode2` values (ExifTool's Sony tables).
fn sony_kind(release_mode2: i64) -> Option<SequenceKind> {
    match release_mode2 {
        // Continuous, Continuous - Burst, Burst 2, Speed/Advance Priority,
        // Anti-Motion Blur, Continuous Low, High Sensitivity, Tele-zoom.
        1 | 5 | 8 | 17 | 19 | 26 | 27 | 29 => Some(SequenceKind::Burst),
        // Continuous exposure bracketing, DRO/WB bracketing, single-frame bracketing.
        2 | 3 | 23 => Some(SequenceKind::Bracket),
        _ => None,
    }
}

/// Detect sequences. Frames may arrive in any order.
pub fn detect(frames: &[Frame]) -> Vec<Sequence> {
    let mut sorted: Vec<&Frame> = frames.iter().collect();
    sorted.sort_by(|a, b| (&a.camera, a.time_ms, a.id).cmp(&(&b.camera, b.time_ms, b.id)));
    let mut out = Vec::new();
    let mut used = vec![false; sorted.len()];

    // 1. Runs marked by the camera.
    let mut i = 0;
    while i < sorted.len() {
        let Some(kind) = sorted[i].release_mode2.and_then(sony_kind) else {
            i += 1;
            continue;
        };
        let mut j = i + 1;
        while j < sorted.len()
            && sorted[j].camera == sorted[i].camera
            && sorted[j].release_mode2 == sorted[i].release_mode2
            && match (sorted[j - 1].sequence_number, sorted[j].sequence_number) {
                (Some(a), Some(b)) => b > a,
                _ => sorted[j].time_ms - sorted[j - 1].time_ms < BURST_GAP_MS,
            }
        {
            j += 1;
        }
        if j - i >= 2 {
            out.push(Sequence {
                kind,
                members: sorted[i..j].iter().map(|f| f.id).collect(),
            });
            used[i..j].iter_mut().for_each(|u| *u = true);
        }
        i = j;
    }

    // 2. Timing alone, for frames the camera did not mark.
    // Runs of consecutive unmarked frames from one camera (positions in `sorted`).
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for k in 0..sorted.len() {
        let free = !used[k] && sorted[k].release_mode2.and_then(sony_kind).is_none();
        let same_camera = cur
            .last()
            .is_none_or(|&p| sorted[p].camera == sorted[k].camera);
        if (!free || !same_camera) && !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
        if free {
            cur.push(k);
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    for run in runs {
        // Split into bursts first.
        let mut start = 0;
        let mut rest: Vec<usize> = Vec::new();
        for n in 1..=run.len() {
            let gap_ok =
                n < run.len() && sorted[run[n]].time_ms - sorted[run[n - 1]].time_ms < BURST_GAP_MS;
            if !gap_ok {
                if n - start >= 2 {
                    out.push(Sequence {
                        kind: SequenceKind::Burst,
                        members: run[start..n].iter().map(|p| sorted[*p].id).collect(),
                    });
                } else {
                    rest.extend(&run[start..n]);
                }
                start = n;
            }
        }
        out.extend(intervals(&sorted, &rest));
    }
    out.sort_by_key(|s| s.members.first().copied());
    out
}

fn intervals(sorted: &[&Frame], cand: &[usize]) -> Vec<Sequence> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 < cand.len() {
        let gap = |a: usize, b: usize| sorted[cand[b]].time_ms - sorted[cand[a]].time_ms;
        let g0 = gap(i, i + 1);
        if g0 < BURST_GAP_MS || sorted[cand[i]].camera != sorted[cand[i + 1]].camera {
            i += 1;
            continue;
        }
        let tol = (g0 as f64 * INTERVAL_TOLERANCE).max(1.0);
        let mut j = i + 1;
        while j + 1 < cand.len()
            && sorted[cand[j + 1]].camera == sorted[cand[i]].camera
            && ((gap(j, j + 1) - g0).abs() as f64) <= tol
        {
            j += 1;
        }
        if j - i + 1 >= 3 {
            out.push(Sequence {
                kind: SequenceKind::Interval,
                members: cand[i..=j].iter().map(|p| sorted[*p].id).collect(),
            });
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: usize, t: i64, rm: Option<i64>, sn: Option<i64>) -> Frame {
        Frame {
            id,
            time_ms: t,
            camera: Some("cam".into()),
            release_mode2: rm,
            sequence_number: sn,
        }
    }

    #[test]
    fn camera_marked_bursts_restart_on_sequence_number() {
        let frames = vec![
            f(0, 0, Some(0), Some(0)),
            f(1, 10_000, Some(1), Some(1)),
            f(2, 10_100, Some(1), Some(2)),
            f(3, 10_200, Some(1), Some(3)),
            f(4, 10_300, Some(1), Some(1)), // new burst
            f(5, 10_400, Some(1), Some(2)),
            f(6, 20_000, Some(2), Some(1)),
            f(7, 20_300, Some(2), Some(2)),
            f(8, 20_600, Some(2), Some(3)),
        ];
        let s = detect(&frames);
        assert_eq!(
            s,
            vec![
                Sequence {
                    kind: SequenceKind::Burst,
                    members: vec![1, 2, 3]
                },
                Sequence {
                    kind: SequenceKind::Burst,
                    members: vec![4, 5]
                },
                Sequence {
                    kind: SequenceKind::Bracket,
                    members: vec![6, 7, 8]
                },
            ]
        );
    }

    #[test]
    fn timing_bursts_and_intervals() {
        let frames = vec![
            f(0, 0, None, None),
            f(1, 300, None, None),
            f(2, 650, None, None), // burst 0..2
            f(3, 30_000, None, None),
            f(4, 35_000, None, None),
            f(5, 40_100, None, None),
            f(6, 45_050, None, None), // interval 3..6 (5 s ± 10 %)
            f(7, 90_000, None, None), // single
        ];
        let s = detect(&frames);
        assert_eq!(
            s,
            vec![
                Sequence {
                    kind: SequenceKind::Burst,
                    members: vec![0, 1, 2]
                },
                Sequence {
                    kind: SequenceKind::Interval,
                    members: vec![3, 4, 5, 6]
                },
            ]
        );
    }

    #[test]
    fn uneven_spacing_is_not_an_interval() {
        let frames = vec![
            f(0, 0, None, None),
            f(1, 5_000, None, None),
            f(2, 20_000, None, None),
        ];
        assert!(detect(&frames).is_empty());
    }

    #[test]
    fn cameras_never_mix() {
        let mut a = f(0, 0, None, None);
        let mut b = f(1, 100, None, None);
        a.camera = Some("a".into());
        b.camera = Some("b".into());
        assert!(detect(&[a, b]).is_empty());
    }
}
