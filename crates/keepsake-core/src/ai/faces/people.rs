//! Name and find (ADR-040, step 3): name a face, then find the person's
//! other faces, which the user confirms or rejects.
//!
//! - [`name_face`]: the face is confirmed as that person (a person is one
//!   per name, any case; a new name adds one).
//! - [`suggest`]: every face with no person, not rejected for this one,
//!   is scored by its likeness to the person's **nearest** confirmed face
//!   (the best cosine similarity to any of them, not to an average: one
//!   average face fails across decades). Faces from at least
//!   [`SAME_PERSON`] are suggested, best first, at most one a photo, and
//!   never in a photo where the person is confirmed already.
//! - Only faces in photos taken within [`YEARS_STEP`] years of a confirmed
//!   face are looked at (and photos with no date). Confirming faces from
//!   2015 lets the next [`suggest`] (Find more) reach 2012, then 2009:
//!   this is how a child's photos are reached from an adult's.
//! - [`decide`]: the user's answers. A rejected face is not suggested for
//!   that person again.
//!
//! Nothing here is applied without the user: suggestions stay suggestions
//! until confirmed. A confirmed face (or a rejection) is written to the
//! photo's sidecar and keywords at once (`MetadataService::sync_faces`).

use std::collections::{HashMap, HashSet};

use super::{MODEL_ID, SAME_PERSON};
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

const YEAR_MS: i64 = 365 * 24 * 60 * 60 * 1000 + 6 * 60 * 60 * 1000;

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

/// Is a photo taken at `at` within reach of the confirmed faces' `dates`?
fn in_reach(at: Option<i64>, dates: &[i64]) -> bool {
    match at {
        None => true,
        Some(t) => dates.is_empty() || dates.iter().any(|d| (t - d).abs() <= YEARS_STEP * YEAR_MS),
    }
}

/// Find `person`'s faces among the faces with no person: see the module.
pub fn suggest(cat: &mut Catalog, person: i64) -> Result<SuggestOutcome, AiError> {
    let (confirmed, candidates) = cat.read(|tx| {
        Ok::<_, CatalogError>((
            tx.faces().of_person(person, FaceState::Confirmed)?,
            tx.faces().candidates(MODEL_ID, person)?,
        ))
    })?;
    if confirmed.is_empty() {
        return Err(AiError::Inference(
            "name one of this person's faces first".into(),
        ));
    }
    let refs: Vec<Embedding> = confirmed
        .iter()
        .filter(|f| f.model == MODEL_ID)
        .filter_map(embedding)
        .collect();
    let dates: Vec<i64> = confirmed.iter().filter_map(|f| f.capture_ms).collect();
    let theirs: HashSet<AssetId> = confirmed.iter().map(|f| f.asset_id).collect();
    // The best face of each photo.
    let mut best: HashMap<AssetId, (i64, f32)> = HashMap::new();
    for c in &candidates {
        if theirs.contains(&c.asset_id) || !in_reach(c.capture_ms, &dates) {
            continue;
        }
        let Some(e) = embedding(c) else { continue };
        let likeness = refs.iter().map(|r| r.cosine(&e)).fold(f32::MIN, f32::max);
        if likeness < SAME_PERSON {
            continue;
        }
        let slot = best.entry(c.asset_id).or_insert((c.id, likeness));
        if likeness > slot.1 {
            *slot = (c.id, likeness);
        }
    }
    // Not in a photo where the person is suggested already (one a photo).
    let suggested_in: HashSet<AssetId> = cat
        .read(|tx| tx.faces().of_person(person, FaceState::Suggested))?
        .iter()
        .map(|f| f.asset_id)
        .collect();
    let picks: Vec<(i64, f32)> = best
        .into_iter()
        .filter(|(a, _)| !suggested_in.contains(a))
        .map(|(_, v)| v)
        .collect();
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
    Ok(SuggestOutcome {
        added,
        waiting,
        confirmed: confirmed.len() as u64,
        from_year: dates
            .iter()
            .min()
            .and_then(|d| year_of(d - YEARS_STEP * YEAR_MS)),
        to_year: dates
            .iter()
            .max()
            .and_then(|d| year_of(d + YEARS_STEP * YEAR_MS)),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_is_a_few_years_around_the_confirmed_dates() {
        let y2015 = 1_420_070_400_000; // 2015-01-01
        let dates = [y2015];
        assert!(in_reach(None, &dates), "undated photos are looked at");
        assert!(in_reach(Some(y2015 - 2 * YEAR_MS), &dates));
        assert!(!in_reach(Some(y2015 - 4 * YEAR_MS), &dates));
        assert!(in_reach(Some(y2015 + 3 * YEAR_MS - 1), &dates));
        assert!(in_reach(Some(0), &[]), "no dated face: every year");
        assert_eq!(year_of(y2015), Some(2015));
    }
}
