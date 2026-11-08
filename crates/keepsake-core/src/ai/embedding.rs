//! Unit-length image and text vectors.
//!
//! Stored as int8 with one scale per vector (`value = i8 * scale`, scale =
//! largest component / 127): 768 bytes instead of 3 KB, and the dot product
//! of two stored vectors stays within about 0.001 of the exact one.

use super::AiError;

/// Length of a SigLIP 2 base vector.
pub const DIM: usize = 768;

/// A unit-length vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding(Vec<f32>);

impl Embedding {
    /// Normalise `v` to unit length. Errors on a zero or non-finite vector.
    pub fn new(mut v: Vec<f32>) -> Result<Self, AiError> {
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !norm.is_finite() || norm <= f32::EPSILON {
            return Err(AiError::Inference(
                "the model returned an empty vector".into(),
            ));
        }
        for x in &mut v {
            *x /= norm;
        }
        Ok(Self(v))
    }

    /// Rebuild from stored int8 values.
    pub fn from_quantized(q: &[i8], scale: f32) -> Result<Self, AiError> {
        Self::new(q.iter().map(|&b| f32::from(b) * scale).collect())
    }

    /// int8 values and their scale.
    pub fn quantize(&self) -> (Vec<i8>, f32) {
        let max = self.0.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        if max <= f32::EPSILON {
            return (vec![0; self.0.len()], 0.0);
        }
        let scale = max / 127.0;
        let q = self
            .0
            .iter()
            .map(|x| (x / scale).round().clamp(-127.0, 127.0) as i8)
            .collect();
        (q, scale)
    }

    pub fn values(&self) -> &[f32] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Cosine similarity (both are unit length). 0 when the lengths differ.
    pub fn cosine(&self, other: &Embedding) -> f32 {
        if self.0.len() != other.0.len() {
            return 0.0;
        }
        self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_and_round_trips_through_int8() {
        let raw: Vec<f32> = (0..DIM)
            .map(|i| ((i * 37 % 101) as f32 - 50.0) / 7.0)
            .collect();
        let e = Embedding::new(raw).unwrap();
        assert!((e.cosine(&e) - 1.0).abs() < 1e-5);
        let (q, s) = e.quantize();
        let back = Embedding::from_quantized(&q, s).unwrap();
        assert!(e.cosine(&back) > 0.999, "{}", e.cosine(&back));
    }

    #[test]
    fn rejects_zero_vectors() {
        assert!(Embedding::new(vec![0.0; 8]).is_err());
        assert!(Embedding::new(vec![f32::NAN; 8]).is_err());
    }

    #[test]
    fn cosine_of_different_lengths_is_zero() {
        let a = Embedding::new(vec![1.0, 0.0]).unwrap();
        let b = Embedding::new(vec![1.0, 0.0, 0.0]).unwrap();
        assert_eq!(a.cosine(&b), 0.0);
    }
}
