//! The built-in tag list and scoring an image against it.
//!
//! `labels.tsv` names each tag and the phrases that describe it; its text
//! vectors (`labels-siglip2-b16-224.bin`) are made once by
//! `scripts/make-label-vectors.py` with the model's text half, so the app
//! needs neither the text model nor a tokenizer. The vector file carries a
//! copy of the list it was made from, and loading checks the two agree.
//!
//! SigLIP scores each tag on its own: `p = sigmoid(scale · cos + bias)`,
//! with the model's trained `logit_scale` and `logit_bias`.

use super::embedding::{Embedding, DIM};
use super::AiError;
use crate::catalog::TagPath;

const TSV: &str = include_str!("labels.tsv");
const VECTORS: &[u8] = include_bytes!("labels-siglip2-b16-224.bin");
const MAGIC: &[u8; 8] = b"KSLV1\0\0\0";

/// `exp(logit_scale)` of google/siglip2-base-patch16-224.
pub const LOGIT_SCALE: f32 = 112.668_9;
/// `logit_bias` of google/siglip2-base-patch16-224.
pub const LOGIT_BIAS: f32 = -16.771_725;

/// A tag is given when its probability is at least this. SigLIP's
/// probabilities are low: on a sample of the product owner's library the
/// best tag usually scored 0.02 to 0.6 and the rest below 0.01.
pub const MIN_PROB: f32 = 0.02;
/// At most this many tags per item, most likely first.
pub const MAX_TAGS: usize = 6;

/// One tag and its text vector.
#[derive(Debug, Clone)]
pub struct Label {
    pub name: String,
    pub vector: Embedding,
}

/// The tag list.
#[derive(Debug, Clone)]
pub struct LabelSet {
    labels: Vec<Label>,
}

/// Tag names of a `labels.tsv` text, in order.
pub fn parse_names(tsv: &str) -> Result<Vec<String>, AiError> {
    let mut out = Vec::new();
    for (i, line) in tsv.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, prompts) = line
            .split_once('\t')
            .ok_or_else(|| AiError::Labels(format!("line {}: no tab", i + 1)))?;
        if name.trim().is_empty() || prompts.trim().is_empty() {
            return Err(AiError::Labels(format!(
                "line {}: empty name or phrases",
                i + 1
            )));
        }
        out.push(name.trim().to_owned());
    }
    Ok(out)
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
        let names = parse_names(tsv)?;
        let count = u32_le(&mut b)? as usize;
        let dim = u32_le(&mut b)? as usize;
        if count != names.len() || dim != DIM {
            return Err(AiError::Labels(format!(
                "{count} vectors of {dim} for {} labels",
                names.len()
            )));
        }
        let mut labels = Vec::with_capacity(count);
        for name in names {
            let s = take(&mut b, 4)?;
            let scale = f32::from_le_bytes([s[0], s[1], s[2], s[3]]);
            let q: Vec<i8> = take(&mut b, dim)?
                .iter()
                .map(|&x| i8::from_le_bytes([x]))
                .collect();
            labels.push(Label {
                name,
                vector: Embedding::from_quantized(&q, scale)?,
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
            .map(|l| (l.name.as_str(), image.cosine(&l.vector)))
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        out.into_iter().map(|(n, c)| (n, probability(c))).collect()
    }

    /// The tags to give an image: at least [`MIN_PROB`], at most
    /// [`MAX_TAGS`], leaving out `rejected` ones.
    pub fn tags_for(&self, image: &Embedding, rejected: &[TagPath]) -> Vec<(TagPath, f64)> {
        self.probabilities(image)
            .into_iter()
            .filter(|(_, p)| *p >= MIN_PROB)
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
            assert_eq!(l.vector.len(), DIM);
        }
        // Different tags have different vectors.
        let dog = &set
            .labels()
            .iter()
            .find(|l| l.name == "Dog")
            .unwrap()
            .vector;
        let cat = &set
            .labels()
            .iter()
            .find(|l| l.name == "Cat")
            .unwrap()
            .vector;
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
        let tags = set.tags_for(&dog.vector, &[]);
        assert_eq!(tags[0].0, TagPath::new(["Dog"]).unwrap());
        assert!(tags.len() <= MAX_TAGS);
        let without = set.tags_for(&dog.vector, &[TagPath::new(["Dog"]).unwrap()]);
        assert!(without.iter().all(|(t, _)| t.to_lightroom() != "Dog"));
    }
}
