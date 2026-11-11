//! Faces, on this computer (ADR-040): finding the faces in a picture and
//! describing each one, so faces of one person can be found again.
//!
//! - [`FaceModels::find`]: YuNet finds faces, each with a box, 5 landmarks
//!   and a score ([`detect`]).
//! - [`FaceModels::describe`]: the face is straightened with its landmarks
//!   ([`align`]) and SFace turns it into a 128-number unit
//!   [`Embedding`]. Two faces whose embeddings have a cosine similarity of
//!   at least [`SAME_PERSON`] are likely the same person.
//!
//! Both models run with the ONNX Runtime the image encoder uses, on the
//! graphics card when it works. Nothing here reads originals or the
//! catalog: the caller passes a picture (a thumbnail).

pub mod align;
pub mod detect;
pub mod install;
pub mod people;
pub mod scan;

use std::path::Path;

use image::{DynamicImage, RgbImage};
use ort::session::Session;
use ort::value::Tensor;
use serde::{Deserialize, Serialize};

use super::embedding::Embedding;
use super::model::{installed_runtime, open_session, ort_err, Accel};
use super::AiError;

/// The face models' id, stored with each face so a later model can tell
/// its own faces from these.
pub const MODEL_ID: &str = "yunet-2023mar+sface-2021dec@1";

/// Faces scoring less are not kept.
pub const MIN_SCORE: f32 = 0.8;

/// Faces overlapping a better one by more than this are the same face.
pub const MAX_OVERLAP: f32 = 0.3;

/// Cosine similarity from which two faces are likely one person (OpenCV
/// Zoo's threshold for SFace).
pub const SAME_PERSON: f32 = 0.363;

/// The embedding's length.
pub const EMBEDDING_LEN: usize = 128;

/// A face in a picture, in that picture's pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Face {
    /// Top-left corner, width and height of its box.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Right eye, left eye, nose tip, right and left mouth corners.
    pub landmarks: [[f32; 2]; 5],
    /// How sure the detector is, 0–1.
    pub score: f32,
}

impl Face {
    /// The same face in a picture `k` times as big.
    pub fn scaled(&self, k: f32) -> Face {
        Face {
            x: self.x * k,
            y: self.y * k,
            w: self.w * k,
            h: self.h * k,
            landmarks: self.landmarks.map(|[x, y]| [x * k, y * k]),
            score: self.score,
        }
    }

    /// Its box clipped to a `w` × `h` picture, in 0–1 units of it (as
    /// stored and as XMP face regions want it): `(x, y, w, h)`.
    pub fn normalised(&self, w: u32, h: u32) -> (f32, f32, f32, f32) {
        let (pw, ph) = (w.max(1) as f32, h.max(1) as f32);
        let x0 = (self.x / pw).clamp(0.0, 1.0);
        let y0 = (self.y / ph).clamp(0.0, 1.0);
        let x1 = ((self.x + self.w) / pw).clamp(0.0, 1.0);
        let y1 = ((self.y + self.h) / ph).clamp(0.0, 1.0);
        (x0, y0, x1 - x0, y1 - y0)
    }
}

/// The recogniser's input for an aligned face: red, green, blue planes,
/// 0–255 (OpenCV's `FaceRecognizerSF::feature` swaps its BGR to RGB).
pub fn recognizer_input(face: &RgbImage) -> Result<Vec<f32>, AiError> {
    let s = align::SIZE;
    if face.dimensions() != (s, s) {
        return Err(AiError::Image(format!(
            "an aligned face is {s} × {s}, not {:?}",
            face.dimensions()
        )));
    }
    let plane = (s * s) as usize;
    let mut out = vec![0f32; plane * 3];
    for (i, p) in face.pixels().enumerate() {
        for c in 0..3 {
            out[c * plane + i] = f32::from(p.0[c]);
        }
    }
    Ok(out)
}

/// The two face models, loaded. Not shared between threads.
pub struct FaceModels {
    detector: Session,
    recognizer: Session,
    accel: Accel,
}

impl std::fmt::Debug for FaceModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceModels")
            .field("accel", &self.accel)
            .finish()
    }
}

impl FaceModels {
    /// Load the detector and recogniser with ONNX Runtime from `runtime`.
    /// `gpu`: try the graphics card first (for both, or neither).
    pub fn load(
        detector: &Path,
        recognizer: &Path,
        runtime: &Path,
        gpu: bool,
    ) -> Result<Self, AiError> {
        let (detector, accel) = open_session(detector, runtime, gpu)?;
        let (recognizer, _) = open_session(recognizer, runtime, accel != Accel::Cpu)?;
        Ok(Self {
            detector,
            recognizer,
            accel,
        })
    }

    /// Load the installed models ([`install`]) and runtime.
    pub fn load_installed(gpu: bool) -> Result<Self, AiError> {
        Self::load(
            &install::DETECTOR.require()?,
            &install::RECOGNIZER.require()?,
            &installed_runtime()?,
            gpu,
        )
    }

    pub fn accel(&self) -> Accel {
        self.accel
    }

    /// The faces in `img`, best first, in `img`'s pixels.
    pub fn find(&mut self, img: &DynamicImage) -> Result<Vec<Face>, AiError> {
        self.find_rgb(&img.to_rgb8())
    }

    fn find_rgb(&mut self, rgb: &RgbImage) -> Result<Vec<Face>, AiError> {
        let (pixels, scale) = detect::preprocess(rgb)?;
        let n = detect::INPUT as usize;
        let input = Tensor::from_array(([1usize, 3, n, n], pixels)).map_err(ort_err)?;
        let outputs = self
            .detector
            .run(ort::inputs!["input" => input])
            .map_err(ort_err)?;
        let get = |name: &str| -> Result<&[f32], AiError> {
            let v = outputs.get(name).ok_or_else(|| {
                AiError::Inference(format!("the face detector has no output {name}"))
            })?;
            Ok(v.try_extract_tensor::<f32>().map_err(ort_err)?.1)
        };
        let mut grids = Vec::with_capacity(detect::STRIDES.len());
        for stride in detect::STRIDES {
            let [cls, obj, bbox, kps] = detect::output_names(stride);
            grids.push(detect::Grid {
                stride,
                cls: get(&cls)?,
                obj: get(&obj)?,
                bbox: get(&bbox)?,
                kps: get(&kps)?,
            });
        }
        let faces = detect::suppress(detect::decode(&grids, MIN_SCORE)?, MAX_OVERLAP);
        Ok(faces.iter().map(|f| f.scaled(1.0 / scale)).collect())
    }

    /// `face`'s embedding (`face` as [`FaceModels::find`] gave it for
    /// `img`).
    pub fn describe(&mut self, img: &DynamicImage, face: &Face) -> Result<Embedding, AiError> {
        self.describe_rgb(&img.to_rgb8(), face)
    }

    fn describe_rgb(&mut self, rgb: &RgbImage, face: &Face) -> Result<Embedding, AiError> {
        let aligned = align::align(rgb, &face.landmarks)
            .ok_or_else(|| AiError::Image("the face's landmarks are all in one place".into()))?;
        let pixels = recognizer_input(&aligned)?;
        let s = align::SIZE as usize;
        let input = Tensor::from_array(([1usize, 3, s, s], pixels)).map_err(ort_err)?;
        let outputs = self
            .recognizer
            .run(ort::inputs!["data" => input])
            .map_err(ort_err)?;
        let out = outputs
            .get("fc1")
            .ok_or_else(|| AiError::Inference("the face recogniser has no output fc1".into()))?;
        let (_, values) = out.try_extract_tensor::<f32>().map_err(ort_err)?;
        if values.len() != EMBEDDING_LEN {
            return Err(AiError::Inference(format!(
                "the face recogniser gave {} numbers, not {EMBEDDING_LEN}",
                values.len()
            )));
        }
        Embedding::new(values.to_vec())
    }

    /// Every face in `img` with its embedding, best first. A face that
    /// cannot be described is left out.
    pub fn find_and_describe(
        &mut self,
        img: &DynamicImage,
    ) -> Result<Vec<(Face, Embedding)>, AiError> {
        let rgb = img.to_rgb8();
        let mut out = Vec::new();
        for f in self.find_rgb(&rgb)? {
            match self.describe_rgb(&rgb, &f) {
                Ok(e) => out.push((f, e)),
                Err(e @ AiError::Image(_)) => tracing::debug!("face left out: {e}"),
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn the_recogniser_gets_red_green_blue_planes() {
        let img = RgbImage::from_pixel(112, 112, Rgb([1, 2, 3]));
        let v = recognizer_input(&img).unwrap();
        let plane = 112 * 112;
        assert_eq!((v[0], v[plane], v[2 * plane]), (1.0, 2.0, 3.0));
        assert!(recognizer_input(&RgbImage::new(100, 112)).is_err());
    }

    #[test]
    fn boxes_scale_and_normalise() {
        let f = Face {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
            landmarks: [[10.0, 20.0]; 5],
            score: 0.9,
        };
        let g = f.scaled(2.0);
        assert_eq!((g.x, g.y, g.w, g.h), (20.0, 40.0, 60.0, 80.0));
        assert_eq!(g.landmarks[0], [20.0, 40.0]);
        assert_eq!(g.score, 0.9);
        let (x, y, w, h) = f.normalised(100, 200);
        for (got, want) in [(x, 0.1), (y, 0.1), (w, 0.3), (h, 0.2)] {
            assert!((got - want).abs() < 1e-6, "{got} ≠ {want}");
        }
        // Clipped to the picture.
        let off = Face { x: -10.0, ..f };
        assert_eq!(off.normalised(100, 200).0, 0.0);
        assert!((off.normalised(100, 200).2 - 0.2).abs() < 1e-6);
    }

    #[test]
    fn missing_models_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path().join("none.onnx");
        assert!(matches!(
            FaceModels::load(&m, &m, &m, false),
            Err(AiError::ModelMissing(_))
        ));
    }
}
