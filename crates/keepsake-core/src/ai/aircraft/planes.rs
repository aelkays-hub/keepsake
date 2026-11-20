//! Where the aircraft are in a picture (ADR-041, the aircraft finder), so
//! that only the text painted on them is read: not a sign, a jet bridge, a
//! truck or the photographer's signature.
//!
//! YOLOX-s (Apache-2.0, Megvii; COCO's 80 kinds of thing, of which
//! "airplane" is the fifth), as ONNX, with the ONNX Runtime of the other AI
//! features. The picture is shrunk to fit 640×640 (padded with grey), the
//! model gives 8,400 boxes in three grids (strides 8, 16 and 32), and the
//! airplanes sure enough are kept, overlapping ones merged.

use std::path::Path;

use image::{DynamicImage, RgbImage};
use ort::session::Session;
use ort::value::Tensor;

use super::ocr::resize;
use crate::ai::model::{open_session, ort_err, Accel};
use crate::ai::AiError;

/// The model's input: a square this many pixels across.
pub const SIDE: u32 = 640;
/// Kinds of thing in the model's output (COCO).
const CLASSES: usize = 80;
/// COCO's "airplane".
const AIRPLANE: usize = 4;
/// How sure (box × class) an airplane must be.
pub const MIN_SCORE: f32 = 0.3;
/// Boxes overlapping more than this (intersection over union) are one.
const SAME_BOX: f32 = 0.45;
/// The grey the picture is padded with.
const PAD: f32 = 114.0;

/// An aircraft found: where it is in the picture's pixels, and how surely.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
}

impl Plane {
    /// The box grown by `ratio` of its size on every side and kept inside
    /// a `w`×`h` picture: `[x, y, w, h]` in whole pixels.
    pub fn grown(&self, ratio: f32, w: u32, h: u32) -> [u32; 4] {
        let (gx, gy) = (self.w * ratio, self.h * ratio);
        let x0 = (self.x - gx).clamp(0.0, w as f32);
        let y0 = (self.y - gy).clamp(0.0, h as f32);
        let x1 = (self.x + self.w + gx).clamp(0.0, w as f32);
        let y1 = (self.y + self.h + gy).clamp(0.0, h as f32);
        let (x0, y0) = (x0.floor() as u32, y0.floor() as u32);
        [
            x0,
            y0,
            (x1.ceil() as u32).saturating_sub(x0),
            (y1.ceil() as u32).saturating_sub(y0),
        ]
    }

    fn iou(&self, o: &Plane) -> f32 {
        let ix = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let iy = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        let i = ix.max(0.0) * iy.max(0.0);
        let u = self.w * self.h + o.w * o.h - i;
        if u > 0.0 {
            i / u
        } else {
            0.0
        }
    }
}

/// How much a `w`×`h` picture is scaled to fit [`SIDE`], and its size then.
pub fn fit(w: u32, h: u32) -> (f32, u32, u32) {
    let k = (SIDE as f32 / w.max(1) as f32).min(SIDE as f32 / h.max(1) as f32);
    let nw = ((w as f32 * k) as u32).clamp(1, SIDE);
    let nh = ((h as f32 * k) as u32).clamp(1, SIDE);
    (k, nw, nh)
}

/// The model's input from a picture already scaled to fit: blue, green,
/// red planes, 0–255, padded with grey to [`SIDE`]×[`SIDE`] on the right
/// and bottom.
pub fn input(small: &RgbImage) -> Vec<f32> {
    let n = (SIDE * SIDE) as usize;
    let mut out = vec![PAD; 3 * n];
    for (x, y, p) in small.enumerate_pixels() {
        if x >= SIDE || y >= SIDE {
            continue;
        }
        let i = (y * SIDE + x) as usize;
        out[i] = f32::from(p[2]);
        out[n + i] = f32::from(p[1]);
        out[2 * n + i] = f32::from(p[0]);
    }
    out
}

/// The airplanes in the model's output (`rows` of `5 + CLASSES`: centre,
/// size as grid offsets, box sureness, class sureness), in the pixels of a
/// picture scaled by `k`; surest first, overlapping ones merged.
pub fn decode(out: &[f32], k: f32) -> Vec<Plane> {
    let width = 5 + CLASSES;
    let mut found = Vec::new();
    let mut row = 0;
    for stride in [8usize, 16, 32] {
        let cells = SIDE as usize / stride;
        for gy in 0..cells {
            for gx in 0..cells {
                let Some(r) = out.get(row * width..(row + 1) * width) else {
                    return merge(found);
                };
                row += 1;
                let score = r[4] * r[5 + AIRPLANE];
                if score < MIN_SCORE {
                    continue;
                }
                let s = stride as f32;
                let cx = (r[0] + gx as f32) * s;
                let cy = (r[1] + gy as f32) * s;
                let (w, h) = (r[2].exp() * s, r[3].exp() * s);
                found.push(Plane {
                    x: (cx - w / 2.0) / k,
                    y: (cy - h / 2.0) / k,
                    w: w / k,
                    h: h / k,
                    score,
                });
            }
        }
    }
    merge(found)
}

/// Surest first; a box overlapping a surer one is dropped.
fn merge(mut found: Vec<Plane>) -> Vec<Plane> {
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut out: Vec<Plane> = Vec::new();
    for p in found {
        if out.iter().all(|o| o.iou(&p) <= SAME_BOX) {
            out.push(p);
        }
    }
    out
}

/// The loaded aircraft finder.
pub struct PlaneFinder {
    session: Session,
    names: (String, String),
    accel: Accel,
}

impl std::fmt::Debug for PlaneFinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaneFinder")
            .field("accel", &self.accel)
            .finish()
    }
}

impl PlaneFinder {
    /// Load the model at `model` with ONNX Runtime from `runtime`; `gpu`:
    /// on the graphics card if it works.
    pub fn load(model: &Path, runtime: &Path, gpu: bool) -> Result<Self, AiError> {
        let (session, accel) = open_session(model, runtime, gpu)?;
        let first = |o: &[ort::value::Outlet], io: &str| {
            o.first()
                .map(|o| o.name().to_owned())
                .ok_or_else(|| AiError::Inference(format!("the aircraft finder has no {io}")))
        };
        let names = (
            first(session.inputs(), "input")?,
            first(session.outputs(), "output")?,
        );
        Ok(Self {
            session,
            names,
            accel,
        })
    }

    pub fn accel(&self) -> Accel {
        self.accel
    }

    /// The aircraft in `img`, surest first.
    pub fn find(&mut self, img: &DynamicImage) -> Result<Vec<Plane>, AiError> {
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        if w == 0 || h == 0 {
            return Err(AiError::Image("empty image".into()));
        }
        let (k, nw, nh) = fit(w, h);
        let small = resize(&rgb, nw, nh)?;
        let side = SIDE as usize;
        let t = Tensor::from_array(([1usize, 3, side, side], input(&small))).map_err(ort_err)?;
        let outputs = self
            .session
            .run(ort::inputs![self.names.0.as_str() => t])
            .map_err(ort_err)?;
        let out = outputs
            .get(self.names.1.as_str())
            .ok_or_else(|| AiError::Inference("the aircraft finder gave nothing".into()))?;
        let (_, rows) = out.try_extract_tensor::<f32>().map_err(ort_err)?;
        Ok(decode(rows, k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 5 + CLASSES;

    /// An output with every row empty but the given ones (row, values).
    fn output(rows: &[(usize, [f32; 4], f32, usize, f32)]) -> Vec<f32> {
        let n = 80 * 80 + 40 * 40 + 20 * 20;
        let mut out = vec![0.0; n * W];
        for &(row, b, obj, class, p) in rows {
            let r = &mut out[row * W..(row + 1) * W];
            r[..4].copy_from_slice(&b);
            r[4] = obj;
            r[5 + class] = p;
        }
        out
    }

    #[test]
    fn a_picture_is_fitted_and_padded_blue_first() {
        assert_eq!(fit(1280, 640), (0.5, 640, 320));
        assert_eq!(fit(640, 1280), (0.5, 320, 640));
        let mut small = RgbImage::new(2, 1);
        small.put_pixel(1, 0, image::Rgb([10, 20, 30]));
        let x = input(&small);
        let n = (SIDE * SIDE) as usize;
        assert_eq!(x.len(), 3 * n);
        assert_eq!((x[1], x[n + 1], x[2 * n + 1]), (30.0, 20.0, 10.0));
        // Outside the picture: grey.
        assert_eq!(x[SIDE as usize], PAD);
    }

    #[test]
    fn airplanes_are_decoded_in_the_picture_s_pixels() {
        // Stride 8 grid, cell (3, 2): centre (3.5 * 8, 2.5 * 8) = (28, 20),
        // size e^0 * 8 = 8; the picture was scaled by 0.5.
        let row = 2 * 80 + 3;
        let planes = decode(
            &output(&[(row, [0.5, 0.5, 0.0, 0.0], 0.9, AIRPLANE, 0.9)]),
            0.5,
        );
        assert_eq!(planes.len(), 1);
        let p = planes[0];
        assert!((p.x - 48.0).abs() < 1e-3 && (p.y - 32.0).abs() < 1e-3);
        assert!((p.w - 16.0).abs() < 1e-3 && (p.h - 16.0).abs() < 1e-3);
        assert!((p.score - 0.81).abs() < 1e-4);
        // Stride 32: the last grid.
        let row = 80 * 80 + 40 * 40 + 20 + 1;
        let p = decode(
            &output(&[(row, [0.0, 0.0, 1.0, 0.0], 1.0, AIRPLANE, 1.0)]),
            1.0,
        )[0];
        // Cell (1, 1): centre (32, 32); width e × 32, height 32.
        assert!((p.x - (32.0 - 32.0 * 1f32.exp() / 2.0)).abs() < 1e-3);
        assert!((p.y - 16.0).abs() < 1e-3 && (p.h - 32.0).abs() < 1e-3);
    }

    #[test]
    fn other_things_and_unsure_ones_are_left_out_and_overlaps_merged() {
        let out = output(&[
            // A bus (class 5), sure.
            (10, [0.0, 0.0, 1.0, 1.0], 0.99, 5, 0.99),
            // An airplane, too unsure.
            (20, [0.0, 0.0, 1.0, 1.0], 0.5, AIRPLANE, 0.5),
            // The same airplane twice, the next cell over; and another far.
            (100, [0.0, 0.0, 2.0, 2.0], 0.9, AIRPLANE, 0.9),
            (101, [0.0, 0.0, 2.0, 2.0], 0.9, AIRPLANE, 0.8),
            (5000, [0.0, 0.0, 2.0, 2.0], 0.9, AIRPLANE, 0.7),
        ]);
        let planes = decode(&out, 1.0);
        assert_eq!(planes.len(), 2, "{planes:?}");
        assert!(planes[0].score > planes[1].score);
    }

    #[test]
    fn a_box_grows_and_stays_inside_the_picture() {
        let p = Plane {
            x: 10.0,
            y: 50.0,
            w: 100.0,
            h: 40.0,
            score: 0.9,
        };
        assert_eq!(p.grown(0.1, 1000, 1000), [0, 46, 120, 48]);
        assert_eq!(p.grown(0.1, 100, 80), [0, 46, 100, 34]);
    }
}
