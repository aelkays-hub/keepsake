//! Guessing an aircraft's type from its look (ADR-041).
//!
//! The picture's SigLIP 2 embedding (the one Tag photos stores) is compared
//! with the text vectors of the types in `aircraft-types.tsv`, made once by
//! `scripts/make-label-vectors.py … aircraft-types` like the tag list's
//! ([`crate::ai::labels`]). Airliners are listed as families: a 777-200
//! and a 777-300 look too alike to tell apart this way.
//!
//! It is a guess, and not a strong one: on 40 of the product owner's
//! photos the best type was right for narrow-bodies and display teams but
//! often wrong between wide-bodies (an A330 scored as an A320, a 787 as a
//! 757). So it is only given when it stands out: the best type must score
//! at least [`GUESS_MIN`] and [`LEAD_MIN`] times the second. With these the
//! sample's guesses were four Boeing 737s, all right where the type was
//! known. A type painted on the aircraft and read by [`super::marks`] is
//! always preferred.

use crate::ai::embedding::Embedding;
use crate::ai::labels::{probability, LabelSet};
use crate::ai::AiError;

const TSV: &str = include_str!("aircraft-types.tsv");
const VECTORS: &[u8] = include_bytes!("aircraft-types-siglip2-b16-224.bin");

/// The version of the type list (part of the reader's id).
pub const LIST_VERSION: u32 = 1;

/// The best type is given when its probability is at least this.
pub const GUESS_MIN: f32 = 0.3;
/// And at least this many times the second best's.
pub const LEAD_MIN: f32 = 3.0;

/// The aircraft type list.
#[derive(Debug, Clone)]
pub struct TypeList {
    set: LabelSet,
}

impl TypeList {
    /// The list built into Keepsake.
    pub fn builtin() -> Result<Self, AiError> {
        Ok(Self {
            set: LabelSet::from_parts(TSV, VECTORS)?,
        })
    }

    pub fn len(&self) -> usize {
        self.set.labels().len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.labels().is_empty()
    }

    /// Every type's probability for a picture, most likely first.
    pub fn ranked(&self, image: &Embedding) -> Vec<(&str, f32)> {
        let mut out: Vec<(&str, f32)> = self
            .set
            .labels()
            .iter()
            .map(|l| (l.name.as_str(), l.cosine(image)))
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        out.into_iter().map(|(n, c)| (n, probability(c))).collect()
    }

    /// The type the picture looks like, when one stands out.
    pub fn guess(&self, image: &Embedding) -> Option<(&str, f32)> {
        stands_out(&self.ranked(image))
    }
}

/// The first of `ranked` (most likely first) when it stands out.
fn stands_out<'a>(ranked: &[(&'a str, f32)]) -> Option<(&'a str, f32)> {
    let (name, p) = *ranked.first()?;
    let second = ranked.get(1).map_or(0.0, |r| r.1);
    (p >= GUESS_MIN && p >= second * LEAD_MIN).then_some((name, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_matches_its_vectors() {
        let t = TypeList::builtin().unwrap();
        assert!(t.len() >= 60);
        // A "picture" that is exactly a type's text vector is that type.
        let b747 = t
            .set
            .labels()
            .iter()
            .find(|l| l.name == "Boeing 747")
            .unwrap();
        assert_eq!(t.ranked(b747.vector())[0].0, "Boeing 747");
    }

    #[test]
    fn a_guess_must_stand_out() {
        assert_eq!(stands_out(&[("A", 0.5), ("B", 0.1)]), Some(("A", 0.5)));
        assert_eq!(stands_out(&[("A", 0.5), ("B", 0.2)]), None, "too close");
        assert_eq!(stands_out(&[("A", 0.2), ("B", 0.001)]), None, "too unsure");
        assert_eq!(stands_out(&[("A", 0.4)]), Some(("A", 0.4)));
        assert_eq!(stands_out(&[]), None);
    }
}
