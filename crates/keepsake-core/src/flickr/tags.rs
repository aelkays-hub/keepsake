//! The tags a photo is uploaded to Flickr with (ADR-046): its keywords
//! flattened, and its camera and lens.
//!
//! - A keyword gives its last part, as Lightroom exports it:
//!   `Aircraft|Registration|C-FEJC` → `C-FEJC`.
//! - With machine tags on, a keyword of three parts or more also gives a
//!   Flickr machine tag: `aircraft:registration=C-FEJC`.
//! - With parent keywords on, the parts above the last are tags too
//!   (`Aircraft`, `Registration`).
//! - The camera gives its make (`Sony`), make and model
//!   (`Sony ILCE-7RM5`) and lens (`FE 200-600mm F5.6-6.3 G OSS`).
//!
//! Only the user's own keywords are used (typed, imported, accepted or
//! from Tag with AI), not the on-device AI's suggestions. Tags are kept
//! once (ignoring case), at most [`MAX_TAGS`], without `"` (Flickr cannot
//! quote it).

use serde::{Deserialize, Serialize};

use crate::catalog::{Asset, AssetTag, TagPath, TagSource};

/// Flickr takes at most 75 tags per photo.
pub const MAX_TAGS: usize = 75;

/// How keywords become tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagOptions {
    /// The camera's make, model and lens.
    pub camera: bool,
    /// Machine tags for keywords of three parts or more.
    pub machine: bool,
    /// The keywords' parents too.
    pub parents: bool,
}

impl Default for TagOptions {
    fn default() -> Self {
        Self {
            camera: true,
            machine: false,
            parents: false,
        }
    }
}

/// A tag as Flickr can take it: trimmed, inner spaces collapsed, no `"`.
pub fn clean(tag: &str) -> String {
    tag.replace('"', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A machine tag's namespace or predicate: lower-case letters, digits and
/// `_`, starting with a letter (`From country` → `from_country`).
fn machine_part(s: &str) -> Option<String> {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if (c.is_whitespace() || c == '-' || c == '_') && !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_owned();
    out.chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase())
        .then_some(out)
}

/// The machine tag of a keyword of three parts or more:
/// `Aircraft|Registration|C-FEJC` → `aircraft:registration=C-FEJC`.
pub fn machine_tag(path: &TagPath) -> Option<String> {
    let s = path.segments();
    if s.len() < 3 {
        return None;
    }
    let ns = machine_part(&s[0])?;
    let pred = machine_part(&s[1])?;
    let value = clean(&s[2..].join(" "));
    (!value.is_empty()).then(|| format!("{ns}:{pred}={value}"))
}

/// The tags one keyword gives.
pub fn of_keyword(path: &TagPath, opts: TagOptions) -> Vec<String> {
    let mut out = Vec::new();
    if opts.parents {
        let s = path.segments();
        out.extend(s[..s.len().saturating_sub(1)].iter().map(|p| clean(p)));
    }
    out.push(clean(path.leaf()));
    if opts.machine {
        out.extend(machine_tag(path));
    }
    out
}

/// A maker as people write it: `SONY` → `Sony`, `NIKON CORPORATION` →
/// `Nikon`, `OLYMPUS IMAGING CORP.` → `Olympus`; `DJI` and `Canon` stay.
pub fn tidy_make(make: &str) -> String {
    let mut m = clean(make);
    for suffix in [
        " IMAGING CORP.",
        " IMAGING CORP",
        " CORPORATION",
        " CORP.",
        " CORP",
        " Co., Ltd.",
        " CO., LTD.",
        " Co.,Ltd.",
        " COMPUTER CO.,LTD.",
    ] {
        if m.len() > suffix.len()
            && m.to_ascii_uppercase()
                .ends_with(&suffix.to_ascii_uppercase())
        {
            m.truncate(m.len() - suffix.len());
        }
    }
    let m = m.trim().to_owned();
    let all_caps =
        m.chars().any(|c| c.is_ascii_alphabetic()) && !m.chars().any(|c| c.is_ascii_lowercase());
    if all_caps && m.chars().filter(char::is_ascii_alphabetic).count() > 3 {
        m.split(' ')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_string() + &c.as_str().to_ascii_lowercase(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        m
    }
}

/// A lens name worth a tag (cameras write `----` or `0.0 mm f/0.0` for
/// none).
fn real_lens(lens: &str) -> bool {
    let l = lens.trim();
    l.chars().any(char::is_alphanumeric) && !l.starts_with("0.0 mm") && !l.starts_with("----")
}

/// The camera's tags: make, make and model, lens.
pub fn camera_tags(make: Option<&str>, model: Option<&str>, lens: Option<&str>) -> Vec<String> {
    let make = make.map(tidy_make).filter(|m| !m.is_empty());
    let model = model.map(clean).filter(|m| !m.is_empty());
    let mut out = Vec::new();
    if let Some(m) = &make {
        out.push(m.clone());
    }
    match (&make, &model) {
        (Some(mk), Some(md)) if md.to_lowercase().starts_with(&mk.to_lowercase()) => {
            out.push(md.clone())
        }
        (Some(mk), Some(md)) => out.push(format!("{mk} {md}")),
        (None, Some(md)) => out.push(md.clone()),
        _ => {}
    }
    if let Some(l) = lens.filter(|l| real_lens(l)) {
        out.push(clean(l));
    }
    out
}

/// Keep each tag once (ignoring case), without empty ones, at most
/// [`MAX_TAGS`].
pub fn dedupe(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in tags {
        let t = clean(&t);
        if t.is_empty() || !seen.insert(t.to_lowercase()) {
            continue;
        }
        out.push(t);
        if out.len() == MAX_TAGS {
            break;
        }
    }
    out
}

/// The tags suggested for a photo: its keywords, then its camera.
pub fn suggested(asset: &Asset, keywords: &[AssetTag], opts: TagOptions) -> Vec<String> {
    let mut all: Vec<String> = keywords
        .iter()
        .filter(|t| t.source == TagSource::User)
        .flat_map(|t| of_keyword(&t.path, opts))
        .collect();
    if opts.camera {
        all.extend(camera_tags(
            asset.camera_make.as_deref(),
            asset.camera_model.as_deref(),
            asset.lens_model.as_deref(),
        ));
    }
    dedupe(all)
}

/// Tags as Flickr's `tags` argument takes them: separated by spaces, one
/// with a space in quotes (`C-FEJC "Air Canada"`).
pub fn to_argument(tags: &[String]) -> String {
    tags.iter()
        .map(|t| clean(t))
        .filter(|t| !t.is_empty())
        .map(|t| {
            if t.contains(' ') {
                format!("\"{t}\"")
            } else {
                t
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AssetId, MediaKind};

    fn p(s: &str) -> TagPath {
        TagPath::parse_lightroom(s).unwrap()
    }

    #[test]
    fn a_keyword_gives_its_last_part() {
        let o = TagOptions::default();
        assert_eq!(
            of_keyword(&p("Aircraft|Registration|C-FEJC"), o),
            ["C-FEJC"]
        );
        assert_eq!(of_keyword(&p("Dog"), o), ["Dog"]);
        assert_eq!(
            of_keyword(&p("Aircraft|Airline|Air  Canada"), o),
            ["Air Canada"]
        );
    }

    #[test]
    fn machine_tags_and_parents_are_options() {
        let o = TagOptions {
            camera: false,
            machine: true,
            parents: true,
        };
        assert_eq!(
            of_keyword(&p("Aircraft|Registration|C-FEJC"), o),
            [
                "Aircraft",
                "Registration",
                "C-FEJC",
                "aircraft:registration=C-FEJC"
            ]
        );
        assert_eq!(
            machine_tag(&p("Aircraft|From country|Canada")).as_deref(),
            Some("aircraft:from_country=Canada")
        );
        assert_eq!(machine_tag(&p("People|Alice")), None, "two parts: none");
        assert_eq!(machine_tag(&p("123|x|y")), None, "must start with a letter");
    }

    #[test]
    fn the_camera_gives_make_model_and_lens() {
        assert_eq!(
            camera_tags(
                Some("SONY"),
                Some("ILCE-7RM5"),
                Some("FE 200-600mm F5.6-6.3 G OSS")
            ),
            ["Sony", "Sony ILCE-7RM5", "FE 200-600mm F5.6-6.3 G OSS"]
        );
        assert_eq!(
            camera_tags(Some("Canon"), Some("Canon EOS R5"), Some("----")),
            ["Canon", "Canon EOS R5"]
        );
        assert_eq!(
            camera_tags(Some("NIKON CORPORATION"), Some("NIKON Z 8"), None),
            ["Nikon", "NIKON Z 8"]
        );
        assert_eq!(tidy_make("OLYMPUS IMAGING CORP."), "Olympus");
        assert_eq!(tidy_make("DJI"), "DJI");
        assert_eq!(tidy_make("FUJIFILM"), "Fujifilm");
        assert_eq!(
            camera_tags(None, None, Some("0.0 mm f/0.0")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn tags_are_kept_once_and_quoted_with_spaces() {
        let t = dedupe(["C-FEJC", "c-fejc", "", "Air \"Canada\"", "Air Canada"].map(String::from));
        assert_eq!(t, ["C-FEJC", "Air Canada"]);
        assert_eq!(to_argument(&t), "C-FEJC \"Air Canada\"");
        let many = dedupe((0..100).map(|i| format!("t{i}")));
        assert_eq!(many.len(), MAX_TAGS);
    }

    #[test]
    fn suggested_tags_are_the_users_keywords_then_the_camera() {
        let mut a = Asset::new(AssetId::new_random(), MediaKind::Photo, 0);
        a.camera_make = Some("SONY".into());
        a.camera_model = Some("ILCE-1".into());
        let kw = vec![
            AssetTag::user(p("Aircraft|Registration|C-FEJC")),
            AssetTag {
                path: p("Sky"),
                source: TagSource::Ai,
                confidence: Some(0.9),
            },
        ];
        assert_eq!(
            suggested(&a, &kw, TagOptions::default()),
            ["C-FEJC", "Sony", "Sony ILCE-1"]
        );
        let none = TagOptions {
            camera: false,
            ..TagOptions::default()
        };
        assert_eq!(suggested(&a, &kw, none), ["C-FEJC"]);
    }
}
