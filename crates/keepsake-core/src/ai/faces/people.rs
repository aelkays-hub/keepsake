//! Name and find (ADR-040, step 3): name a face, then find the person's
//! other faces, which the user confirms or rejects.
//!
//! - [`name_face`]: the face is confirmed as that person (a person is one
//!   per name, any case; a new name adds one).
//! - [`suggest`]: every face with no person, not rejected for this one,
//!   is scored by its likeness to the person's **average face from around
//!   the same years** (the confirmed faces within [`YEARS_STEP`] years of
//!   the photo; tested on the product owner's library: one bad
//!   confirmation no longer pulls in strangers, as matching the single
//!   nearest face did). Faces scoring at least the cut-off
//!   ([`DEFAULT_MATCH`], the user can make it stricter or looser) are
//!   suggested, best first, at most one a photo, never in a photo where the
//!   person is confirmed already, and not when the face looks more like a
//!   face rejected for this person than like them. Faces narrower than
//!   [`MIN_MATCH_PX`] in the searched picture are not used.
//! - Only faces in photos taken within [`YEARS_STEP`] years of a confirmed
//!   face are looked at (and photos with no date). Confirming faces from
//!   2015 lets the next [`suggest`] (Find more) reach 2012, then 2009:
//!   this is how a child's photos are reached from an adult's.
//! - Each [`suggest`] scores the person's waiting suggestions again
//!   ([`rescore`]): those under the cut-off are dropped.
//! - [`check`]: a person's confirmed faces, least like the others first, to
//!   find mistakes.
//! - [`decide`]: the user's answers. A rejected face is not suggested for
//!   that person again.
//!
//! Nothing here is applied without the user: suggestions stay suggestions
//! until confirmed. A confirmed face (or a rejection) is written to the
//! photo's sidecar and keywords at once (`MetadataService::sync_faces`).

use std::collections::{HashMap, HashSet};

use std::collections::BTreeMap;

use super::MODEL_ID;
use crate::ai::embedding::Embedding;
use crate::ai::AiError;
use crate::catalog::{AssetId, Catalog, CatalogError, FaceRow, FaceState};
use crate::library::Library;
use crate::metadata::MetadataService;

/// The sidecars and keywords of these photos follow their faces.
fn sync(lib: &Library, cat: &mut Catalog, photos: &[AssetId]) -> Result<(), AiError> {
    let mut svc = MetadataService::new(lib, cat);
    for p in photos {
        svc.sync_faces(*p).map_err(|e| match e {
            crate::library::LibraryError::Catalog(e) => AiError::Catalog(e),
            e => AiError::Image(e.to_string()),
        })?;
    }
    Ok(())
}

/// How far from a confirmed face's date suggestions are looked for, in
/// years, each time.
pub const YEARS_STEP: i64 = 3;

/// What a [`suggest`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct SuggestOutcome {
    /// Faces suggested this time.
    pub added: u64,
    /// The person's suggestions waiting for an answer, in all.
    pub waiting: u64,
    /// Their confirmed faces (what suggestions are measured against).
    pub confirmed: u64,
    /// The years looked at, from the confirmed faces' dates (none: no
    /// confirmed face has a date, so every year was).
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
}

/// Name `face` as `name`: it is that person's, confirmed, and any earlier
/// "not this person" for it is forgotten. Returns the person's id.
pub fn name_face(
    lib: &Library,
    cat: &mut Catalog,
    face: i64,
    name: &str,
    now_ms: i64,
) -> Result<i64, AiError> {
    let (person, photo) = cat.write(|tx| {
        let Some(f) = tx.faces().get(face)? else {
            return Err(CatalogError::Invalid(format!("no face {face}")));
        };
        let person = tx.faces().add_person(name, now_ms)?;
        tx.faces().unreject(face, person)?;
        tx.faces()
            .set_person(face, Some((person, FaceState::Confirmed)))?;
        Ok((person, f.asset_id))
    })?;
    sync(lib, cat, &[photo])?;
    Ok(person)
}

fn embedding(f: &FaceRow) -> Option<Embedding> {
    Embedding::from_quantized(&f.face.embedding, f.face.emb_scale).ok()
}

fn year_of(ms: i64) -> Option<i32> {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| chrono::Datelike::year(&d))
}

/// The cut-off used until the user picks another: a face must be at least
/// this like the person's average face (cosine) to be suggested.
pub const DEFAULT_MATCH: f32 = 0.55;

/// The stricter/looser control's range.
pub const MATCH_RANGE: (f32, f32) = (0.40, 0.75);

/// Faces narrower than this in the searched picture (pixels) are not used
/// to find a person: their description is mostly noise.
pub const MIN_MATCH_PX: f32 = 60.0;

/// A cut-off kept in range.
pub fn clamp_match(m: f32) -> f32 {
    if m.is_finite() {
        m.clamp(MATCH_RANGE.0, MATCH_RANGE.1)
    } else {
        DEFAULT_MATCH
    }
}

fn year_from(ms: Option<i64>) -> Option<i32> {
    ms.and_then(year_of)
}

fn big_enough(f: &FaceRow) -> bool {
    f.width_px().is_none_or(|w| w >= MIN_MATCH_PX)
}

/// A person's confirmed faces as sums of their descriptions per year, to
/// get the average face of any few years at once; and the faces said not
/// to be them.
struct Matcher {
    by_year: BTreeMap<i32, (Vec<f32>, usize)>,
    all: (Vec<f32>, usize),
    negatives: Vec<Embedding>,
}

fn add(sum: &mut (Vec<f32>, usize), v: &[f32]) {
    if sum.0.is_empty() {
        sum.0 = vec![0.0; v.len()];
    }
    if sum.0.len() == v.len() {
        for (a, b) in sum.0.iter_mut().zip(v) {
            *a += b;
        }
        sum.1 += 1;
    }
}

impl Matcher {
    fn new(confirmed: &[FaceRow], rejected: &[FaceRow]) -> Option<Self> {
        let usable: Vec<&FaceRow> = confirmed.iter().filter(|f| f.model == MODEL_ID).collect();
        let good: Vec<&FaceRow> = usable.iter().copied().filter(|f| big_enough(f)).collect();
        // Small faces only if there is nothing better.
        let refs = if good.is_empty() { usable } else { good };
        let mut m = Matcher {
            by_year: BTreeMap::new(),
            all: (Vec::new(), 0),
            negatives: rejected.iter().filter_map(embedding).collect(),
        };
        for f in refs {
            let Some(e) = embedding(f) else { continue };
            add(&mut m.all, e.values());
            if let Some(y) = year_from(f.capture_ms) {
                add(m.by_year.entry(y).or_insert((Vec::new(), 0)), e.values());
            }
        }
        (m.all.1 > 0).then_some(m)
    }

    /// The average face for a photo taken in `year` (every year: undated,
    /// or no confirmed face has a date), leaving `minus` out; `None` when
    /// no confirmed face is near enough in time.
    fn average(
        &self,
        year: Option<i32>,
        minus: Option<(&[f32], Option<i32>)>,
    ) -> Option<Embedding> {
        let mut sum: (Vec<f32>, usize) = (Vec::new(), 0);
        match year.filter(|_| !self.by_year.is_empty()) {
            None => sum = self.all.clone(),
            Some(y) => {
                let step = YEARS_STEP as i32;
                for (_, s) in self.by_year.range(y - step..=y + step) {
                    if sum.0.is_empty() {
                        sum.0 = vec![0.0; s.0.len()];
                    }
                    for (a, b) in sum.0.iter_mut().zip(&s.0) {
                        *a += b;
                    }
                    sum.1 += s.1;
                }
            }
        }
        if let Some((v, vy)) = minus {
            let inside = match (year, vy) {
                (Some(y), Some(fy)) if !self.by_year.is_empty() => {
                    (fy - y).abs() <= YEARS_STEP as i32
                }
                _ => year.is_none() || self.by_year.is_empty(),
            };
            if inside && sum.1 > 0 && sum.0.len() == v.len() {
                for (a, b) in sum.0.iter_mut().zip(v) {
                    *a -= b;
                }
                sum.1 -= 1;
            }
        }
        if sum.1 == 0 {
            return None;
        }
        Embedding::new(sum.0).ok()
    }

    /// How much `f` looks like the person, or `None` when it is out of
    /// reach, too small, or looks more like a face rejected for them.
    fn score(&self, f: &FaceRow, cut: f32) -> Option<f32> {
        if !big_enough(f) {
            return None;
        }
        let e = embedding(f)?;
        let avg = self.average(year_from(f.capture_ms), None)?;
        let like = avg.cosine(&e);
        if like < cut {
            return Some(like);
        }
        let unlike = self
            .negatives
            .iter()
            .map(|n| n.cosine(&e))
            .fold(f32::MIN, f32::max);
        if unlike > like {
            return None;
        }
        Some(like)
    }

    /// The years looked at.
    fn years(&self) -> (Option<i32>, Option<i32>) {
        let step = YEARS_STEP as i32;
        (
            self.by_year.keys().next().map(|y| y - step),
            self.by_year.keys().next_back().map(|y| y + step),
        )
    }
}

fn matcher(cat: &Catalog, person: i64) -> Result<(Vec<FaceRow>, Option<Matcher>), AiError> {
    let (confirmed, rejected) = cat.read(|tx| {
        Ok::<_, CatalogError>((
            tx.faces().of_person(person, FaceState::Confirmed)?,
            tx.faces().rejected_faces(person)?,
        ))
    })?;
    let m = Matcher::new(&confirmed, &rejected);
    Ok((confirmed, m))
}

/// Score `person`'s waiting suggestions again with cut-off `cut`: those
/// under it (or no longer in reach) are taken off. Returns how many were.
pub fn rescore(cat: &mut Catalog, person: i64, cut: f32) -> Result<u64, AiError> {
    let cut = clamp_match(cut);
    let (_, m) = matcher(cat, person)?;
    let waiting = cat.read(|tx| tx.faces().of_person(person, FaceState::Suggested))?;
    let mut off = Vec::new();
    let mut keep = Vec::new();
    for f in &waiting {
        match m.as_ref().and_then(|m| m.score(f, cut)) {
            Some(l) if l >= cut => keep.push((f.id, l)),
            _ => off.push(f.id),
        }
    }
    cat.write(|tx| {
        for id in &off {
            tx.faces().set_person(*id, None)?;
        }
        for (id, l) in &keep {
            tx.faces().set_likeness(*id, *l)?;
        }
        Ok::<_, CatalogError>(())
    })?;
    Ok(off.len() as u64)
}

/// [`rescore`] for everyone. Returns how many suggestions were taken off.
pub fn rescore_all(cat: &mut Catalog, cut: f32) -> Result<u64, AiError> {
    let people = cat.read(|tx| tx.faces().people())?;
    let mut n = 0;
    for p in people {
        n += rescore(cat, p.id, cut)?;
    }
    Ok(n)
}

/// Find `person`'s faces among the faces with no person, with cut-off
/// `cut` (see the module). Their waiting suggestions are scored again
/// first.
pub fn suggest(cat: &mut Catalog, person: i64, cut: f32) -> Result<SuggestOutcome, AiError> {
    let cut = clamp_match(cut);
    rescore(cat, person, cut)?;
    let (confirmed, m) = matcher(cat, person)?;
    if confirmed.is_empty() {
        return Err(AiError::Inference(
            "name one of this person's faces first".into(),
        ));
    }
    let candidates = cat.read(|tx| tx.faces().candidates(MODEL_ID, person))?;
    let theirs: HashSet<AssetId> = confirmed.iter().map(|f| f.asset_id).collect();
    let suggested_in: HashSet<AssetId> = cat
        .read(|tx| tx.faces().of_person(person, FaceState::Suggested))?
        .iter()
        .map(|f| f.asset_id)
        .collect();
    // The best face of each photo.
    let mut best: HashMap<AssetId, (i64, f32)> = HashMap::new();
    if let Some(m) = &m {
        for c in &candidates {
            if theirs.contains(&c.asset_id) || suggested_in.contains(&c.asset_id) {
                continue;
            }
            let Some(like) = m.score(c, cut).filter(|l| *l >= cut) else {
                continue;
            };
            let slot = best.entry(c.asset_id).or_insert((c.id, like));
            if like > slot.1 {
                *slot = (c.id, like);
            }
        }
    }
    let picks: Vec<(i64, f32)> = best.into_values().collect();
    let added = picks.len() as u64;
    cat.write(|tx| {
        for (face, likeness) in &picks {
            tx.faces().suggest(*face, person, *likeness)?;
        }
        Ok::<_, CatalogError>(())
    })?;
    let waiting = cat
        .read(|tx| tx.faces().of_person(person, FaceState::Suggested))?
        .len() as u64;
    let (from_year, to_year) = m.as_ref().map_or((None, None), Matcher::years);
    Ok(SuggestOutcome {
        added,
        waiting,
        confirmed: confirmed.len() as u64,
        from_year,
        to_year,
    })
}

/// `person`'s confirmed faces with how much each looks like the others
/// from around its years, least alike first: the likely mistakes. Faces
/// with no description yet (read from a sidecar) are left out.
pub fn check(cat: &Catalog, person: i64) -> Result<Vec<(FaceRow, f32)>, AiError> {
    let (confirmed, m) = matcher(cat, person)?;
    let Some(m) = m else { return Ok(Vec::new()) };
    let mut out: Vec<(FaceRow, f32)> = Vec::new();
    for f in confirmed {
        if f.model != MODEL_ID {
            continue;
        }
        let Some(e) = embedding(&f) else { continue };
        let year = year_from(f.capture_ms);
        // Leave the face itself out of the average (when it is in it).
        let minus = big_enough(&f).then_some((e.values(), year));
        let Some(avg) = m.average(year, minus) else {
            continue;
        };
        let like = avg.cosine(&e);
        out.push((f, like));
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    Ok(out)
}

/// The person's suggestions waiting for an answer, best first.
pub fn review(cat: &Catalog, person: i64) -> Result<Vec<FaceRow>, AiError> {
    Ok(cat.read(|tx| tx.faces().of_person(person, FaceState::Suggested))?)
}

/// The user's answers: `confirm` are `person`'s faces, `reject` are not
/// (and are not suggested for them again). Faces not suggested for this
/// person are left alone. Returns `(confirmed, rejected)`.
pub fn decide(
    lib: &Library,
    cat: &mut Catalog,
    person: i64,
    confirm: &[i64],
    reject: &[i64],
) -> Result<(u64, u64), AiError> {
    let (n, mut photos) = cat.write(|tx| {
        let mut n = (0, 0);
        let mut photos: Vec<AssetId> = Vec::new();
        for id in confirm {
            if let Some(f) = tx.faces().get(*id)?.filter(|f| f.person == Some(person)) {
                tx.faces()
                    .set_person(*id, Some((person, FaceState::Confirmed)))?;
                n.0 += 1;
                photos.push(f.asset_id);
            }
        }
        for id in reject {
            if let Some(f) = tx.faces().get(*id)?.filter(|f| f.person == Some(person)) {
                tx.faces().reject(*id, person)?;
                n.1 += 1;
                photos.push(f.asset_id);
            }
        }
        Ok::<_, CatalogError>((n, photos))
    })?;
    photos.sort();
    photos.dedup();
    sync(lib, cat, &photos)?;
    Ok(n)
}

/// "Not this person": the face's person is taken off it, and it is not
/// suggested for them again. Returns whether it had one.
pub fn not_this_person(lib: &Library, cat: &mut Catalog, face: i64) -> Result<bool, AiError> {
    let Some(f) = cat.read(|tx| tx.faces().get(face))? else {
        return Ok(false);
    };
    let Some(person) = f.person else {
        return Ok(false);
    };
    cat.write(|tx| tx.faces().reject(face, person))?;
    sync(lib, cat, &[f.asset_id])?;
    Ok(true)
}

/// "Not a face": hidden for good (a new search does not find it again).
pub fn not_a_face(lib: &Library, cat: &mut Catalog, face: i64) -> Result<(), AiError> {
    let Some(f) = cat.read(|tx| tx.faces().get(face))? else {
        return Ok(());
    };
    cat.write(|tx| tx.faces().hide(face))?;
    if f.person.is_some() {
        sync(lib, cat, &[f.asset_id])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cut_off_stays_in_range() {
        assert_eq!(clamp_match(0.1), MATCH_RANGE.0);
        assert_eq!(clamp_match(0.9), MATCH_RANGE.1);
        assert_eq!(clamp_match(f32::NAN), DEFAULT_MATCH);
        assert_eq!(clamp_match(0.6), 0.6);
        assert_eq!(year_of(1_420_070_400_000), Some(2015));
    }
}
