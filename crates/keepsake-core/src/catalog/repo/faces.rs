use rusqlite::{params, OptionalExtension};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, Rw};

define_repo!(
    /// Faces found in photos, people and the user's decisions about them
    /// (ADR-040).
    Faces
);

/// What is known about a face's person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaceState {
    /// No person.
    Unknown,
    /// Found by Name and find; the user has not confirmed it.
    Suggested,
    /// The user said who it is.
    Confirmed,
}

impl FaceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Suggested => "suggested",
            Self::Confirmed => "confirmed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Unknown, Self::Suggested, Self::Confirmed]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

/// A face to store: box and landmarks in 0–1 units of the picture, the
/// detector's score, the embedding as int8 with its scale.
#[derive(Debug, Clone, PartialEq)]
pub struct NewFace {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub landmarks: [[f32; 2]; 5],
    pub score: f32,
    pub embedding: Vec<i8>,
    pub emb_scale: f32,
}

/// A stored face.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceRow {
    pub id: i64,
    pub asset_id: AssetId,
    pub face: NewFace,
    pub model: String,
    pub person: Option<i64>,
    pub state: FaceState,
}

/// An asset's face scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceScan {
    pub model: String,
    /// The picture searched (the thumbnail), in pixels.
    pub width: u32,
    pub height: u32,
    pub made_ms: i64,
}

/// A named person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: i64,
    pub name: String,
    pub created_ms: i64,
}

/// How far finding faces has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct FaceCounts {
    /// Photos (not videos) in the library.
    pub photos: u64,
    /// Of those, searched for faces with the current models.
    pub scanned: u64,
    /// Of those, with at least one face.
    pub with_faces: u64,
    /// Faces found.
    pub faces: u64,
    /// Named people.
    pub people: u64,
    /// Face jobs still to run (queued or running).
    pub pending: u64,
    /// Face jobs that failed for good.
    pub failed: u64,
}

/// Two boxes overlapping more than this are the same face (when scanning
/// again keeps the faces the user named).
pub const SAME_FACE_IOU: f32 = 0.5;

fn landmarks_blob(l: &[[f32; 2]; 5]) -> Vec<u8> {
    l.iter()
        .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
        .collect()
}

fn blob_landmarks(b: &[u8]) -> Result<[[f32; 2]; 5], CatalogError> {
    if b.len() != 40 {
        return Err(CatalogError::Invalid(format!(
            "face landmarks are {} bytes, not 40",
            b.len()
        )));
    }
    let mut out = [[0f32; 2]; 5];
    for (i, c) in b.as_chunks::<4>().0.iter().enumerate() {
        out[i / 2][i % 2] = f32::from_le_bytes(*c);
    }
    Ok(out)
}

fn i8_blob(v: &[i8]) -> Vec<u8> {
    v.iter().map(|b| b.to_ne_bytes()[0]).collect()
}

fn blob_i8(v: Vec<u8>) -> Vec<i8> {
    v.into_iter().map(|b| i8::from_ne_bytes([b])).collect()
}

/// The overlap of two boxes, intersection over union.
pub fn box_iou(a: &NewFace, b: &NewFace) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// A name as compared: trimmed, lower case.
pub fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

type RawFace = (
    i64,
    AssetId,
    (f64, f64, f64, f64),
    Vec<u8>,
    f64,
    String,
    Vec<u8>,
    f64,
    Option<i64>,
    String,
);

const FACE_COLS: &str = "id, asset_id, x, y, w, h, landmarks, score, model, embedding, emb_scale,
                         person_id, state";

fn raw_face(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawFace> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        (r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?),
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
        r.get(11)?,
        r.get(12)?,
    ))
}

fn face_row(raw: RawFace) -> Result<FaceRow, CatalogError> {
    let (id, asset_id, (x, y, w, h), lm, score, model, emb, scale, person, state) = raw;
    Ok(FaceRow {
        id,
        asset_id,
        face: NewFace {
            x: x as f32,
            y: y as f32,
            w: w as f32,
            h: h as f32,
            landmarks: blob_landmarks(&lm)?,
            score: score as f32,
            embedding: blob_i8(emb),
            emb_scale: scale as f32,
        },
        model,
        person,
        state: FaceState::parse(&state)
            .ok_or_else(|| CatalogError::Invalid(format!("face state {state}")))?,
    })
}

impl<M> Faces<'_, M> {
    /// The asset's face scan, if it was searched.
    pub fn scan(&self, asset: AssetId) -> Result<Option<FaceScan>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT model, width, height, made_ms FROM face_scans WHERE asset_id = ?1",
                [asset],
                |r| {
                    Ok(FaceScan {
                        model: r.get(0)?,
                        width: r.get(1)?,
                        height: r.get(2)?,
                        made_ms: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// The asset's faces, best first.
    pub fn for_asset(&self, asset: AssetId) -> Result<Vec<FaceRow>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {FACE_COLS} FROM faces WHERE asset_id = ?1 ORDER BY score DESC, id"
        ))?;
        let raw: Vec<RawFace> = st.query_map([asset], raw_face)?.collect::<Result<_, _>>()?;
        raw.into_iter().map(face_row).collect()
    }

    /// One face.
    pub fn get(&self, id: i64) -> Result<Option<FaceRow>, CatalogError> {
        let raw = self
            .conn
            .query_row(
                &format!("SELECT {FACE_COLS} FROM faces WHERE id = ?1"),
                [id],
                raw_face,
            )
            .optional()?;
        raw.map(face_row).transpose()
    }

    /// Photos not yet searched for faces with `model`, newest first.
    pub fn unscanned_photos(&self, model: &str) -> Result<Vec<AssetId>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT a.id FROM assets a
              WHERE a.media_kind = 'photo'
                AND NOT EXISTS (SELECT 1 FROM face_scans s
                                 WHERE s.asset_id = a.id AND s.model = ?1)
              ORDER BY a.capture_ts_ms DESC, a.id",
        )?;
        let ids = st
            .query_map([model], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// How far finding faces with `model` has got; `job_kind`'s jobs are
    /// the pending and failed ones.
    pub fn counts(&self, model: &str, job_kind: &str) -> Result<FaceCounts, CatalogError> {
        let n = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<u64, CatalogError> {
            let v: i64 = self.conn.query_row(sql, p, |r| r.get(0))?;
            super::to_u64(v, "count")
        };
        let (pending, failed): (i64, i64) = self.conn.query_row(
            "SELECT coalesce(sum(state IN ('queued','running')), 0),
                    coalesce(sum(state = 'failed'), 0)
               FROM jobs WHERE kind = ?1",
            [job_kind],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(FaceCounts {
            photos: n(
                "SELECT count(*) FROM assets WHERE media_kind = 'photo'",
                &[],
            )?,
            scanned: n(
                "SELECT count(*) FROM face_scans WHERE model = ?1",
                &[&model],
            )?,
            with_faces: n(
                "SELECT count(DISTINCT asset_id) FROM faces WHERE model = ?1",
                &[&model],
            )?,
            faces: n("SELECT count(*) FROM faces WHERE model = ?1", &[&model])?,
            people: n("SELECT count(*) FROM people", &[])?,
            pending: super::to_u64(pending, "count")?,
            failed: super::to_u64(failed, "count")?,
        })
    }

    /// Everyone named, by name.
    pub fn people(&self) -> Result<Vec<Person>, CatalogError> {
        let mut st = self
            .conn
            .prepare("SELECT id, name, created_ms FROM people ORDER BY name_key, id")?;
        let v = st
            .query_map([], |r| {
                Ok(Person {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_ms: r.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// The person with this name (any case, spaces around it ignored).
    pub fn person_named(&self, name: &str) -> Result<Option<Person>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, created_ms FROM people WHERE name_key = ?1",
                [name_key(name)],
                |r| {
                    Ok(Person {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        created_ms: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// The people a face was rejected for.
    pub fn rejected_for(&self, face: i64) -> Result<Vec<i64>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT person_id FROM face_rejections WHERE face_id = ?1 ORDER BY person_id",
        )?;
        let v = st
            .query_map([face], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }
}

impl Faces<'_, Rw> {
    /// Store the result of searching `asset`'s picture (`width` × `height`)
    /// for faces with `model`. Faces with no person are replaced; faces the
    /// user named (or that were suggested) stay, and a new face on top of
    /// one of them (overlapping more than [`SAME_FACE_IOU`]) is the same
    /// face, so it is not added again. Returns how many faces were added.
    pub fn save_scan(
        &self,
        asset: AssetId,
        model: &str,
        width: u32,
        height: u32,
        faces: &[NewFace],
        now_ms: i64,
    ) -> Result<u64, CatalogError> {
        self.conn.execute(
            "DELETE FROM faces WHERE asset_id = ?1 AND state = 'unknown'",
            [asset],
        )?;
        let kept = self.for_asset(asset)?;
        let mut added = 0;
        for f in faces {
            if kept.iter().any(|k| box_iou(&k.face, f) > SAME_FACE_IOU) {
                continue;
            }
            self.conn.execute(
                "INSERT INTO faces (asset_id, x, y, w, h, landmarks, score, model, embedding,
                                    emb_scale)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    asset,
                    f64::from(f.x.clamp(0.0, 1.0)),
                    f64::from(f.y.clamp(0.0, 1.0)),
                    f64::from(f.w),
                    f64::from(f.h),
                    landmarks_blob(&f.landmarks),
                    f64::from(f.score.clamp(0.0, 1.0)),
                    model,
                    i8_blob(&f.embedding),
                    f64::from(f.emb_scale),
                ],
            )?;
            added += 1;
        }
        self.conn.execute(
            "INSERT INTO face_scans (asset_id, model, width, height, made_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(asset_id) DO UPDATE SET model = excluded.model,
               width = excluded.width, height = excluded.height, made_ms = excluded.made_ms",
            params![asset, model, width, height, now_ms],
        )?;
        Ok(added)
    }

    /// The person named `name`, added if new. Returns their id.
    pub fn add_person(&self, name: &str, now_ms: i64) -> Result<i64, CatalogError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CatalogError::Invalid("a person needs a name".into()));
        }
        if let Some(p) = self.person_named(name)? {
            return Ok(p.id);
        }
        self.conn.execute(
            "INSERT INTO people (name, name_key, created_ms) VALUES (?1, ?2, ?3)",
            params![name, name_key(name), now_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Say whose face it is (`state` suggested or confirmed), or that it is
    /// nobody's (`None`).
    pub fn set_person(
        &self,
        face: i64,
        person: Option<(i64, FaceState)>,
    ) -> Result<(), CatalogError> {
        let (person, state) = match person {
            Some((_, FaceState::Unknown)) => {
                return Err(CatalogError::Invalid(
                    "a face with a person is suggested or confirmed".into(),
                ))
            }
            Some((p, s)) => (Some(p), s),
            None => (None, FaceState::Unknown),
        };
        self.conn.execute(
            "UPDATE faces SET person_id = ?2, state = ?3 WHERE id = ?1",
            params![face, person, state.as_str()],
        )?;
        Ok(())
    }

    /// Record that `face` is not `person` (and take it off them).
    pub fn reject(&self, face: i64, person: i64) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO face_rejections (face_id, person_id) VALUES (?1, ?2)",
            params![face, person],
        )?;
        self.conn.execute(
            "UPDATE faces SET person_id = NULL, state = 'unknown'
              WHERE id = ?1 AND person_id = ?2",
            params![face, person],
        )?;
        Ok(())
    }

    /// Forget every face, scan, person and decision (Delete all face data).
    /// Returns how many faces there were.
    pub fn delete_all(&self) -> Result<u64, CatalogError> {
        self.conn.execute("DELETE FROM face_rejections", [])?;
        let n = self.conn.execute("DELETE FROM faces", [])?;
        self.conn.execute("DELETE FROM face_scans", [])?;
        self.conn.execute("DELETE FROM people", [])?;
        Ok(n as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landmarks_go_in_and_out_unchanged() {
        let l = [[0.1, 0.2], [0.3, 0.4], [0.5, 0.6], [0.7, 0.8], [0.9, 1.0]];
        assert_eq!(blob_landmarks(&landmarks_blob(&l)).unwrap(), l);
        assert!(blob_landmarks(&[0; 12]).is_err());
    }

    #[test]
    fn states_and_names() {
        for s in [
            FaceState::Unknown,
            FaceState::Suggested,
            FaceState::Confirmed,
        ] {
            assert_eq!(FaceState::parse(s.as_str()), Some(s));
        }
        assert_eq!(FaceState::parse("rejected"), None);
        assert_eq!(name_key("  Sara "), "sara");
    }
}
