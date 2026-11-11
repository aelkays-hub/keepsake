//! Reading the text in a picture (ADR-041) with PP-OCR's two models, run
//! with the ONNX Runtime the other AI features use.
//!
//! 1. The **detector** (a DBNet) turns the picture into a map of how likely
//!    each pixel is text. Pixels over [`PIXEL_MIN`] that touch form a
//!    region; a region whose mean is at least [`BOX_MIN`] becomes the
//!    smallest rectangle around it (at any angle), grown a little because
//!    the detector marks the middle of the letters ([`boxes`]).
//! 2. Each rectangle is cut out of the picture and straightened
//!    ([`crop`]), then the **recogniser** reads it: it gives, for each
//!    step along the line, how likely each character of its dictionary is,
//!    and repeats and blanks are dropped ([`ctc_decode`]).
//!
//! The preprocessing is PaddleOCR's: the detector wants blue, green, red
//! planes normalised with the ImageNet mean and deviation, sides that are
//! multiples of 32; the recogniser a line [`REC_HEIGHT`] high, `−1…1`,
//! padded on the right to at least [`REC_MIN_WIDTH`].
//!
//! Nothing here reads originals or the catalog: the caller passes a
//! picture.

use std::path::Path;

use fast_image_resize::{
    images::Image as FirImage, FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
};
use image::{DynamicImage, Rgb, RgbImage};
use ort::session::Session;
use ort::value::Tensor;

use crate::ai::model::{open_session, ort_err, Accel};
use crate::ai::AiError;

/// The recogniser's characters (PaddleOCR's `ppocrv5_en_dict.txt`,
/// Apache-2.0), one a line. The model's class 0 is the blank, then these,
/// then a space.
const DICT: &str = include_str!("ppocrv5_en_dict.txt");

/// A pixel is text from this probability.
pub const PIXEL_MIN: f32 = 0.3;
/// A region is text when its pixels' mean probability is at least this.
pub const BOX_MIN: f32 = 0.5;
/// How much a region is grown: by `area × UNCLIP / perimeter` on each side.
pub const UNCLIP: f32 = 1.6;
/// Regions thinner than this (detector pixels) are noise.
const MIN_SIDE: f32 = 3.0;
/// The detector's sides are multiples of this.
const DET_STEP: u32 = 32;
/// The height of a line for the recogniser.
pub const REC_HEIGHT: u32 = 48;
/// A line is padded to at least this width.
pub const REC_MIN_WIDTH: u32 = 320;
/// And squeezed to at most this.
const REC_MAX_WIDTH: u32 = 2400;
/// A line cut out of the picture is at most this high.
pub const CROP_MAX_HEIGHT: f32 = 96.0;
/// A line read with less confidence than this is dropped.
pub const TEXT_MIN: f32 = 0.5;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// A rectangle at any angle: its corners in picture pixels, clockwise from
/// the top left of the text (so `0 → 1` runs along the line).
pub type Quad = [[f32; 2]; 4];

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// One line of text found in a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub text: String,
    /// How sure the recogniser is, 0–1 (the mean over its characters).
    pub score: f32,
    pub quad: Quad,
}

impl TextLine {
    /// The line's height in picture pixels (the letters' size, roughly).
    pub fn height(&self) -> f32 {
        dist(self.quad[0], self.quad[3])
    }

    pub fn width(&self) -> f32 {
        dist(self.quad[0], self.quad[1])
    }

    pub fn center(&self) -> [f32; 2] {
        let q = &self.quad;
        [
            (q[0][0] + q[1][0] + q[2][0] + q[3][0]) / 4.0,
            (q[0][1] + q[1][1] + q[2][1] + q[3][1]) / 4.0,
        ]
    }
}

fn resize(rgb: &RgbImage, w: u32, h: u32) -> Result<RgbImage, AiError> {
    if rgb.dimensions() == (w, h) {
        return Ok(rgb.clone());
    }
    let bad = |e: &dyn std::fmt::Display| AiError::Image(e.to_string());
    let src = FirImage::from_vec_u8(
        rgb.width(),
        rgb.height(),
        rgb.as_raw().clone(),
        PixelType::U8x3,
    )
    .map_err(|e| bad(&e))?;
    let mut dst = FirImage::new(w, h, PixelType::U8x3);
    Resizer::new()
        .resize(
            &src,
            &mut dst,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)),
        )
        .map_err(|e| bad(&e))?;
    RgbImage::from_raw(w, h, dst.into_vec()).ok_or_else(|| AiError::Image("resize failed".into()))
}

/// The detector's input size for a `w` × `h` picture: its long side at most
/// `max_side` (a picture is never enlarged), both sides rounded to
/// multiples of 32.
pub fn det_size(w: u32, h: u32, max_side: u32) -> (u32, u32) {
    let long = w.max(h).max(1) as f32;
    let k = (max_side as f32 / long).min(1.0);
    let round = |v: u32| {
        let s = (v as f32 * k / DET_STEP as f32).round() as u32;
        s.max(1) * DET_STEP
    };
    (round(w), round(h))
}

/// The detector's input for `rgb` (already at the detector's size): blue,
/// green, red planes, normalised.
pub fn det_input(rgb: &RgbImage) -> Vec<f32> {
    let plane = (rgb.width() * rgb.height()) as usize;
    let mut out = vec![0f32; plane * 3];
    for (i, p) in rgb.pixels().enumerate() {
        for c in 0..3 {
            // Plane c holds channel 2 − c: blue first.
            let v = f32::from(p.0[2 - c]) / 255.0;
            out[c * plane + i] = (v - MEAN[c]) / STD[c];
        }
    }
    out
}

/// The convex hull of `pts` (Andrew's monotone chain), anticlockwise in
/// picture coordinates turned upright; without repeats.
fn hull(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut h: Vec<[f32; 2]> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = h.len();
        let iter: Box<dyn Iterator<Item = &[f32; 2]>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for p in iter {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], *p) <= 0.0 {
                h.pop();
            }
            h.push(*p);
        }
        h.pop();
    }
    h
}

/// A rectangle at any angle: centre, the unit vector along the text, and
/// half its length and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub c: [f32; 2],
    /// Along the line, pointing right (or down, for an upright line).
    pub u: [f32; 2],
    pub half_len: f32,
    pub half_height: f32,
}

impl Rect {
    /// Its corners, clockwise from the top left of the text.
    pub fn quad(&self) -> Quad {
        let (u, v) = (self.u, [-self.u[1], self.u[0]]);
        let at = |a: f32, b: f32| {
            [
                self.c[0] + u[0] * a * self.half_len + v[0] * b * self.half_height,
                self.c[1] + u[1] * a * self.half_len + v[1] * b * self.half_height,
            ]
        };
        [at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]
    }

    /// Grown on every side by `area × ratio / perimeter` (DBNet's unclip).
    pub fn grown(&self, ratio: f32) -> Rect {
        let (w, h) = (self.half_len * 2.0, self.half_height * 2.0);
        let d = w * h * ratio / (2.0 * (w + h)).max(f32::MIN_POSITIVE);
        Rect {
            half_len: self.half_len + d,
            half_height: self.half_height + d,
            ..*self
        }
    }
}

/// The smallest rectangle around the points of a convex hull. Text runs
/// along its longer side, unless it is nearly square: then along the side
/// closer to level.
pub fn min_rect(hull: &[[f32; 2]]) -> Option<Rect> {
    if hull.len() < 3 {
        return None;
    }
    let mut best: Option<(f32, Rect)> = None;
    for i in 0..hull.len() {
        let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
        let len = dist(a, b);
        if len <= f32::EPSILON {
            continue;
        }
        let e = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
        let n = [-e[1], e[0]];
        let (mut e0, mut e1, mut n0, mut n1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in hull {
            let (pe, pn) = (p[0] * e[0] + p[1] * e[1], p[0] * n[0] + p[1] * n[1]);
            e0 = e0.min(pe);
            e1 = e1.max(pe);
            n0 = n0.min(pn);
            n1 = n1.max(pn);
        }
        let area = (e1 - e0) * (n1 - n0);
        if best.as_ref().is_none_or(|(a, _)| area < *a) {
            let (ce, cn) = ((e0 + e1) / 2.0, (n0 + n1) / 2.0);
            let c = [e[0] * ce + n[0] * cn, e[1] * ce + n[1] * cn];
            let (he, hn) = ((e1 - e0) / 2.0, (n1 - n0) / 2.0);
            // Which side the text runs along.
            let along_e = if he >= hn * 1.5 {
                true
            } else if hn >= he * 1.5 {
                false
            } else {
                e[0].abs() >= n[0].abs()
            };
            let (mut u, half_len, half_height) = if along_e { (e, he, hn) } else { (n, hn, he) };
            if u[0] < 0.0 || (u[0] == 0.0 && u[1] < 0.0) {
                u = [-u[0], -u[1]];
            }
            best = Some((
                area,
                Rect {
                    c,
                    u,
                    half_len,
                    half_height,
                },
            ));
        }
    }
    best.map(|(_, r)| r)
}

/// The text rectangles of a probability map (`w` × `h`, row by row), each
/// with its region's mean probability, in the map's pixels.
pub fn boxes(prob: &[f32], w: usize, h: usize) -> Vec<(Rect, f32)> {
    if prob.len() != w * h {
        return Vec::new();
    }
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if seen[start] || prob[start] < PIXEL_MIN {
            continue;
        }
        // One region: its rows' first and last pixels, and its mean.
        let mut rows: std::collections::BTreeMap<usize, (usize, usize)> = Default::default();
        let (mut sum, mut n) = (0f64, 0u32);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            sum += f64::from(prob[i]);
            n += 1;
            let r = rows.entry(y).or_insert((x, x));
            r.0 = r.0.min(x);
            r.1 = r.1.max(x);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if !seen[j] && prob[j] >= PIXEL_MIN {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let score = (sum / f64::from(n.max(1))) as f32;
        if score < BOX_MIN {
            continue;
        }
        // A pixel covers a unit square: its corners bound the region.
        let mut pts = Vec::with_capacity(rows.len() * 4);
        for (y, (x0, x1)) in &rows {
            let (y, x0, x1) = (*y as f32, *x0 as f32, *x1 as f32 + 1.0);
            pts.extend([[x0, y], [x1, y], [x0, y + 1.0], [x1, y + 1.0]]);
        }
        let Some(rect) = min_rect(&hull(pts)) else {
            continue;
        };
        if rect.half_height * 2.0 < MIN_SIDE || rect.half_len * 2.0 < MIN_SIDE {
            continue;
        }
        out.push((rect.grown(UNCLIP), score));
    }
    out
}

/// The colour at `(x, y)` by bilinear interpolation; the nearest edge
/// pixel outside the picture.
fn sample(img: &RgbImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let (x, y) = (x.clamp(0.0, (w - 1) as f32), y.clamp(0.0, (h - 1) as f32));
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let px = |xi: i64, yi: i64| -> [f32; 3] {
        let p = img.get_pixel(xi.min(w - 1) as u32, yi.min(h - 1) as u32).0;
        [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]
    };
    let (p00, p10, p01, p11) = (
        px(x0, y0),
        px(x0 + 1, y0),
        px(x0, y0 + 1),
        px(x0 + 1, y0 + 1),
    );
    let mut out = [0.0; 3];
    for c in 0..3 {
        let top = p00[c] + (p10[c] - p00[c]) * fx;
        let bottom = p01[c] + (p11[c] - p01[c]) * fx;
        out[c] = top + (bottom - top) * fy;
    }
    out
}

/// The part of `img` inside `quad`, straightened (the quad's first side
/// along the top), at the picture's own scale, or smaller so that it is at
/// most [`CROP_MAX_HEIGHT`] high. `None` when it is empty.
pub fn crop(img: &RgbImage, quad: &Quad) -> Option<RgbImage> {
    if img.width() == 0 || img.height() == 0 {
        return None;
    }
    let (qw, qh) = (dist(quad[0], quad[1]), dist(quad[0], quad[3]));
    if qw < 2.0 || qh < 2.0 {
        return None;
    }
    // Big letters are read smaller: the recogniser wants 48 px anyway.
    let k = (CROP_MAX_HEIGHT / qh).min(1.0);
    let w = (qw * k).round().clamp(2.0, 8192.0) as u32;
    let h = (qh * k).round().max(2.0) as u32;
    let (o, across, down) = (quad[0], quad[1], quad[3]);
    let mut out = RgbImage::new(w, h);
    for v in 0..h {
        let fv = (v as f32 + 0.5) / h as f32;
        for u in 0..w {
            let fu = (u as f32 + 0.5) / w as f32;
            let x = o[0] + (across[0] - o[0]) * fu + (down[0] - o[0]) * fv - 0.5;
            let y = o[1] + (across[1] - o[1]) * fu + (down[1] - o[1]) * fv - 0.5;
            let c = sample(img, x, y);
            out.put_pixel(u, v, Rgb(c.map(|c| c.round().clamp(0.0, 255.0) as u8)));
        }
    }
    Some(out)
}

/// The recogniser's input for a straightened line: blue, green, red planes
/// [`REC_HEIGHT`] high, `−1…1`, padded with zeros on the right. Returns it
/// with its width.
pub fn rec_input(line: &RgbImage) -> Result<(Vec<f32>, u32), AiError> {
    let (w, h) = line.dimensions();
    if w == 0 || h == 0 {
        return Err(AiError::Image("empty text line".into()));
    }
    let text_w = ((REC_HEIGHT as f32 * w as f32 / h as f32).ceil() as u32).clamp(8, REC_MAX_WIDTH);
    let small = resize(line, text_w, REC_HEIGHT)?;
    let width = text_w.max(REC_MIN_WIDTH);
    let plane = (width * REC_HEIGHT) as usize;
    let mut out = vec![0f32; plane * 3];
    for (x, y, p) in small.enumerate_pixels() {
        let i = (y * width + x) as usize;
        for c in 0..3 {
            out[c * plane + i] = (f32::from(p.0[2 - c]) / 255.0 - 0.5) / 0.5;
        }
    }
    Ok((out, width))
}

/// The recogniser's dictionary: class `i + 1` is `dict[i]` (class 0 is the
/// blank).
pub fn dictionary() -> Vec<&'static str> {
    let mut d: Vec<&str> = DICT
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty())
        .collect();
    d.push(" ");
    d
}

/// Read the recogniser's output (`steps` × `classes` probabilities): at
/// each step the likeliest class; blanks and repeats dropped. Returns the
/// text and the mean probability of its characters (0 for no text).
pub fn ctc_decode(probs: &[f32], steps: usize, classes: usize, dict: &[&str]) -> (String, f32) {
    let mut text = String::new();
    let (mut sum, mut n) = (0f32, 0u32);
    let mut last = 0usize;
    for t in 0..steps {
        let Some(row) = probs.get(t * classes..(t + 1) * classes) else {
            break;
        };
        let (best, p) = row
            .iter()
            .enumerate()
            .fold(
                (0usize, f32::MIN),
                |(bi, bp), (i, p)| {
                    if *p > bp {
                        (i, *p)
                    } else {
                        (bi, bp)
                    }
                },
            );
        if best != 0 && best != last {
            if let Some(ch) = dict.get(best - 1) {
                text.push_str(ch);
                sum += p;
                n += 1;
            }
        }
        last = best;
    }
    let score = if n == 0 { 0.0 } else { sum / n as f32 };
    (text.trim().to_owned(), score)
}

/// The two text models, loaded. Not shared between threads.
pub struct TextReader {
    detector: Session,
    recognizer: Session,
    det_names: (String, String),
    rec_names: (String, String),
    dict: Vec<&'static str>,
    accel: Accel,
    /// The detector sees the picture with its long side at most this.
    pub max_side: u32,
}

impl std::fmt::Debug for TextReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextReader")
            .field("accel", &self.accel)
            .field("max_side", &self.max_side)
            .finish()
    }
}

fn names(s: &Session, what: &str) -> Result<(String, String), AiError> {
    let first = |o: &[ort::value::Outlet], io: &str| {
        o.first()
            .map(|o| o.name().to_owned())
            .ok_or_else(|| AiError::Inference(format!("the text {what} has no {io}")))
    };
    Ok((first(s.inputs(), "input")?, first(s.outputs(), "output")?))
}

impl TextReader {
    /// Load the detector and recogniser with ONNX Runtime from `runtime`.
    /// `gpu`: run the detector on the graphics card if it works. The
    /// recogniser always runs on the processor: it is small, and every
    /// line has another width, which a graphics card plans anew each time.
    pub fn load(
        detector: &Path,
        recognizer: &Path,
        runtime: &Path,
        gpu: bool,
        max_side: u32,
    ) -> Result<Self, AiError> {
        let (detector, accel) = open_session(detector, runtime, gpu)?;
        let (recognizer, _) = open_session(recognizer, runtime, false)?;
        Ok(Self {
            det_names: names(&detector, "detector")?,
            rec_names: names(&recognizer, "recogniser")?,
            detector,
            recognizer,
            dict: dictionary(),
            accel,
            max_side,
        })
    }

    pub fn accel(&self) -> Accel {
        self.accel
    }

    /// Where text is in `rgb`: rectangles in its pixels, with the
    /// detector's confidence.
    pub fn find(&mut self, rgb: &RgbImage) -> Result<Vec<(Quad, f32)>, AiError> {
        let (w, h) = rgb.dimensions();
        if w == 0 || h == 0 {
            return Err(AiError::Image("empty image".into()));
        }
        let (dw, dh) = det_size(w, h, self.max_side);
        let small = resize(rgb, dw, dh)?;
        let input = Tensor::from_array(([1usize, 3, dh as usize, dw as usize], det_input(&small)))
            .map_err(ort_err)?;
        let outputs = self
            .detector
            .run(ort::inputs![self.det_names.0.as_str() => input])
            .map_err(ort_err)?;
        let out = outputs
            .get(self.det_names.1.as_str())
            .ok_or_else(|| AiError::Inference("the text detector gave nothing".into()))?;
        let (_, prob) = out.try_extract_tensor::<f32>().map_err(ort_err)?;
        let (kx, ky) = (w as f32 / dw as f32, h as f32 / dh as f32);
        Ok(boxes(prob, dw as usize, dh as usize)
            .into_iter()
            .map(|(r, score)| (r.quad().map(|[x, y]| [x * kx, y * ky]), score))
            .collect())
    }

    /// Read one straightened line: its text and confidence.
    pub fn read_line(&mut self, line: &RgbImage) -> Result<(String, f32), AiError> {
        let (pixels, width) = rec_input(line)?;
        let input = Tensor::from_array(([1usize, 3, REC_HEIGHT as usize, width as usize], pixels))
            .map_err(ort_err)?;
        let outputs = self
            .recognizer
            .run(ort::inputs![self.rec_names.0.as_str() => input])
            .map_err(ort_err)?;
        let out = outputs
            .get(self.rec_names.1.as_str())
            .ok_or_else(|| AiError::Inference("the text recogniser gave nothing".into()))?;
        let (shape, probs) = out.try_extract_tensor::<f32>().map_err(ort_err)?;
        let dims: Vec<usize> = shape
            .iter()
            .map(|d| usize::try_from(*d).unwrap_or(0))
            .collect();
        let [_, steps, classes] = dims[..] else {
            return Err(AiError::Inference(format!(
                "the text recogniser's output has shape {dims:?}"
            )));
        };
        if classes != self.dict.len() + 1 {
            return Err(AiError::Inference(format!(
                "the text recogniser knows {classes} characters, its dictionary {}",
                self.dict.len() + 1
            )));
        }
        Ok(ctc_decode(probs, steps, classes, &self.dict))
    }

    /// Every line of text in `img` read with at least [`TEXT_MIN`]
    /// confidence, biggest letters first.
    pub fn read(&mut self, img: &DynamicImage) -> Result<Vec<TextLine>, AiError> {
        let rgb = img.to_rgb8();
        let mut out = Vec::new();
        for (quad, _) in self.find(&rgb)? {
            let Some(line) = crop(&rgb, &quad) else {
                continue;
            };
            let (text, score) = self.read_line(&line)?;
            if !text.is_empty() && score >= TEXT_MIN {
                out.push(TextLine { text, score, quad });
            }
        }
        out.sort_by(|a, b| b.height().total_cmp(&a.height()));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_detector_size_is_a_multiple_of_32_and_never_bigger() {
        assert_eq!(det_size(2560, 1707, 1600), (1600, 1056));
        assert_eq!(det_size(1024, 683, 1600), (1024, 672));
        assert_eq!(det_size(683, 1024, 960), (640, 960));
        assert_eq!(det_size(10, 10, 960), (32, 32));
    }

    #[test]
    fn the_detector_gets_blue_first_normalised() {
        let img = RgbImage::from_pixel(32, 32, Rgb([255, 0, 0]));
        let v = det_input(&img);
        let plane = 32 * 32;
        // Blue 0, green 0, red 255.
        assert!((v[0] - (0.0 - 0.485) / 0.229).abs() < 1e-5);
        assert!((v[plane] - (0.0 - 0.456) / 0.224).abs() < 1e-5);
        assert!((v[2 * plane] - (1.0 - 0.406) / 0.225).abs() < 1e-5);
    }

    fn map(w: usize, h: usize, on: impl Fn(usize, usize) -> bool) -> Vec<f32> {
        (0..w * h)
            .map(|i| if on(i % w, i / w) { 0.9 } else { 0.0 })
            .collect()
    }

    #[test]
    fn a_level_region_becomes_its_rectangle_grown() {
        // 40 × 10 at (20, 30).
        let prob = map(100, 80, |x, y| {
            (20..60).contains(&x) && (30..40).contains(&y)
        });
        let found = boxes(&prob, 100, 80);
        assert_eq!(found.len(), 1);
        let (r, score) = found[0];
        assert!((score - 0.9).abs() < 1e-4);
        assert!((r.c[0] - 40.0).abs() < 1e-3 && (r.c[1] - 35.0).abs() < 1e-3);
        assert!((r.u[0] - 1.0).abs() < 1e-4 && r.u[1].abs() < 1e-4);
        // Grown by 40·10·1.6 / (2·50) = 6.4 on each side.
        assert!((r.half_len - 26.4).abs() < 1e-3, "{r:?}");
        assert!((r.half_height - 11.4).abs() < 1e-3, "{r:?}");
        let q = r.quad();
        assert!(q[0][0] < q[1][0] && q[0][1] < q[3][1], "{q:?}");
    }

    #[test]
    fn a_slanted_region_keeps_its_angle() {
        // A band along y = x / 2, 6 thick.
        let prob = map(120, 80, |x, y| {
            let d = y as f32 - x as f32 / 2.0 - 5.0;
            (10..100).contains(&x) && d.abs() <= 3.0
        });
        let found = boxes(&prob, 120, 80);
        assert_eq!(found.len(), 1);
        let r = found[0].0;
        let angle = r.u[1].atan2(r.u[0]).to_degrees();
        assert!((angle - 26.57).abs() < 3.0, "{angle}");
        assert!(r.half_len > r.half_height * 3.0);
    }

    #[test]
    fn faint_and_tiny_regions_are_dropped_and_separate_ones_kept_apart() {
        let mut prob = map(100, 60, |x, y| {
            ((5..30).contains(&x) || (50..90).contains(&x)) && (10..20).contains(&y)
        });
        // A faint region and a speck.
        for x in 5..40 {
            for y in 40..50 {
                prob[y * 100 + x] = 0.35;
            }
        }
        prob[55 * 100 + 80] = 0.95;
        assert_eq!(boxes(&prob, 100, 60).len(), 2);
        assert!(boxes(&prob, 10, 10).is_empty(), "wrong size");
    }

    #[test]
    fn a_nearly_square_region_reads_level() {
        let r = min_rect(&hull(vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 12.0],
            [0.0, 12.0],
        ]))
        .unwrap();
        assert!((r.u[0] - 1.0).abs() < 1e-5, "{r:?}");
        // Much taller than wide: the text runs down.
        let r = min_rect(&hull(vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 40.0],
            [0.0, 40.0],
        ]))
        .unwrap();
        assert!((r.u[1] - 1.0).abs() < 1e-5, "{r:?}");
        assert!(min_rect(&[[0.0, 0.0], [1.0, 1.0]]).is_none());
    }

    #[test]
    fn a_crop_is_straightened() {
        // Left half red, right half blue; cut a level box across the join.
        let mut img = RgbImage::from_pixel(100, 50, Rgb([255, 0, 0]));
        for x in 50..100 {
            for y in 0..50 {
                img.put_pixel(x, y, Rgb([0, 0, 255]));
            }
        }
        let q = [[40.0, 10.0], [60.0, 10.0], [60.0, 20.0], [40.0, 20.0]];
        let c = crop(&img, &q).unwrap();
        assert_eq!(c.dimensions(), (20, 10));
        assert_eq!(c.get_pixel(2, 5).0, [255, 0, 0]);
        assert_eq!(c.get_pixel(17, 5).0, [0, 0, 255]);
        // The same box turned a quarter: its top runs down the picture.
        let q = [[60.0, 10.0], [60.0, 30.0], [40.0, 30.0], [40.0, 10.0]];
        let c = crop(&img, &q).unwrap();
        assert_eq!(c.dimensions(), (20, 20));
        assert_eq!(c.get_pixel(10, 2).0, [0, 0, 255]);
        assert_eq!(c.get_pixel(10, 17).0, [255, 0, 0]);
        assert!(crop(&img, &[[0.0, 0.0]; 4]).is_none());
        // A tall line comes out at most 96 high, in proportion.
        let big = RgbImage::new(1000, 600);
        let q = [[0.0, 0.0], [960.0, 0.0], [960.0, 480.0], [0.0, 480.0]];
        assert_eq!(crop(&big, &q).unwrap().dimensions(), (192, 96));
    }

    #[test]
    fn a_line_is_scaled_to_the_recogniser_and_padded() {
        let line = RgbImage::from_pixel(200, 50, Rgb([255, 255, 255]));
        let (v, w) = rec_input(&line).unwrap();
        assert_eq!(w, REC_MIN_WIDTH);
        let plane = (w * REC_HEIGHT) as usize;
        assert_eq!(v.len(), plane * 3);
        assert!((v[0] - 1.0).abs() < 1e-6, "white is 1");
        // 200 × 50 becomes 192 wide; the rest is padding.
        assert!((v[191] - 1.0).abs() < 1e-6);
        assert_eq!(v[192], 0.0);
        // A long line is not padded.
        let long = RgbImage::new(1000, 50);
        assert_eq!(rec_input(&long).unwrap().1, 960);
        assert!(rec_input(&RgbImage::new(0, 5)).is_err());
    }

    #[test]
    fn the_dictionary_ends_with_a_space() {
        let d = dictionary();
        assert_eq!(d.len(), 437);
        assert_eq!(&d[..3], ["0", "1", "2"]);
        assert_eq!(d[10], "A");
        assert_eq!(d.last(), Some(&" "));
    }

    #[test]
    fn decoding_drops_blanks_and_repeats() {
        let dict = ["A", "B", "-", " "];
        let classes = dict.len() + 1;
        // A A _ A B B _ - _ (space): "AAB- " → trimmed.
        let seq = [1usize, 1, 0, 1, 2, 2, 0, 3, 0, 4];
        let mut probs = vec![0.01f32; seq.len() * classes];
        for (t, c) in seq.iter().enumerate() {
            probs[t * classes + c] = 0.9;
        }
        let (text, score) = ctc_decode(&probs, seq.len(), classes, &dict);
        assert_eq!(text, "AAB-");
        assert!((score - 0.9).abs() < 1e-5);
        assert_eq!(ctc_decode(&[], 0, classes, &dict), (String::new(), 0.0));
    }
}
