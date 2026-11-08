//! Finding faces with YuNet (ADR-040): the picture going in and the faces
//! coming out, as OpenCV's `FaceDetectorYN` does it.
//!
//! - In: the picture made to fit [`INPUT`] × [`INPUT`] (the model's fixed
//!   size), the rest black (right and bottom); blue, green, red planes,
//!   0–255, not normalised.
//! - Out: for each of 3 grids (cells of 8, 16 and 32 pixels) a class and an
//!   object score per cell (the face's score is the square root of their
//!   product), a box (centre offset in cells, log of width and height) and
//!   5 landmarks (offsets in cells). Overlapping faces are then merged,
//!   keeping the best (non-maximum suppression).

use fast_image_resize::{
    images::Image as FirImage, FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
};
use image::RgbImage;

use super::Face;
use crate::ai::AiError;

/// The model's input size (YuNet 2023mar has a fixed 640 × 640 input).
pub const INPUT: u32 = 640;

/// The grids' cell sizes, in input pixels.
pub const STRIDES: [u32; 3] = [8, 16, 32];

/// Output names, per grid: `cls_8`, `obj_8`, `bbox_8`, `kps_8`, …
pub fn output_names(stride: u32) -> [String; 4] {
    ["cls", "obj", "bbox", "kps"].map(|k| format!("{k}_{stride}"))
}

/// Cells in a grid.
pub fn cells(stride: u32) -> usize {
    let n = (INPUT / stride) as usize;
    n * n
}

/// The model's input for `img`, and the scale from `img` to it (input
/// pixels per picture pixel).
pub fn preprocess(img: &RgbImage) -> Result<(Vec<f32>, f32), AiError> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err(AiError::Image("empty image".into()));
    }
    let scale = INPUT as f32 / w.max(h) as f32;
    let nw = ((w as f32 * scale).round() as u32).clamp(1, INPUT);
    let nh = ((h as f32 * scale).round() as u32).clamp(1, INPUT);
    let src = FirImage::from_vec_u8(w, h, img.as_raw().clone(), PixelType::U8x3)
        .map_err(|e| AiError::Image(e.to_string()))?;
    let mut dst = FirImage::new(nw, nh, PixelType::U8x3);
    Resizer::new()
        .resize(
            &src,
            &mut dst,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)),
        )
        .map_err(|e| AiError::Image(e.to_string()))?;
    let plane = (INPUT * INPUT) as usize;
    let mut out = vec![0f32; plane * 3];
    let px = dst.buffer();
    for y in 0..nh as usize {
        for x in 0..nw as usize {
            let i = (y * nw as usize + x) * 3;
            let o = y * INPUT as usize + x;
            // RGB in, BGR out.
            out[o] = f32::from(px[i + 2]);
            out[plane + o] = f32::from(px[i + 1]);
            out[2 * plane + o] = f32::from(px[i]);
        }
    }
    Ok((out, scale))
}

/// One grid's outputs, flattened as the model gives them.
#[derive(Debug, Clone, Copy)]
pub struct Grid<'a> {
    pub stride: u32,
    pub cls: &'a [f32],
    pub obj: &'a [f32],
    pub bbox: &'a [f32],
    pub kps: &'a [f32],
}

/// The faces scoring at least `min_score`, in input pixels, before merging.
pub fn decode(grids: &[Grid<'_>], min_score: f32) -> Result<Vec<Face>, AiError> {
    let mut faces = Vec::new();
    for g in grids {
        let cols = (INPUT / g.stride) as usize;
        let n = cells(g.stride);
        if g.cls.len() < n || g.obj.len() < n || g.bbox.len() < n * 4 || g.kps.len() < n * 10 {
            return Err(AiError::Inference(format!(
                "the face detector's grid {} is the wrong size",
                g.stride
            )));
        }
        let s = g.stride as f32;
        for idx in 0..n {
            let score = (g.cls[idx].clamp(0.0, 1.0) * g.obj[idx].clamp(0.0, 1.0)).sqrt();
            if score < min_score {
                continue;
            }
            let (r, c) = ((idx / cols) as f32, (idx % cols) as f32);
            let b = &g.bbox[idx * 4..idx * 4 + 4];
            let (cx, cy) = ((c + b[0]) * s, (r + b[1]) * s);
            let (w, h) = (b[2].exp() * s, b[3].exp() * s);
            let k = &g.kps[idx * 10..idx * 10 + 10];
            let mut landmarks = [[0f32; 2]; 5];
            for (j, lm) in landmarks.iter_mut().enumerate() {
                *lm = [(k[2 * j] + c) * s, (k[2 * j + 1] + r) * s];
            }
            faces.push(Face {
                x: cx - w / 2.0,
                y: cy - h / 2.0,
                w,
                h,
                landmarks,
                score,
            });
        }
    }
    Ok(faces)
}

/// How much two boxes overlap: their intersection over their union.
pub fn iou(a: &Face, b: &Face) -> f32 {
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

/// Best first; a face overlapping a better one by more than `max_iou` is
/// left out.
pub fn suppress(mut faces: Vec<Face>, max_iou: f32) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Face> = Vec::new();
    for f in faces {
        if kept.iter().all(|k| iou(k, &f) <= max_iou) {
            kept.push(f);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// Grids with nothing in them, to put one face in.
    struct Empty {
        cls: Vec<Vec<f32>>,
        obj: Vec<Vec<f32>>,
        bbox: Vec<Vec<f32>>,
        kps: Vec<Vec<f32>>,
    }

    impl Empty {
        fn new() -> Self {
            let z = |k: usize| STRIDES.iter().map(|&s| vec![0.0; cells(s) * k]).collect();
            Self {
                cls: z(1),
                obj: z(1),
                bbox: z(4),
                kps: z(10),
            }
        }

        /// A face in grid `g` at cell (row `r`, column `c`).
        fn put(&mut self, g: usize, r: usize, c: usize, score: f32, size_cells: f32) {
            let i = r * (INPUT / STRIDES[g]) as usize + c;
            self.cls[g][i] = score;
            self.obj[g][i] = score;
            let b = &mut self.bbox[g][i * 4..i * 4 + 4];
            b.copy_from_slice(&[0.5, 0.5, size_cells.ln(), size_cells.ln()]);
            for j in 0..5 {
                self.kps[g][i * 10 + 2 * j] = 0.1 * j as f32;
                self.kps[g][i * 10 + 2 * j + 1] = 0.2;
            }
        }

        fn grids(&self) -> Vec<Grid<'_>> {
            (0..3)
                .map(|g| Grid {
                    stride: STRIDES[g],
                    cls: &self.cls[g],
                    obj: &self.obj[g],
                    bbox: &self.bbox[g],
                    kps: &self.kps[g],
                })
                .collect()
        }
    }

    #[test]
    fn a_face_is_decoded_where_the_model_put_it() {
        let mut e = Empty::new();
        // Grid 16, row 5, column 10: centre (10.5·16, 5.5·16) = (168, 88),
        // 4 cells = 64 pixels wide.
        e.put(1, 5, 10, 0.9, 4.0);
        let faces = decode(&e.grids(), 0.5).unwrap();
        assert_eq!(faces.len(), 1);
        let f = &faces[0];
        assert!((f.score - 0.9).abs() < 1e-6, "√(0.9·0.9)");
        assert!(
            (f.x - 136.0).abs() < 1e-3 && (f.y - 56.0).abs() < 1e-3,
            "{f:?}"
        );
        assert!((f.w - 64.0).abs() < 1e-3 && (f.h - 64.0).abs() < 1e-3);
        assert!((f.landmarks[2][0] - (10.2 * 16.0)).abs() < 1e-3);
        assert!((f.landmarks[2][1] - (5.2 * 16.0)).abs() < 1e-3);
    }

    #[test]
    fn low_scores_are_left_out_and_scores_are_clamped() {
        let mut e = Empty::new();
        e.put(0, 1, 1, 0.4, 2.0);
        e.put(2, 3, 3, 1.7, 2.0);
        let faces = decode(&e.grids(), 0.5).unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].score, 1.0);
    }

    #[test]
    fn a_short_output_is_an_error() {
        let e = Empty::new();
        let mut g = e.grids();
        g[0].kps = &g[0].kps[..10];
        assert!(decode(&g, 0.5).is_err());
    }

    fn face(x: f32, score: f32) -> Face {
        Face {
            x,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            landmarks: [[0.0; 2]; 5],
            score,
        }
    }

    #[test]
    fn overlapping_faces_keep_the_best() {
        // Two boxes almost on top of each other, and one apart.
        let kept = suppress(vec![face(0.0, 0.7), face(10.0, 0.9), face(300.0, 0.8)], 0.3);
        let xs: Vec<f32> = kept.iter().map(|f| f.x).collect();
        assert_eq!(xs, [10.0, 300.0]);
        assert!((iou(&face(0.0, 1.0), &face(50.0, 1.0)) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(iou(&face(0.0, 1.0), &face(200.0, 1.0)), 0.0);
    }

    #[test]
    fn the_picture_is_fitted_and_padded_as_blue_green_red() {
        // 1280 × 640, red: half the size, the bottom half black.
        let img = RgbImage::from_pixel(1280, 640, Rgb([200, 10, 0]));
        let (v, scale) = preprocess(&img).unwrap();
        assert_eq!(scale, 0.5);
        let plane = (INPUT * INPUT) as usize;
        assert_eq!(v.len(), plane * 3);
        let at = |c: usize, x: usize, y: usize| v[c * plane + y * INPUT as usize + x];
        assert_eq!((at(0, 5, 5), at(1, 5, 5), at(2, 5, 5)), (0.0, 10.0, 200.0));
        assert_eq!(at(2, 5, 330), 0.0, "padding below");
        assert_eq!(at(2, 639, 319), 200.0);
        // A small picture is made bigger to fit.
        let (_, s) = preprocess(&RgbImage::new(320, 160)).unwrap();
        assert_eq!(s, 2.0);
        assert!(preprocess(&RgbImage::new(0, 10)).is_err());
    }
}
