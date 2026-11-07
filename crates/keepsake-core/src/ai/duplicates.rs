//! Same-day duplicates: bursts, re-shoots and copies of one picture.
//!
//! Found on demand, one calendar day (capture local date) at a time, from
//! the stored analyses; nothing about the groups is stored. Two items of
//! the same kind (photo with photo, video with video) are linked when
//! - their difference hashes are within [`dhash::NEAR`] bits (the same
//!   picture, re-encoded or resized), or
//! - their embeddings' cosine is at least [`COSINE_SAME`], or
//!   [`COSINE_BURST`] when they are in the same burst/bracket run or were
//!   taken within [`BURST_GAP_MS`] of each other.
//!
//! Linked items are joined into groups (A~B and B~C put A, B, C together).
//! An item the user marked "not a duplicate" links to nothing. Big days
//! compare each item only with those taken within [`WINDOW_MS`] of it.

use serde::Serialize;

use super::dhash;
use super::{AiError, MODEL_ID};
use crate::catalog::{AssetId, Catalog, DayItem, DuplicateChoice, MediaKind};

/// Cosine at or above which two items count as the same shot.
pub const COSINE_SAME: f32 = 0.93;
/// The lower bar for items of one burst, or taken moments apart.
pub const COSINE_BURST: f32 = 0.88;
/// "Moments apart".
pub const BURST_GAP_MS: i64 = 15_000;
/// Days with more items than this compare only items close in time.
pub const ALL_PAIRS_MAX: usize = 1500;
/// "Close in time" on big days.
pub const WINDOW_MS: i64 = 2 * 60 * 60 * 1000;

/// One item of a group.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DuplicateMember {
    pub asset_id: AssetId,
    pub media_kind: MediaKind,
    pub capture_ms: Option<i64>,
    pub rating: i64,
    pub choice: Option<DuplicateChoice>,
    /// Cosine to the suggested keeper (1 for the keeper itself).
    pub similarity: f32,
    /// Same picture by difference hash as the suggested keeper.
    pub exact: bool,
}

/// A group of near-identical items, in capture order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DuplicateGroup {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub members: Vec<DuplicateMember>,
    /// The user's keeper if chosen, else the suggestion: best rated, then
    /// the earliest.
    pub keeper: AssetId,
    /// The user has picked a keeper, or every member is decided.
    pub decided: bool,
}

/// One day that has groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DuplicateDay {
    pub date: String,
    pub groups: u64,
    /// Items in those groups.
    pub items: u64,
    /// Groups still waiting for a decision.
    pub undecided: u64,
}

/// Every day with groups, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct DuplicateSummary {
    pub days: Vec<DuplicateDay>,
    pub groups: u64,
    pub items: u64,
    pub undecided: u64,
}

/// Cosine of two stored (int8) embeddings.
fn cosine(a: &DayItem, b: &DayItem) -> f32 {
    let (x, y) = (&a.analysis, &b.analysis);
    if x.embedding.len() != y.embedding.len() {
        return 0.0;
    }
    let dot: i32 = x
        .embedding
        .iter()
        .zip(&y.embedding)
        .map(|(&p, &q)| i32::from(p) * i32::from(q))
        .sum();
    dot as f32 * x.emb_scale * y.emb_scale
}

fn close_in_time(a: &DayItem, b: &DayItem, within: i64) -> bool {
    match (a.capture_ms, b.capture_ms) {
        (Some(x), Some(y)) => (x - y).abs() <= within,
        _ => false,
    }
}

/// Are these two the same shot?
pub fn linked(a: &DayItem, b: &DayItem) -> bool {
    if a.media_kind != b.media_kind {
        return false;
    }
    if a.choice == Some(DuplicateChoice::NotDuplicate)
        || b.choice == Some(DuplicateChoice::NotDuplicate)
    {
        return false;
    }
    if dhash::distance(a.analysis.dhash, b.analysis.dhash) <= dhash::NEAR {
        return true;
    }
    let burst =
        (a.sequence.is_some() && a.sequence == b.sequence) || close_in_time(a, b, BURST_GAP_MS);
    cosine(a, b) >= if burst { COSINE_BURST } else { COSINE_SAME }
}

struct Sets(Vec<usize>);

impl Sets {
    fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }
    fn join(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra.max(rb)] = ra.min(rb);
        }
    }
}

/// Groups of two or more among one day's items (indexes into `items`, in
/// order). `items` must be in capture order.
pub fn group_indexes(items: &[DayItem]) -> Vec<Vec<usize>> {
    let n = items.len();
    let mut sets = Sets((0..n).collect());
    let all_pairs = n <= ALL_PAIRS_MAX;
    for i in 0..n {
        for j in (i + 1)..n {
            if !all_pairs {
                // In capture order: past the window, nothing later is close.
                match (items[i].capture_ms, items[j].capture_ms) {
                    (Some(a), Some(b)) if b - a > WINDOW_MS => break,
                    (Some(_), Some(_)) => {}
                    _ => continue,
                }
            }
            if linked(&items[i], &items[j]) {
                sets.join(i, j);
            }
        }
    }
    let mut by_root: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for i in 0..n {
        let r = sets.find(i);
        by_root.entry(r).or_default().push(i);
    }
    by_root.into_values().filter(|g| g.len() >= 2).collect()
}

fn make_group(date: &str, items: &[DayItem], idx: &[usize]) -> Option<DuplicateGroup> {
    let chosen = idx
        .iter()
        .copied()
        .find(|&i| items[i].choice == Some(DuplicateChoice::Keeper));
    let best = chosen.or_else(|| {
        idx.iter().copied().max_by(|&a, &b| {
            items[a]
                .rating
                .cmp(&items[b].rating)
                // Earlier wins a tie: reverse the index order.
                .then_with(|| b.cmp(&a))
        })
    })?;
    let k = &items[best];
    let members = idx
        .iter()
        .map(|&i| {
            let it = &items[i];
            DuplicateMember {
                asset_id: it.analysis.asset_id,
                media_kind: it.media_kind,
                capture_ms: it.capture_ms,
                rating: it.rating,
                choice: it.choice,
                similarity: if i == best { 1.0 } else { cosine(k, it) },
                exact: dhash::distance(k.analysis.dhash, it.analysis.dhash) <= dhash::NEAR,
            }
        })
        .collect();
    Some(DuplicateGroup {
        date: date.to_owned(),
        members,
        keeper: k.analysis.asset_id,
        decided: chosen.is_some() || idx.iter().all(|&i| items[i].choice.is_some()),
    })
}

/// The groups of one day.
pub fn day_groups(cat: &Catalog, date: &str) -> Result<Vec<DuplicateGroup>, AiError> {
    let items = cat.read(|tx| tx.ai().day_items(date, MODEL_ID))?;
    Ok(group_indexes(&items)
        .iter()
        .filter_map(|g| make_group(date, &items, g))
        .collect())
}

/// Every day with groups, newest first.
pub fn summary(cat: &Catalog) -> Result<DuplicateSummary, AiError> {
    summary_in(cat, None, None)
}

/// The days from `from` to `to` (`YYYY-MM-DD`, inclusive; `None`: no
/// bound) that have groups, newest first.
pub fn summary_in(
    cat: &Catalog,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<DuplicateSummary, AiError> {
    let dates = cat.read(|tx| tx.ai().days_with_pairs(MODEL_ID, from, to))?;
    let mut out = DuplicateSummary::default();
    for date in dates {
        let groups = day_groups(cat, &date)?;
        if groups.is_empty() {
            continue;
        }
        let day = DuplicateDay {
            groups: groups.len() as u64,
            items: groups.iter().map(|g| g.members.len() as u64).sum(),
            undecided: groups.iter().filter(|g| !g.decided).count() as u64,
            date,
        };
        out.groups += day.groups;
        out.items += day.items;
        out.undecided += day.undecided;
        out.days.push(day);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::Embedding;
    use crate::catalog::{AiAnalysis, SequenceId};

    /// A vector at cosine `cos` from basis direction `dir` (towards `dir + 1`).
    fn emb(dir: u32, cos: f32) -> (Vec<i8>, f32) {
        let mut v = vec![0.0f32; 64];
        v[dir as usize] = cos;
        v[dir as usize + 1] = (1.0 - cos * cos).max(0.0).sqrt();
        Embedding::new(v).unwrap().quantize()
    }

    fn item(n: u8, dir: u32, cos: f32, dhash: u64, at_s: i64) -> DayItem {
        let (embedding, emb_scale) = emb(dir, cos);
        DayItem {
            analysis: AiAnalysis {
                asset_id: AssetId(uuid::Uuid::from_bytes([n; 16])),
                model: MODEL_ID.into(),
                embedding,
                emb_scale,
                dhash,
                made_ms: 0,
            },
            media_kind: MediaKind::Photo,
            capture_ms: Some(at_s * 1000),
            rating: 0,
            sequence: None,
            choice: None,
        }
    }

    #[test]
    fn similar_items_group_and_different_ones_do_not() {
        let items = vec![
            item(1, 0, 1.0, 0x0F0F, 0),
            item(2, 0, 0.97, 0xF0F0_0000, 600), // close vector, other hash
            item(3, 20, 1.0, 0x00FF_0000_0000, 1200),
            item(4, 40, 1.0, 0x0F0E, 5000), // other vector, hash 1 bit from item 1
        ];
        let groups = group_indexes(&items);
        assert_eq!(groups, vec![vec![0, 1, 3]]);
    }

    #[test]
    fn photos_and_videos_never_mix() {
        let a = item(1, 0, 1.0, 7, 0);
        let mut b = item(2, 0, 1.0, 7, 1);
        b.media_kind = MediaKind::Video;
        assert!(!linked(&a, &b));
    }

    #[test]
    fn not_a_duplicate_links_to_nothing() {
        let a = item(1, 0, 1.0, 7, 0);
        let mut b = item(2, 0, 1.0, 7, 1);
        assert!(linked(&a, &b));
        b.choice = Some(DuplicateChoice::NotDuplicate);
        assert!(!linked(&a, &b));
    }

    #[test]
    fn bursts_use_the_lower_bar() {
        let a = item(1, 0, 1.0, 1, 0);
        let b = item(2, 0, 0.905, u64::MAX << 32, 3600);
        let c = cosine(&a, &b);
        assert!((COSINE_BURST..COSINE_SAME).contains(&c), "{c}");
        assert!(!linked(&a, &b));
        let mut b2 = b.clone();
        b2.capture_ms = Some(5_000);
        assert!(linked(&a, &b2), "moments apart");
        let (mut a3, mut b3) = (a.clone(), b.clone());
        let seq = SequenceId(uuid::Uuid::from_bytes([9; 16]));
        a3.sequence = Some(seq);
        b3.sequence = Some(seq);
        assert!(linked(&a3, &b3), "same burst");
    }

    #[test]
    fn keeper_is_the_users_pick_else_best_rated_then_earliest() {
        let mut items = vec![
            item(1, 0, 1.0, 7, 0),
            item(2, 0, 1.0, 7, 1),
            item(3, 0, 1.0, 7, 2),
        ];
        let g = make_group("2026-11-07", &items, &[0, 1, 2]).unwrap();
        assert_eq!(g.keeper, items[0].analysis.asset_id);
        assert!(!g.decided);
        items[2].rating = 4;
        let g = make_group("2026-11-07", &items, &[0, 1, 2]).unwrap();
        assert_eq!(g.keeper, items[2].analysis.asset_id);
        items[1].choice = Some(DuplicateChoice::Keeper);
        let g = make_group("2026-11-07", &items, &[0, 1, 2]).unwrap();
        assert_eq!(g.keeper, items[1].analysis.asset_id);
        assert!(g.decided);
        assert_eq!(g.members[1].similarity, 1.0);
    }

    #[test]
    fn big_days_compare_within_the_window_only() {
        let mut items: Vec<DayItem> = (0..ALL_PAIRS_MAX + 2)
            .map(|i| {
                item(
                    (i % 250) as u8,
                    2 * (i as u32 % 31),
                    1.0,
                    (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
                    i as i64,
                )
            })
            .collect();
        // Two copies of one shot, far apart in time: not compared.
        let far = WINDOW_MS / 1000 + 10_000;
        items.push(item(251, 62, 1.0, 0x5555, far));
        items.push(item(252, 62, 1.0, 0x5555, far + far));
        items.sort_by_key(|i| i.capture_ms);
        let groups = group_indexes(&items);
        let n = items.len();
        assert!(!groups
            .iter()
            .any(|g| g.contains(&(n - 1)) && g.contains(&(n - 2))));
    }
}
