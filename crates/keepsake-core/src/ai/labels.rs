//! The built-in tag list and scoring an image against it.
//!
//! `labels.tsv` names each tag, the phrases that describe it and, in an
//! optional third column, a tag it also gives (Baby gives People). The
//! phrases' text vectors (`labels-siglip2-b16-224.bin`) are made once by
//! `scripts/make-label-vectors.py` with the model's text half, so the app
//! needs neither the text model nor a tokenizer. The vector file carries a
//! copy of the list it was made from, and loading checks the two agree.
//!
//! A tag scores its best-matching phrase: "a woman posing for a photo
//! shoot" finds a studio portrait that "a person" misses, and averaging
//! the phrases blurred them. SigLIP scores each tag on its own:
//! `p = sigmoid(scale · cos + bias)`, with the model's trained
//! `logit_scale` and `logit_bias`.

use super::embedding::{Embedding, DIM};
use super::AiError;
use crate::catalog::TagPath;

const TSV: &str = include_str!("labels.tsv");
const VECTORS: &[u8] = include_bytes!("labels-siglip2-b16-224.bin");
const MAGIC: &[u8; 8] = b"KSLV2\0\0\0";

/// The version of the tag list. Photos tagged with an older list are
/// tagged again (from their stored analysis) when Tag photos is next run
/// on their dates. 1: one averaged vector per tag; 2: best phrase, more
/// phrases for people, implied tags.
pub const LIST_VERSION: u32 = 2;

/// `exp(logit_scale)` of google/siglip2-base-patch16-224.
pub const LOGIT_SCALE: f32 = 112.668_9;
/// `logit_bias` of google/siglip2-base-patch16-224.
pub const LOGIT_BIAS: f32 = -16.771_725;

/// A tag is given when its probability is at least this. SigLIP's
/// probabilities are low: on a sample of the product owner's library the
/// best tag usually scored 0.02 to 0.85 and the rest below 0.01.
pub const MIN_PROB: f32 = 0.02;
/// At most this many tags per item, most likely first (implied ones
/// included).
pub const MAX_TAGS: usize = 6;

/// One tag and its phrases' text vectors.
#[derive(Debug, Clone)]
pub struct Label {
    pub name: String,
    /// One unit vector per phrase, in the list's order (never empty).
    pub vectors: Vec<Embedding>,
    /// A tag this one also gives (Baby → People).
    pub implies: Option<String>,
}

impl Label {
    /// Its first phrase's vector.
    pub fn vector(&self) -> &Embedding {
        &self.vectors[0]
    }

    /// Its best phrase's cosine with an image.
    pub fn cosine(&self, image: &Embedding) -> f32 {
        self.vectors
            .iter()
            .map(|v| image.cosine(v))
            .fold(f32::NEG_INFINITY, f32::max)
    }
}

/// The tag list.
#[derive(Debug, Clone)]
pub struct LabelSet {
    labels: Vec<Label>,
}

/// One line of `labels.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub phrases: usize,
    pub implies: Option<String>,
}

/// The entries of a `labels.tsv` text, in order.
pub fn parse(tsv: &str) -> Result<Vec<Entry>, AiError> {
    let mut out: Vec<Entry> = Vec::new();
    for (i, line) in tsv.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = |why: &str| AiError::Labels(format!("line {}: {why}", i + 1));
        let mut cols = line.split('\t');
        let name = cols.next().unwrap_or_default().trim();
        let phrases = cols
            .next()
            .ok_or_else(|| bad("no tab"))?
            .split('|')
            .filter(|p| !p.trim().is_empty())
            .count();
        let implies = cols
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if cols.next().is_some() {
            return Err(bad("more than three columns"));
        }
        if name.is_empty() || phrases == 0 {
            return Err(bad("empty name or phrases"));
        }
        out.push(Entry {
            name: name.to_owned(),
            phrases,
            implies,
        });
    }
    for e in &out {
        if let Some(t) = &e.implies {
            if !out.iter().any(|o| &o.name == t) {
                return Err(AiError::Labels(format!(
                    "{}: implies unknown tag {t}",
                    e.name
                )));
            }
        }
    }
    Ok(out)
}

/// Tag names of a `labels.tsv` text, in order.
pub fn parse_names(tsv: &str) -> Result<Vec<String>, AiError> {
    Ok(parse(tsv)?.into_iter().map(|e| e.name).collect())
}

fn take<'a>(b: &mut &'a [u8], n: usize) -> Result<&'a [u8], AiError> {
    if b.len() < n {
        return Err(AiError::Labels("vector file is truncated".into()));
    }
    let (head, rest) = b.split_at(n);
    *b = rest;
    Ok(head)
}

fn u32_le(b: &mut &[u8]) -> Result<u32, AiError> {
    let s = take(b, 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

impl LabelSet {
    /// The list built into Keepsake.
    pub fn builtin() -> Result<Self, AiError> {
        Self::from_parts(TSV, VECTORS)
    }

    /// Parse a vector file and check it against `tsv`.
    pub fn from_parts(tsv: &str, vectors: &[u8]) -> Result<Self, AiError> {
        let mut b = vectors;
        if take(&mut b, 8)? != MAGIC {
            return Err(AiError::Labels("not a label vector file".into()));
        }
        let n = u32_le(&mut b)? as usize;
        let made_from = std::str::from_utf8(take(&mut b, n)?)
            .map_err(|_| AiError::Labels("vector file's label list is not UTF-8".into()))?;
        if made_from != tsv.replace('\r', "") {
            return Err(AiError::Labels(
                "labels.tsv changed since the vectors were made: run scripts/make-label-vectors.py"
                    .into(),
            ));
        }
        let entries = parse(tsv)?;
        let count = u32_le(&mut b)? as usize;
        let dim = u32_le(&mut b)? as usize;
        if count != entries.len() || dim != DIM {
            return Err(AiError::Labels(format!(
                "{count} tags of {dim} for {} labels",
                entries.len()
            )));
        }
        let mut labels = Vec::with_capacity(count);
        for e in entries {
            if u32_le(&mut b)? as usize != e.phrases {
                return Err(AiError::Labels(format!("{}: wrong phrase count", e.name)));
            }
            let mut vectors = Vec::with_capacity(e.phrases);
            for _ in 0..e.phrases {
                let s = take(&mut b, 4)?;
                let scale = f32::from_le_bytes([s[0], s[1], s[2], s[3]]);
                let q: Vec<i8> = take(&mut b, dim)?
                    .iter()
                    .map(|&x| i8::from_le_bytes([x]))
                    .collect();
                vectors.push(Embedding::from_quantized(&q, scale)?);
            }
            labels.push(Label {
                name: e.name,
                vectors,
                implies: e.implies,
            });
        }
        if !b.is_empty() {
            return Err(AiError::Labels("trailing bytes in vector file".into()));
        }
        Ok(Self { labels })
    }

    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    /// Every tag's probability for an image, most likely first.
    pub fn probabilities(&self, image: &Embedding) -> Vec<(&str, f32)> {
        // Ranked by cosine: probabilities round to 1.0 near the top.
        let mut out: Vec<(&str, f32)> = self
            .labels
            .iter()
            .map(|l| (l.name.as_str(), l.cosine(image)))
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        out.into_iter().map(|(n, c)| (n, probability(c))).collect()
    }

    /// The tags to give an image: at least [`MIN_PROB`], each followed by
    /// the tag it implies (at the same probability, unless that scored
    /// higher itself), at most [`MAX_TAGS`], leaving out `rejected` ones.
    pub fn tags_for(&self, image: &Embedding, rejected: &[TagPath]) -> Vec<(TagPath, f64)> {
        let mut out: Vec<(String, f32)> = Vec::new();
        for (name, p) in self.probabilities(image) {
            if p < MIN_PROB {
                break;
            }
            let implied = self
                .labels
                .iter()
                .find(|l| l.name == name)
                .and_then(|l| l.implies.clone());
            for n in std::iter::once(name.to_owned()).chain(implied) {
                if !out.iter().any(|(o, _)| *o == n) {
                    out.push((n, p));
                }
            }
        }
        out.into_iter()
            .filter_map(|(name, p)| TagPath::new([name]).ok().map(|t| (t, f64::from(p))))
            .filter(|(t, _)| !rejected.contains(t))
            .take(MAX_TAGS)
            .collect()
    }
}

/// SigLIP's probability for a cosine similarity.
pub fn probability(cos: f32) -> f32 {
    1.0 / (1.0 + (-(LOGIT_SCALE * cos + LOGIT_BIAS)).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_list_matches_its_vectors() {
        let set = LabelSet::builtin().unwrap();
        assert_eq!(set.labels().len(), parse_names(TSV).unwrap().len());
        assert!(set.labels().len() >= 100);
        for l in set.labels() {
            assert!(!l.vectors.is_empty());
            assert!(l.vectors.iter().all(|v| v.len() == DIM));
        }
        // Different tags have different vectors.
        let dog = &set
            .labels()
            .iter()
            .find(|l| l.name == "Dog")
            .unwrap()
            .vector();
        let cat = &set
            .labels()
            .iter()
            .find(|l| l.name == "Cat")
            .unwrap()
            .vector();
        assert!(dog.cosine(cat) < 0.99);
    }

    #[test]
    fn a_changed_list_is_refused() {
        let edited = format!("{TSV}Volcano\ta volcano\n");
        assert!(matches!(
            LabelSet::from_parts(&edited, VECTORS),
            Err(AiError::Labels(_))
        ));
        assert!(LabelSet::from_parts(TSV, &VECTORS[..VECTORS.len() - 1]).is_err());
        assert!(LabelSet::from_parts(TSV, b"nope").is_err());
    }

    #[test]
    fn crlf_checkouts_still_match() {
        let crlf = TSV.replace('\n', "\r\n");
        assert!(LabelSet::from_parts(&crlf, VECTORS).is_ok());
    }

    #[test]
    fn probability_follows_the_model() {
        // The 50% point is where scale·cos = −bias.
        let mid = -LOGIT_BIAS / LOGIT_SCALE;
        assert!((probability(mid) - 0.5).abs() < 1e-4);
        assert!(probability(0.0) < 0.001);
        assert!(probability(0.3) > 0.99);
    }

    #[test]
    fn tags_take_the_best_and_skip_rejected_ones() {
        let set = LabelSet::builtin().unwrap();
        // An "image" that is exactly the Dog text vector scores Dog first.
        let dog = set.labels().iter().find(|l| l.name == "Dog").unwrap();
        let tags = set.tags_for(dog.vector(), &[]);
        assert_eq!(tags[0].0, TagPath::new(["Dog"]).unwrap());
        assert!(tags.len() <= MAX_TAGS);
        let without = set.tags_for(dog.vector(), &[TagPath::new(["Dog"]).unwrap()]);
        assert!(without.iter().all(|(t, _)| t.to_lightroom() != "Dog"));
    }

    #[test]
    fn a_tag_scores_its_best_phrase_and_gives_the_tag_it_implies() {
        let set = LabelSet::builtin().unwrap();
        let portrait = set.labels().iter().find(|l| l.name == "Portrait").unwrap();
        assert!(portrait.vectors.len() > 3);
        assert_eq!(portrait.implies.as_deref(), Some("People"));
        // An "image" that is exactly one of its later phrases.
        let image = portrait.vectors.last().unwrap();
        assert!((portrait.cosine(image) - 1.0).abs() < 1e-3);
        let tags: Vec<String> = set
            .tags_for(image, &[])
            .into_iter()
            .map(|(t, _)| t.to_lightroom())
            .collect();
        assert_eq!(&tags[..2], ["Portrait", "People"]);
        // A removed implied tag stays removed.
        let people = TagPath::new(["People"]).unwrap();
        assert!(set
            .tags_for(image, &[people])
            .iter()
            .all(|(t, _)| t.to_lightroom() != "People"));
    }

    #[test]
    fn the_list_is_checked() {
        assert!(parse("A\ta|b\tB\nB\tb\n").is_ok());
        assert!(parse("A\ta\tNope\n").is_err());
        assert!(parse("A\ta\tB\tC\n").is_err());
        assert!(parse("A\t|\n").is_err());
        assert_eq!(parse("A\ta| |b\n").unwrap()[0].phrases, 2);
    }
}
