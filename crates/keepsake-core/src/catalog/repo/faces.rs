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
    /// A suggestion's likeness to the person's confirmed faces.
    pub likeness: Option<f32>,
    /// When its photo was taken (UTC ms), if known.
    pub capture_ms: Option<i64>,
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

/// A "not this person": the person's name and the face's box (x, y, w, h,
/// 0–1).
pub type RejectionBox = (String, f32, f32, f32, f32);

/// The model id of a named face read from a sidecar, before its photo is
/// searched: it has no description, and is never used to find others.
pub const RESTORED_MODEL: &str = "sidecar";

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

const FACE_COLS: &str = "f.id, f.asset_id, f.x, f.y, f.w, f.h, f.landmarks, f.score, f.model,
    f.embedding, f.emb_scale, f.person_id, f.state, f.likeness,
    (SELECT a.capture_ts_ms FROM assets a WHERE a.id = f.asset_id)";

fn face_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FaceRow> {
    let lm: Vec<u8> = r.get(6)?;
    let state: String = r.get(12)?;
    let bad = |i: usize, m: String| {
        rusqlite::Error::FromSqlConversionFailure(
            i,
            rusqlite::types::Type::Text,
            Box::new(CatalogError::Invalid(m)),
        )
    };
    let landmarks = blob_landmarks(&lm).map_err(|e| bad(6, e.to_string()))?;
    let state = FaceState::parse(&state).ok_or_else(|| bad(12, format!("face state {state}")))?;
    let f = |i: usize| -> rusqlite::Result<f32> { Ok(r.get::<_, f64>(i)? as f32) };
    Ok(FaceRow {
        id: r.get(0)?,
        asset_id: r.get(1)?,
        face: NewFace {
            x: f(2)?,
            y: f(3)?,
            w: f(4)?,
            h: f(5)?,
            landmarks,
            score: f(7)?,
            embedding: blob_i8(r.get(9)?),
            emb_scale: f(10)?,
        },
        model: r.get(8)?,
        person: r.get(11)?,
        state,
        likeness: r.get::<_, Option<f64>>(13)?.map(|v| v as f32),
        capture_ms: r.get(14)?,
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
            "SELECT {FACE_COLS} FROM faces f WHERE f.asset_id = ?1 ORDER BY f.score DESC, f.id"
        ))?;
        let v = st.query_map([asset], face_row)?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// One face.
    pub fn get(&self, id: i64) -> Result<Option<FaceRow>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {FACE_COLS} FROM faces f WHERE f.id = ?1"),
                [id],
                face_row,
            )
            .optional()?)
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

    /// A person's faces in `state` (suggested: best first; else newest
    /// photo first).
    pub fn of_person(&self, person: i64, state: FaceState) -> Result<Vec<FaceRow>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {FACE_COLS} FROM faces f WHERE f.person_id = ?1 AND f.state = ?2
              ORDER BY f.likeness DESC, 15 DESC, f.id"
        ))?;
        let v = st
            .query_map(params![person, state.as_str()], face_row)?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Faces of `model` with no person, and not rejected for `person`: the
    /// ones that may be suggested for them.
    pub fn candidates(&self, model: &str, person: i64) -> Result<Vec<FaceRow>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {FACE_COLS} FROM faces f
              WHERE f.state = 'unknown' AND f.model = ?1
                AND NOT EXISTS (SELECT 1 FROM face_rejections r
                                 WHERE r.face_id = f.id AND r.person_id = ?2)"
        ))?;
        let v = st
            .query_map(params![model, person], face_row)?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// One person.
    pub fn person(&self, id: i64) -> Result<Option<Person>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, created_ms FROM people WHERE id = ?1",
                [id],
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

    /// The names of the people confirmed in a photo, sorted, once each.
    pub fn confirmed_names(&self, asset: AssetId) -> Result<Vec<String>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT DISTINCT p.name FROM faces f JOIN people p ON p.id = f.person_id
              WHERE f.asset_id = ?1 AND f.state = 'confirmed' ORDER BY p.name_key",
        )?;
        let v = st
            .query_map([asset], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// A photo's confirmed faces with their person's name, by position.
    pub fn confirmed_regions(
        &self,
        asset: AssetId,
    ) -> Result<Vec<(String, NewFace)>, CatalogError> {
        let mut out = Vec::new();
        for f in self.for_asset(asset)? {
            if f.state != FaceState::Confirmed {
                continue;
            }
            if let Some(p) = f.person.map(|p| self.person(p)).transpose()?.flatten() {
                out.push((p.name, f.face));
            }
        }
        out.sort_by(|a, b| a.1.x.total_cmp(&b.1.x).then(a.0.cmp(&b.0)));
        Ok(out)
    }

    /// "Not this person" on a photo, as (person's name, box x, y, w, h):
    /// those on its faces and those waiting for a search.
    pub fn rejection_boxes(&self, asset: AssetId) -> Result<Vec<RejectionBox>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT p.name, f.x, f.y, f.w, f.h FROM face_rejections r
               JOIN faces f ON f.id = r.face_id JOIN people p ON p.id = r.person_id
              WHERE f.asset_id = ?1
             UNION ALL
             SELECT p.name, q.x, q.y, q.w, q.h FROM face_rejections_pending q
               JOIN people p ON p.id = q.person_id
              WHERE q.asset_id = ?1
             ORDER BY 1, 2, 3",
        )?;
        let v = st
            .query_map([asset], |r| {
                let f = |i: usize| -> rusqlite::Result<f32> { Ok(r.get::<_, f64>(i)? as f32) };
                Ok((r.get(0)?, f(1)?, f(2)?, f(3)?, f(4)?))
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Photos with a named face or a "not this person".
    pub fn photos_with_decisions(&self) -> Result<Vec<AssetId>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT asset_id FROM faces WHERE person_id IS NOT NULL
             UNION SELECT f.asset_id FROM face_rejections r JOIN faces f ON f.id = r.face_id
             UNION SELECT asset_id FROM face_rejections_pending",
        )?;
        let v = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(v)
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
            if let Some(k) = kept.iter().find(|k| box_iou(&k.face, f) > SAME_FACE_IOU) {
                // A named face read from a sidecar (no description yet), or
                // one from older models: it takes this search's face, and
                // keeps its person.
                if k.model != model {
                    self.conn.execute(
                        "UPDATE faces SET landmarks = ?2, score = ?3, model = ?4, embedding = ?5,
                                          emb_scale = ?6
                          WHERE id = ?1",
                        params![
                            k.id,
                            landmarks_blob(&f.landmarks),
                            f64::from(f.score.clamp(0.0, 1.0)),
                            model,
                            i8_blob(&f.embedding),
                            f64::from(f.emb_scale),
                        ],
                    )?;
                }
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
            let id = self.conn.last_insert_rowid();
            self.apply_pending_rejections(asset, id, f)?;
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

    /// Rejections waiting for a face at `f`'s place become that face's.
    fn apply_pending_rejections(
        &self,
        asset: AssetId,
        face: i64,
        f: &NewFace,
    ) -> Result<(), CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT rowid, person_id, x, y, w, h FROM face_rejections_pending WHERE asset_id = ?1",
        )?;
        let rows: Vec<(i64, i64, NewFace)> = st
            .query_map([asset], |r| {
                let g = |i: usize| -> rusqlite::Result<f32> { Ok(r.get::<_, f64>(i)? as f32) };
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    NewFace {
                        x: g(2)?,
                        y: g(3)?,
                        w: g(4)?,
                        h: g(5)?,
                        landmarks: [[0.0; 2]; 5],
                        score: 0.0,
                        embedding: Vec::new(),
                        emb_scale: 0.0,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?;
        for (rowid, person, b) in rows {
            if box_iou(&b, f) > SAME_FACE_IOU {
                self.conn.execute(
                    "INSERT OR IGNORE INTO face_rejections (face_id, person_id) VALUES (?1, ?2)",
                    params![face, person],
                )?;
                self.conn.execute(
                    "DELETE FROM face_rejections_pending WHERE rowid = ?1",
                    [rowid],
                )?;
            }
        }
        Ok(())
    }

    /// A named face read from a sidecar (a rebuild): the photo's face at
    /// that place becomes the person's, or, if it was not searched yet, one
    /// is kept for it (with no description until the photo is searched).
    pub fn restore_confirmed(
        &self,
        asset: AssetId,
        person: i64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> Result<(), CatalogError> {
        let at = NewFace {
            x,
            y,
            w,
            h,
            landmarks: [[0.0; 2]; 5],
            score: 0.0,
            embedding: vec![0],
            emb_scale: 0.0,
        };
        let here = self.for_asset(asset)?;
        if let Some(f) = here.iter().find(|f| box_iou(&f.face, &at) > SAME_FACE_IOU) {
            if f.person != Some(person) || f.state != FaceState::Confirmed {
                self.set_person(f.id, Some((person, FaceState::Confirmed)))?;
            }
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO faces (asset_id, x, y, w, h, landmarks, score, model, embedding,
                                emb_scale, person_id, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, 0, ?9, 'confirmed')",
            params![
                asset,
                f64::from(x.clamp(0.0, 1.0)),
                f64::from(y.clamp(0.0, 1.0)),
                f64::from(w.clamp(f32::MIN_POSITIVE, 1.0)),
                f64::from(h.clamp(f32::MIN_POSITIVE, 1.0)),
                landmarks_blob(&at.landmarks),
                RESTORED_MODEL,
                i8_blob(&at.embedding),
                person,
            ],
        )?;
        Ok(())
    }

    /// A "not this person" read from a sidecar: on the face at that place,
    /// or kept until a search finds one there.
    pub fn restore_rejection(
        &self,
        asset: AssetId,
        person: i64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> Result<(), CatalogError> {
        let at = NewFace {
            x,
            y,
            w,
            h,
            landmarks: [[0.0; 2]; 5],
            score: 0.0,
            embedding: Vec::new(),
            emb_scale: 0.0,
        };
        if let Some(f) = self
            .for_asset(asset)?
            .iter()
            .find(|f| box_iou(&f.face, &at) > SAME_FACE_IOU)
        {
            return self.reject(f.id, person);
        }
        let dup: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM face_rejections_pending
                             WHERE asset_id = ?1 AND person_id = ?2
                               AND abs(x - ?3) < 1e-4 AND abs(y - ?4) < 1e-4)",
            params![asset, person, f64::from(x), f64::from(y)],
            |r| r.get(0),
        )?;
        if !dup {
            self.conn.execute(
                "INSERT INTO face_rejections_pending (asset_id, person_id, x, y, w, h)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    asset,
                    person,
                    f64::from(x),
                    f64::from(y),
                    f64::from(w),
                    f64::from(h)
                ],
            )?;
        }
        Ok(())
    }

    /// Take every name off every face and forget every "not this person"
    /// (the first half of Delete all face data: the sidecars and keywords
    /// follow, then [`Faces::delete_all`]).
    pub fn forget_decisions(&self) -> Result<(), CatalogError> {
        self.conn.execute("DELETE FROM face_rejections", [])?;
        self.conn
            .execute("DELETE FROM face_rejections_pending", [])?;
        self.conn.execute(
            "UPDATE faces SET person_id = NULL, state = 'unknown', likeness = NULL
              WHERE person_id IS NOT NULL",
            [],
        )?;
        Ok(())
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
            "UPDATE faces SET person_id = ?2, state = ?3, likeness = NULL WHERE id = ?1",
            params![face, person, state.as_str()],
        )?;
        Ok(())
    }

    /// Suggest `face` for `person`, with its likeness.
    pub fn suggest(&self, face: i64, person: i64, likeness: f32) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE faces SET person_id = ?2, state = 'suggested', likeness = ?3
              WHERE id = ?1 AND state = 'unknown'",
            params![face, person, f64::from(likeness)],
        )?;
        Ok(())
    }

    /// Forget that `face` was rejected for `person` (the user named it so).
    pub fn unreject(&self, face: i64, person: i64) -> Result<(), CatalogError> {
        self.conn.execute(
            "DELETE FROM face_rejections WHERE face_id = ?1 AND person_id = ?2",
            params![face, person],
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
            "UPDATE faces SET person_id = NULL, state = 'unknown', likeness = NULL
              WHERE id = ?1 AND person_id = ?2",
            params![face, person],
        )?;
        Ok(())
    }

    /// Forget every face, scan, person and decision (Delete all face data).
    /// Returns how many faces there were.
    pub fn delete_all(&self) -> Result<u64, CatalogError> {
        self.conn.execute("DELETE FROM face_rejections", [])?;
        self.conn
            .execute("DELETE FROM face_rejections_pending", [])?;
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
