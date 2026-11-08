//! XMP sidecars: the durable source of truth for user metadata.
//!
//! Field mapping (Lightroom and digiKam compatible):
//!
//! | Data | Written as | Also read |
//! |---|---|---|
//! | rating | `xmp:Rating` | Lightroom `-1` (rejected) is read as 0 |
//! | colour label | `xmp:Label` + `digiKam:ColorLabel` | `digiKam:ColorLabel` when `xmp:Label` is absent |
//! | comment | `dc:description` (`x-default`) | |
//! | keywords | `dc:subject`, `lr:hierarchicalSubject` (`A\|B`), `digiKam:TagsList` (`A/B`) | |
//! | capture time | `exif:DateTimeOriginal`, `photoshop:DateCreated` | `xmp:CreateDate` |
//! | camera / lens | `tiff:Make`, `tiff:Model`, `exifEX:LensModel` | |
//! | duration | `xmpDM:duration` | |
//! | everything else | `keepsake:*` | |

pub(crate) mod dom;
pub mod packet;
pub mod sidecar;

use std::collections::BTreeSet;

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use thiserror::Error;

use crate::catalog::{
    AssetId, Blake3Hash, CaptureSource, CaptureTime, Confidence, DuplicateChoice, Event, EventId,
    FileRole, GpsPoint, MediaKind, Rating, RecipeId, SequenceId, SequenceKind, TagPath,
};
use crate::relpath::{path_key, CaseSensitivity, RelPath};
use packet::{field, Field, ReadField};
pub use packet::{ArrayKind, Value, XmpPacket};

/// Namespace URIs.
pub mod ns {
    pub const X: &str = "adobe:ns:meta/";
    pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    pub const XMP: &str = "http://ns.adobe.com/xap/1.0/";
    pub const DC: &str = "http://purl.org/dc/elements/1.1/";
    pub const LR: &str = "http://ns.adobe.com/lightroom/1.0/";
    pub const DIGIKAM: &str = "http://www.digikam.org/ns/1.0/";
    pub const EXIF: &str = "http://ns.adobe.com/exif/1.0/";
    pub const EXIF_EX: &str = "http://cipa.jp/exif/1.0/";
    pub const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
    pub const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
    pub const XMP_DM: &str = "http://ns.adobe.com/xmp/1.0/DynamicMedia/";
    pub const KEEPSAKE: &str = "http://ns.keepsake.app/xmp/1.0/";
    /// Metadata Working Group regions (face regions; ADR-040).
    pub const MWG_RS: &str = "http://www.metadataworkinggroup.com/schemas/regions/";
    pub const ST_AREA: &str = "http://ns.adobe.com/xmp/sType/Area#";
    pub const ST_DIM: &str = "http://ns.adobe.com/xap/1.0/sType/Dimensions#";

    /// Conventional prefix for a namespace.
    pub fn preferred_prefix(uri: &str) -> &'static str {
        match uri {
            X => "x",
            RDF => "rdf",
            XMP => "xmp",
            DC => "dc",
            LR => "lr",
            DIGIKAM => "digiKam",
            EXIF => "exif",
            EXIF_EX => "exifEX",
            PHOTOSHOP => "photoshop",
            TIFF => "tiff",
            XMP_DM => "xmpDM",
            KEEPSAKE => "keepsake",
            MWG_RS => "mwg-rs",
            ST_AREA => "stArea",
            ST_DIM => "stDim",
            _ => "ns",
        }
    }
}

/// Errors reading or writing XMP.
#[derive(Debug, Error)]
pub enum XmpError {
    #[error("XMP parse error: {0}")]
    Parse(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// A recipe reference stored in the sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeRef {
    pub id: RecipeId,
    pub kind: String,
    pub schema_version: u32,
    /// Relative to the library's `recipes/` directory.
    pub rel_path: RelPath,
    pub content_hash: Blake3Hash,
    pub created_ms: i64,
    pub updated_ms: i64,
}

/// A member file of the asset, as listed in the sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRef {
    /// Relative to the volume root.
    pub rel_path: RelPath,
    pub role: FileRole,
    /// Position of an AVCHD `segment` file (1, 2, ...).
    pub segment_index: Option<u32>,
}

/// The burst/bracket/interval run an asset belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceRef {
    pub id: SequenceId,
    pub kind: SequenceKind,
    pub position: u32,
}

/// A named face, as an MWG face region (ADR-040): its box's centre and
/// size in 0–1 units of the picture.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceRegion {
    pub name: String,
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
}

impl FaceRegion {
    /// From a box's top-left corner and size (0–1).
    pub fn from_box(name: &str, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            name: name.to_owned(),
            cx: x + w / 2.0,
            cy: y + h / 2.0,
            w,
            h,
        }
    }

    /// The box's top-left corner and size (0–1), kept in the picture.
    pub fn to_box(&self) -> (f32, f32, f32, f32) {
        let w = self.w.clamp(f32::MIN_POSITIVE, 1.0);
        let h = self.h.clamp(f32::MIN_POSITIVE, 1.0);
        let x = (self.cx - self.w / 2.0).clamp(0.0, 1.0 - w);
        let y = (self.cy - self.h / 2.0).clamp(0.0, 1.0 - h);
        (x, y, w, h)
    }
}

/// "This face is not that person", by the face's box (ADR-040).
#[derive(Debug, Clone, PartialEq)]
pub struct FaceRejection {
    pub person: String,
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
}

/// Everything Keepsake reads from or writes to an asset's sidecar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AssetXmp {
    pub asset_id: Option<AssetId>,
    pub media_kind: Option<MediaKind>,
    pub added_ms: Option<i64>,
    pub metadata_ms: Option<i64>,
    pub rating: Option<Rating>,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub user_tags: Vec<TagPath>,
    pub ai_tags: Vec<(TagPath, f64)>,
    /// AI tags the user removed (never given again), sorted.
    pub ai_tags_rejected: Vec<TagPath>,
    /// The user's decision in a group of duplicates.
    pub duplicate_choice: Option<DuplicateChoice>,
    /// Which AI gave which keyword (Tag with AI), as (provider id,
    /// keyword), sorted (ADR-039).
    pub ai_tag_sources: Vec<(String, TagPath)>,
    /// Named faces (ADR-040), as `mwg-rs:Regions` face regions. Read: every
    /// named face region in the file (Lightroom's and digiKam's too).
    pub face_regions: Vec<FaceRegion>,
    /// The picture the regions were found in (`mwg-rs:AppliedToDimensions`).
    pub face_size: Option<(u32, u32)>,
    /// Faces said not to be a person.
    pub face_rejections: Vec<FaceRejection>,
    /// Written only: the names whose face regions Keepsake manages (any
    /// case). A named face region in the file with another name, and every
    /// region that is not a named face, is kept as it is.
    pub face_names: Vec<String>,
    pub capture: Option<CaptureTime>,
    pub capture_shift_s: i64,
    pub duration_ms: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub event: Option<Event>,
    pub recipes: Vec<RecipeRef>,
    pub files: Vec<FileRef>,
    pub capture_source: Option<CaptureSource>,
    pub capture_confidence: Option<Confidence>,
    pub gps: Option<GpsPoint>,
    pub sequence: Option<SequenceRef>,
}

const DIGIKAM_COLORS: &[(i64, &str)] = &[
    (1, "Red"),
    (2, "Orange"),
    (3, "Yellow"),
    (4, "Green"),
    (5, "Blue"),
    (6, "Purple"),
    (7, "Gray"),
    (8, "Black"),
    (9, "White"),
];

fn digikam_code(label: &str) -> Option<i64> {
    let l = label.to_lowercase();
    if l == "magenta" {
        return Some(6);
    }
    DIGIKAM_COLORS
        .iter()
        .find(|(_, n)| n.to_lowercase() == l)
        .map(|(c, _)| *c)
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// Format an instant as an XMP date. With a known offset the local time and
/// offset are written; otherwise the UTC clock time without a zone designator.
pub fn format_date(t: CaptureTime) -> String {
    let utc = Utc
        .timestamp_millis_opt(t.utc_ms)
        .single()
        .unwrap_or_default();
    match t
        .utc_offset_min
        .and_then(|m| FixedOffset::east_opt(i32::from(m) * 60))
    {
        Some(off) => utc
            .with_timezone(&off)
            .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
            .to_string(),
        None => utc.naive_utc().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
    }
}

/// Format an instant as a UTC XMP date (`…Z`).
pub fn format_utc(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Parse an XMP/ISO 8601 date. A value without a zone designator is taken as
/// UTC with an unknown offset.
pub fn parse_date(s: &str) -> Option<CaptureTime> {
    let s = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        let off = if s.ends_with('Z') || s.ends_with('z') {
            Some(0)
        } else {
            Some((dt.offset().local_minus_utc() / 60) as i16)
        };
        return Some(CaptureTime {
            utc_ms: dt.timestamp_millis(),
            utc_offset_min: off,
        });
    }
    for fmt in ["%Y-%m-%dT%H:%M%:z", "%Y-%m-%dT%H:%M%#z"] {
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Some(CaptureTime {
                utc_ms: dt.timestamp_millis(),
                utc_offset_min: Some((dt.offset().local_minus_utc() / 60) as i16),
            });
        }
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(CaptureTime {
                utc_ms: n.and_utc().timestamp_millis(),
                utc_offset_min: None,
            });
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0).map(|n| CaptureTime {
            utc_ms: n.and_utc().timestamp_millis(),
            utc_offset_min: None,
        });
    }
    None
}

fn parse_ms(s: Option<String>) -> Option<i64> {
    s.and_then(|v| parse_date(&v)).map(|t| t.utc_ms)
}

/// `"1/1000"` → `(1, 1000)`.
fn parse_rational(s: &str) -> Option<(i64, i64)> {
    let (n, d) = s.trim().split_once('/')?;
    let (n, d) = (n.trim().parse().ok()?, d.trim().parse().ok()?);
    (d != 0).then_some((n, d))
}

fn parse_uuid(s: Option<&str>) -> Option<uuid::Uuid> {
    s.and_then(|v| uuid::Uuid::parse_str(v.trim()).ok())
}

fn read_event(f: &[ReadField]) -> Option<Event> {
    let id = EventId(parse_uuid(field(f, ns::KEEPSAKE, "ID"))?);
    Some(Event {
        id,
        title: field(f, ns::KEEPSAKE, "Title")
            .unwrap_or_default()
            .to_owned(),
        place_name: non_empty(field(f, ns::KEEPSAKE, "Place").map(str::to_owned)),
        lat: field(f, ns::KEEPSAKE, "Latitude").and_then(|v| v.parse().ok()),
        lon: field(f, ns::KEEPSAKE, "Longitude").and_then(|v| v.parse().ok()),
        created_ms: parse_ms(field(f, ns::KEEPSAKE, "Created").map(str::to_owned)).unwrap_or(0),
    })
}

fn read_recipe(f: &[ReadField]) -> Option<RecipeRef> {
    Some(RecipeRef {
        id: RecipeId(parse_uuid(field(f, ns::KEEPSAKE, "ID"))?),
        kind: field(f, ns::KEEPSAKE, "Kind")?.to_owned(),
        schema_version: field(f, ns::KEEPSAKE, "SchemaVersion")?.parse().ok()?,
        rel_path: RelPath::parse(field(f, ns::KEEPSAKE, "Path")?).ok()?,
        content_hash: Blake3Hash::from_hex(field(f, ns::KEEPSAKE, "Hash")?)?,
        created_ms: parse_ms(field(f, ns::KEEPSAKE, "Created").map(str::to_owned))?,
        updated_ms: parse_ms(field(f, ns::KEEPSAKE, "Updated").map(str::to_owned))?,
    })
}

/// A face region's name, if it is a named face with an area.
fn read_face_region(item: &Value) -> Option<FaceRegion> {
    if item.field(ns::MWG_RS, "Type")?.as_text()?.trim() != "Face" {
        return None;
    }
    let name = item.field(ns::MWG_RS, "Name")?.as_text()?.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let area = item.field(ns::MWG_RS, "Area")?;
    if let Some(u) = area.field(ns::ST_AREA, "unit").and_then(Value::as_text) {
        if u.trim() != "normalized" {
            return None;
        }
    }
    let n = |k: &str| {
        area.field(ns::ST_AREA, k)?
            .as_text()?
            .trim()
            .parse::<f32>()
            .ok()
    };
    let (cx, cy, w, h) = (n("x")?, n("y")?, n("w")?, n("h")?);
    let ok = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
    (ok(cx) && ok(cy) && ok(w) && ok(h) && w > 0.0 && h > 0.0).then_some(FaceRegion {
        name,
        cx,
        cy,
        w,
        h,
    })
}

fn face_region_value(f: &FaceRegion) -> Value {
    Value::fields([
        (ns::MWG_RS, "Type", Value::text("Face")),
        (ns::MWG_RS, "Name", Value::text(&f.name)),
        (
            ns::MWG_RS,
            "Area",
            Value::fields([
                (ns::ST_AREA, "x", Value::text(f.cx.to_string())),
                (ns::ST_AREA, "y", Value::text(f.cy.to_string())),
                (ns::ST_AREA, "w", Value::text(f.w.to_string())),
                (ns::ST_AREA, "h", Value::text(f.h.to_string())),
                (ns::ST_AREA, "unit", Value::text("normalized")),
            ]),
        ),
    ])
}

impl AssetXmp {
    /// Write the face regions Keepsake manages, keeping the file's others,
    /// and the rejections.
    fn write_faces(&self, p: &mut XmpPacket) {
        let existing = p.value(ns::MWG_RS, "Regions");
        // Keepsake's regions: the names it manages now, those it writes,
        // and those it wrote last time (`keepsake:FaceNames`).
        let before = p.array(ns::KEEPSAKE, "FaceNames");
        let ours: Vec<String> = self
            .face_names
            .iter()
            .chain(self.face_regions.iter().map(|f| &f.name))
            .chain(before.iter())
            .map(|n| n.trim().to_lowercase())
            .collect();
        let mut written: Vec<String> = self.face_regions.iter().map(|f| f.name.clone()).collect();
        written.sort();
        written.dedup();
        p.set_array(ns::KEEPSAKE, "FaceNames", ArrayKind::Bag, &written);
        let others: Vec<Value> = existing
            .as_ref()
            .and_then(|r| r.field(ns::MWG_RS, "RegionList"))
            .map(|l| l.items().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|item| {
                read_face_region(item).is_none_or(|f| !ours.contains(&f.name.to_lowercase()))
            })
            .collect();
        let mut list: Vec<Value> = self.face_regions.iter().map(face_region_value).collect();
        list.extend(others);
        if list.is_empty() {
            p.set_value(ns::MWG_RS, "Regions", None);
        } else {
            let dims = self.face_size.map(|(w, h)| {
                Value::fields([
                    (ns::ST_DIM, "w", Value::text(w.to_string())),
                    (ns::ST_DIM, "h", Value::text(h.to_string())),
                    (ns::ST_DIM, "unit", Value::text("pixel")),
                ])
            });
            let dims = dims.or_else(|| {
                existing
                    .as_ref()
                    .and_then(|r| r.field(ns::MWG_RS, "AppliedToDimensions"))
                    .cloned()
            });
            let mut fields = Vec::new();
            if let Some(d) = dims {
                fields.push((ns::MWG_RS.to_owned(), "AppliedToDimensions".to_owned(), d));
            }
            fields.push((
                ns::MWG_RS.to_owned(),
                "RegionList".to_owned(),
                Value::Array(ArrayKind::Bag, list),
            ));
            p.set_value(ns::MWG_RS, "Regions", Some(&Value::Struct(fields)));
        }
        let rej: Vec<Vec<Field>> = self
            .face_rejections
            .iter()
            .map(|r| {
                vec![
                    (ns::KEEPSAKE, "Person", r.person.clone()),
                    (ns::KEEPSAKE, "X", r.cx.to_string()),
                    (ns::KEEPSAKE, "Y", r.cy.to_string()),
                    (ns::KEEPSAKE, "W", r.w.to_string()),
                    (ns::KEEPSAKE, "H", r.h.to_string()),
                ]
            })
            .collect();
        p.set_struct_array(ns::KEEPSAKE, "FaceRejections", ArrayKind::Bag, &rej);
    }
    /// Extract Keepsake's view of a packet. Unknown or malformed values are
    /// ignored rather than failing the whole read.
    pub fn read(p: &XmpPacket) -> Self {
        let mut x = AssetXmp {
            asset_id: parse_uuid(p.simple(ns::KEEPSAKE, "AssetID").as_deref()).map(AssetId),
            media_kind: p
                .simple(ns::KEEPSAKE, "MediaKind")
                .and_then(|v| MediaKind::parse(v.trim())),
            added_ms: parse_ms(p.simple(ns::KEEPSAKE, "Added")),
            metadata_ms: parse_ms(p.simple(ns::XMP, "MetadataDate")),
            rating: p
                .simple(ns::XMP, "Rating")
                .and_then(|v| v.trim().parse::<f64>().ok())
                .map(|v| Rating::from_xmp(v.round() as i64)),
            comment: non_empty(p.lang_alt_default(ns::DC, "description")),
            capture_shift_s: p
                .simple(ns::KEEPSAKE, "CaptureShiftSeconds")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            camera_make: non_empty(p.simple(ns::TIFF, "Make")),
            camera_model: non_empty(p.simple(ns::TIFF, "Model")),
            lens_model: non_empty(p.simple(ns::EXIF_EX, "LensModel")),
            ..Default::default()
        };

        x.label = non_empty(p.simple(ns::XMP, "Label")).or_else(|| {
            let code: i64 = p.simple(ns::DIGIKAM, "ColorLabel")?.trim().parse().ok()?;
            DIGIKAM_COLORS
                .iter()
                .find(|(c, _)| *c == code)
                .map(|(_, n)| (*n).to_owned())
        });

        x.capture = p
            .simple(ns::EXIF, "DateTimeOriginal")
            .or_else(|| p.simple(ns::PHOTOSHOP, "DateCreated"))
            .or_else(|| p.simple(ns::XMP, "CreateDate"))
            .and_then(|v| parse_date(&v));

        x.duration_ms = p.structure(ns::XMP_DM, "duration").and_then(|f| {
            let value: f64 = field(&f, ns::XMP_DM, "value")?.trim().parse().ok()?;
            let (n, d) = field(&f, ns::XMP_DM, "scale")
                .and_then(parse_rational)
                .unwrap_or((1, 1));
            Some((value * n as f64 * 1000.0 / d as f64).round() as i64)
        });

        x.user_tags = read_keywords(p);
        x.ai_tags = p
            .struct_array(ns::KEEPSAKE, "AITags")
            .iter()
            .filter_map(|f| {
                let path = TagPath::parse_lightroom(field(f, ns::KEEPSAKE, "Name")?).ok()?;
                let c: f64 = field(f, ns::KEEPSAKE, "Confidence")?.trim().parse().ok()?;
                (0.0..=1.0).contains(&c).then_some((path, c))
            })
            .collect();
        x.ai_tags.sort_by(|a, b| a.0.cmp(&b.0));
        x.ai_tags.dedup_by(|a, b| a.0 == b.0);
        x.ai_tags_rejected = p
            .array(ns::KEEPSAKE, "AITagsRejected")
            .iter()
            .filter_map(|v| TagPath::parse_lightroom(v).ok())
            .collect();
        x.ai_tags_rejected.sort();
        x.ai_tags_rejected.dedup();
        x.ai_tag_sources = p
            .struct_array(ns::KEEPSAKE, "AITagSources")
            .iter()
            .filter_map(|f| {
                let by = crate::ai::cloud::Provider::parse(field(f, ns::KEEPSAKE, "By")?.trim())?;
                let path = TagPath::parse_lightroom(field(f, ns::KEEPSAKE, "Name")?).ok()?;
                Some((by.id().to_owned(), path))
            })
            .collect();
        x.ai_tag_sources.sort();
        x.ai_tag_sources.dedup();
        if let Some(r) = p.value(ns::MWG_RS, "Regions") {
            x.face_size = r.field(ns::MWG_RS, "AppliedToDimensions").and_then(|d| {
                let n = |k: &str| {
                    d.field(ns::ST_DIM, k)?
                        .as_text()?
                        .trim()
                        .parse::<f64>()
                        .ok()
                };
                Some((n("w")?.round() as u32, n("h")?.round() as u32))
            });
            x.face_regions = r
                .field(ns::MWG_RS, "RegionList")
                .map(|l| l.items())
                .unwrap_or_default()
                .iter()
                .filter_map(read_face_region)
                .collect();
        }
        x.face_rejections = p
            .struct_array(ns::KEEPSAKE, "FaceRejections")
            .iter()
            .filter_map(|f| {
                let n = |k: &str| field(f, ns::KEEPSAKE, k)?.trim().parse::<f32>().ok();
                let person = field(f, ns::KEEPSAKE, "Person")?.trim().to_owned();
                (!person.is_empty()).then_some(())?;
                Some(FaceRejection {
                    person,
                    cx: n("X")?,
                    cy: n("Y")?,
                    w: n("W")?,
                    h: n("H")?,
                })
            })
            .collect();
        x.duplicate_choice = p
            .simple(ns::KEEPSAKE, "DuplicateChoice")
            .and_then(|v| DuplicateChoice::parse(v.trim()));

        x.event = p
            .structure(ns::KEEPSAKE, "Event")
            .and_then(|f| read_event(&f));
        x.recipes = p
            .struct_array(ns::KEEPSAKE, "Recipes")
            .iter()
            .filter_map(|f| read_recipe(f))
            .collect();
        x.files = p
            .struct_array(ns::KEEPSAKE, "Files")
            .iter()
            .filter_map(|f| {
                Some(FileRef {
                    rel_path: RelPath::parse(field(f, ns::KEEPSAKE, "Path")?).ok()?,
                    role: FileRole::parse(field(f, ns::KEEPSAKE, "Role")?.trim())?,
                    segment_index: field(f, ns::KEEPSAKE, "SegmentIndex")
                        .and_then(|v| v.trim().parse().ok()),
                })
            })
            .collect();
        x.capture_source = p
            .simple(ns::KEEPSAKE, "CaptureSource")
            .and_then(|v| CaptureSource::parse(v.trim()));
        x.capture_confidence = p
            .simple(ns::KEEPSAKE, "CaptureConfidence")
            .and_then(|v| Confidence::parse(v.trim()));
        x.gps = read_gps(p);
        x.sequence = p.structure(ns::KEEPSAKE, "Sequence").and_then(|f| {
            Some(SequenceRef {
                id: SequenceId(parse_uuid(field(&f, ns::KEEPSAKE, "ID"))?),
                kind: SequenceKind::parse(field(&f, ns::KEEPSAKE, "Kind")?.trim())?,
                position: field(&f, ns::KEEPSAKE, "Position")?.trim().parse().ok()?,
            })
        });
        x
    }

    /// Write Keepsake's fields into `p`, replacing previous values of the
    /// fields it manages and leaving everything else untouched.
    pub fn write(&self, p: &mut XmpPacket) {
        let set_opt = |p: &mut XmpPacket, uri: &str, local: &str, v: Option<String>| match v {
            Some(v) => p.set_simple(uri, local, &v),
            None => p.remove(uri, local),
        };

        set_opt(
            p,
            ns::KEEPSAKE,
            "AssetID",
            self.asset_id.map(|a| a.to_string()),
        );
        set_opt(
            p,
            ns::KEEPSAKE,
            "MediaKind",
            self.media_kind.map(|m| m.as_str().to_owned()),
        );
        set_opt(p, ns::KEEPSAKE, "Added", self.added_ms.map(format_utc));
        set_opt(p, ns::XMP, "MetadataDate", self.metadata_ms.map(format_utc));
        set_opt(
            p,
            ns::XMP,
            "Rating",
            self.rating.map(|r| r.get().to_string()),
        );
        set_opt(p, ns::XMP, "Label", self.label.clone());
        set_opt(
            p,
            ns::DIGIKAM,
            "ColorLabel",
            self.label
                .as_deref()
                .and_then(digikam_code)
                .map(|c| c.to_string()),
        );
        p.set_lang_alt_default(ns::DC, "description", self.comment.as_deref());

        // Keywords.
        let mut flat: BTreeSet<String> = BTreeSet::new();
        let mut paths = self.user_tags.clone();
        paths.sort();
        paths.dedup();
        for t in &paths {
            flat.extend(t.segments().iter().cloned());
        }
        p.set_array(
            ns::DC,
            "subject",
            ArrayKind::Bag,
            &flat.into_iter().collect::<Vec<_>>(),
        );
        p.set_array(
            ns::LR,
            "hierarchicalSubject",
            ArrayKind::Bag,
            &paths.iter().map(TagPath::to_lightroom).collect::<Vec<_>>(),
        );
        p.set_array(
            ns::DIGIKAM,
            "TagsList",
            ArrayKind::Seq,
            &paths.iter().map(TagPath::to_digikam).collect::<Vec<_>>(),
        );
        let ai: Vec<Vec<Field>> = self
            .ai_tags
            .iter()
            .map(|(t, c)| {
                vec![
                    (ns::KEEPSAKE, "Name", t.to_lightroom()),
                    (ns::KEEPSAKE, "Confidence", c.to_string()),
                ]
            })
            .collect();
        p.set_struct_array(ns::KEEPSAKE, "AITags", ArrayKind::Bag, &ai);
        p.set_array(
            ns::KEEPSAKE,
            "AITagsRejected",
            ArrayKind::Bag,
            &self
                .ai_tags_rejected
                .iter()
                .map(TagPath::to_lightroom)
                .collect::<Vec<_>>(),
        );
        let sources: Vec<Vec<Field>> = self
            .ai_tag_sources
            .iter()
            .map(|(by, t)| {
                vec![
                    (ns::KEEPSAKE, "Name", t.to_lightroom()),
                    (ns::KEEPSAKE, "By", by.clone()),
                ]
            })
            .collect();
        p.set_struct_array(ns::KEEPSAKE, "AITagSources", ArrayKind::Bag, &sources);
        self.write_faces(p);
        set_opt(
            p,
            ns::KEEPSAKE,
            "DuplicateChoice",
            self.duplicate_choice.map(|c| c.as_str().to_owned()),
        );

        // Capture time and camera.
        let date = self.capture.map(format_date);
        set_opt(p, ns::EXIF, "DateTimeOriginal", date.clone());
        set_opt(p, ns::PHOTOSHOP, "DateCreated", date);
        set_opt(
            p,
            ns::KEEPSAKE,
            "CaptureShiftSeconds",
            (self.capture_shift_s != 0).then(|| self.capture_shift_s.to_string()),
        );
        set_opt(p, ns::TIFF, "Make", self.camera_make.clone());
        set_opt(p, ns::TIFF, "Model", self.camera_model.clone());
        set_opt(p, ns::EXIF_EX, "LensModel", self.lens_model.clone());
        let dur: Option<Vec<Field>> = self.duration_ms.map(|ms| {
            vec![
                (ns::XMP_DM, "value", ms.to_string()),
                (ns::XMP_DM, "scale", "1/1000".to_owned()),
            ]
        });
        p.set_struct(ns::XMP_DM, "duration", dur.as_deref());

        // Event.
        let ev: Option<Vec<Field>> = self.event.as_ref().map(|e| {
            let mut f = vec![
                (ns::KEEPSAKE, "ID", e.id.to_string()),
                (ns::KEEPSAKE, "Title", e.title.clone()),
                (ns::KEEPSAKE, "Created", format_utc(e.created_ms)),
            ];
            if let Some(pl) = &e.place_name {
                f.push((ns::KEEPSAKE, "Place", pl.clone()));
            }
            if let Some(v) = e.lat {
                f.push((ns::KEEPSAKE, "Latitude", v.to_string()));
            }
            if let Some(v) = e.lon {
                f.push((ns::KEEPSAKE, "Longitude", v.to_string()));
            }
            f
        });
        p.set_struct(ns::KEEPSAKE, "Event", ev.as_deref());

        // Recipes and member files.
        let recipes: Vec<Vec<Field>> = self
            .recipes
            .iter()
            .map(|r| {
                vec![
                    (ns::KEEPSAKE, "ID", r.id.to_string()),
                    (ns::KEEPSAKE, "Kind", r.kind.clone()),
                    (ns::KEEPSAKE, "SchemaVersion", r.schema_version.to_string()),
                    (ns::KEEPSAKE, "Path", r.rel_path.to_string()),
                    (ns::KEEPSAKE, "Hash", r.content_hash.to_hex()),
                    (ns::KEEPSAKE, "Created", format_utc(r.created_ms)),
                    (ns::KEEPSAKE, "Updated", format_utc(r.updated_ms)),
                ]
            })
            .collect();
        p.set_struct_array(ns::KEEPSAKE, "Recipes", ArrayKind::Bag, &recipes);
        let files: Vec<Vec<Field>> = self
            .files
            .iter()
            .map(|f| {
                let mut v = vec![
                    (ns::KEEPSAKE, "Path", f.rel_path.to_string()),
                    (ns::KEEPSAKE, "Role", f.role.as_str().to_owned()),
                ];
                if let Some(i) = f.segment_index {
                    v.push((ns::KEEPSAKE, "SegmentIndex", i.to_string()));
                }
                v
            })
            .collect();
        p.set_struct_array(ns::KEEPSAKE, "Files", ArrayKind::Bag, &files);

        set_opt(
            p,
            ns::KEEPSAKE,
            "CaptureSource",
            self.capture_source.map(|c| c.as_str().to_owned()),
        );
        set_opt(
            p,
            ns::KEEPSAKE,
            "CaptureConfidence",
            self.capture_confidence.map(|c| c.as_str().to_owned()),
        );
        write_gps(p, self.gps);
        let seq: Option<Vec<Field>> = self.sequence.map(|s| {
            vec![
                (ns::KEEPSAKE, "ID", s.id.to_string()),
                (ns::KEEPSAKE, "Kind", s.kind.as_str().to_owned()),
                (ns::KEEPSAKE, "Position", s.position.to_string()),
            ]
        });
        p.set_struct(ns::KEEPSAKE, "Sequence", seq.as_deref());
    }
}

/// `exif:GPSLatitude` form: `DDD,MM.mmmmmmK`.
fn format_coord(v: f64, pos: char, neg: char) -> String {
    let hemi = if v < 0.0 { neg } else { pos };
    let a = v.abs();
    let deg = a.trunc();
    let min = (a - deg) * 60.0;
    format!("{},{:.6}{}", deg as i64, min, hemi)
}

/// Parse `DDD,MM.mmK`, `DDD,MM,SSK` or a plain decimal.
fn parse_coord(s: &str) -> Option<f64> {
    let s = s.trim();
    let (body, sign) = match s.chars().last()? {
        'N' | 'E' | 'n' | 'e' => (&s[..s.len() - 1], 1.0),
        'S' | 'W' | 's' | 'w' => (&s[..s.len() - 1], -1.0),
        _ => (s, 1.0),
    };
    let parts: Vec<f64> = body
        .split(',')
        .map(|p| p.trim().parse::<f64>().ok())
        .collect::<Option<_>>()?;
    let v = match parts.as_slice() {
        [d] => *d,
        [d, m] => d + m / 60.0,
        [d, m, sec] => d + m / 60.0 + sec / 3600.0,
        _ => return None,
    };
    Some(sign * v)
}

fn read_gps(p: &XmpPacket) -> Option<GpsPoint> {
    // Exact values written by Keepsake first, then the standard EXIF form.
    let exact = (|| {
        Some(GpsPoint {
            lat: p.simple(ns::KEEPSAKE, "Latitude")?.trim().parse().ok()?,
            lon: p.simple(ns::KEEPSAKE, "Longitude")?.trim().parse().ok()?,
            alt_m: p
                .simple(ns::KEEPSAKE, "Altitude")
                .and_then(|v| v.trim().parse().ok()),
        })
    })();
    let g = exact.or_else(|| {
        let lat = parse_coord(&p.simple(ns::EXIF, "GPSLatitude")?)?;
        let lon = parse_coord(&p.simple(ns::EXIF, "GPSLongitude")?)?;
        let alt = p.simple(ns::EXIF, "GPSAltitude").and_then(|v| {
            let v = v.trim();
            let a = match v.split_once('/') {
                Some((n, d)) => n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok()?,
                None => v.parse().ok()?,
            };
            let below = p
                .simple(ns::EXIF, "GPSAltitudeRef")
                .is_some_and(|r| r.trim() == "1");
            Some(if below { -a } else { a })
        });
        Some(GpsPoint {
            lat,
            lon,
            alt_m: alt,
        })
    })?;
    ((-90.0..=90.0).contains(&g.lat) && (-180.0..=180.0).contains(&g.lon)).then_some(g)
}

fn write_gps(p: &mut XmpPacket, g: Option<GpsPoint>) {
    for (uri, local) in [
        (ns::EXIF, "GPSLatitude"),
        (ns::EXIF, "GPSLongitude"),
        (ns::EXIF, "GPSAltitude"),
        (ns::EXIF, "GPSAltitudeRef"),
        (ns::KEEPSAKE, "Latitude"),
        (ns::KEEPSAKE, "Longitude"),
        (ns::KEEPSAKE, "Altitude"),
    ] {
        p.remove(uri, local);
    }
    let Some(g) = g else { return };
    p.set_simple(ns::EXIF, "GPSLatitude", &format_coord(g.lat, 'N', 'S'));
    p.set_simple(ns::EXIF, "GPSLongitude", &format_coord(g.lon, 'E', 'W'));
    p.set_simple(ns::KEEPSAKE, "Latitude", &g.lat.to_string());
    p.set_simple(ns::KEEPSAKE, "Longitude", &g.lon.to_string());
    if let Some(a) = g.alt_m {
        p.set_simple(
            ns::EXIF,
            "GPSAltitude",
            &format!("{}/1000", (a.abs() * 1000.0).round() as i64),
        );
        p.set_simple(ns::EXIF, "GPSAltitudeRef", if a < 0.0 { "1" } else { "0" });
        p.set_simple(ns::KEEPSAKE, "Altitude", &a.to_string());
    }
}

/// Keywords: hierarchical lists first (Lightroom, then digiKam), then any flat
/// `dc:subject` keyword that is not already part of a hierarchy.
fn read_keywords(p: &XmpPacket) -> Vec<TagPath> {
    let mut paths: Vec<TagPath> = p
        .array(ns::LR, "hierarchicalSubject")
        .iter()
        .filter_map(|s| TagPath::parse_lightroom(s).ok())
        .collect();
    if paths.is_empty() {
        paths = p
            .array(ns::DIGIKAM, "TagsList")
            .iter()
            .filter_map(|s| TagPath::parse_digikam(s).ok())
            .collect();
    }
    let known: BTreeSet<String> = paths
        .iter()
        .flat_map(|t| t.segments().iter())
        .map(|s| path_key(s, CaseSensitivity::Insensitive))
        .collect();
    for kw in p.array(ns::DC, "subject") {
        if !known.contains(&path_key(kw.trim(), CaseSensitivity::Insensitive)) {
            if let Ok(t) = TagPath::new([kw.as_str()]) {
                paths.push(t);
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_round_trips() {
        for t in [
            CaptureTime {
                utc_ms: 1_718_425_800_123,
                utc_offset_min: Some(540),
            },
            CaptureTime {
                utc_ms: 1_718_425_800_000,
                utc_offset_min: Some(-300),
            },
            CaptureTime {
                utc_ms: 1_718_425_800_000,
                utc_offset_min: Some(0),
            },
            CaptureTime {
                utc_ms: 1_718_425_800_000,
                utc_offset_min: None,
            },
        ] {
            assert_eq!(parse_date(&format_date(t)), Some(t), "{}", format_date(t));
        }
        assert_eq!(
            parse_date("2024-06-15T14:30+09:00").map(|t| t.utc_offset_min),
            Some(Some(540))
        );
        assert!(parse_date("2024-06-15").is_some());
        assert!(parse_date("garbage").is_none());
    }

    #[test]
    fn digikam_codes() {
        assert_eq!(digikam_code("Red"), Some(1));
        assert_eq!(digikam_code("purple"), Some(6));
        assert_eq!(digikam_code("To Do"), None);
    }
}
