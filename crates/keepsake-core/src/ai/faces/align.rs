//! Straightening a face for the recogniser (ADR-040): the face's 5
//! landmarks are moved onto the recogniser's template with the closest
//! similarity transform (rotation, one scale, shift; no mirroring), and
//! the 112 × 112 face is read out of the picture through it.
//!
//! This is what OpenCV's `FaceRecognizerSF::alignCrop` does (Umeyama's
//! method; for 2-D points without mirroring it has the closed form used
//! here), sampled bilinearly with black outside the picture.

use image::{Rgb, RgbImage};

/// The aligned face's size.
pub const SIZE: u32 = 112;

/// Where the 5 landmarks go in the 112 × 112 face: right eye, left eye,
/// nose tip, right and left mouth corners (as the person sees them; the
/// first is on the left of the picture).
pub const TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

/// `x' = a·x − b·y + tx`, `y' = b·x + a·y + ty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Similarity {
    pub a: f32,
    pub b: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Similarity {
    pub fn apply(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        [
            self.a * x - self.b * y + self.tx,
            self.b * x + self.a * y + self.ty,
        ]
    }

    /// Where `p` (in the result) comes from (in the source). `None` if the
    /// transform squashes everything to a point.
    pub fn invert(&self, [u, v]: [f32; 2]) -> Option<[f32; 2]> {
        let det = self.a * self.a + self.b * self.b;
        if det <= f32::EPSILON {
            return None;
        }
        let (du, dv) = (u - self.tx, v - self.ty);
        Some([
            (self.a * du + self.b * dv) / det,
            (-self.b * du + self.a * dv) / det,
        ])
    }

    /// The scale: how much bigger the result is than the source.
    pub fn scale(&self) -> f32 {
        (self.a * self.a + self.b * self.b).sqrt()
    }
}

fn mean(p: &[[f32; 2]]) -> [f32; 2] {
    let n = p.len().max(1) as f32;
    let (x, y) = p.iter().fold((0.0, 0.0), |(x, y), q| (x + q[0], y + q[1]));
    [x / n, y / n]
}

/// The similarity transform taking `src` closest to `dst` (least squares).
/// `None` when the source points are all in one place.
pub fn similarity(src: &[[f32; 2]], dst: &[[f32; 2]]) -> Option<Similarity> {
    if src.len() != dst.len() || src.is_empty() {
        return None;
    }
    let (ms, md) = (mean(src), mean(dst));
    let (mut sxx, mut sxy, mut var) = (0.0f64, 0.0f64, 0.0f64);
    for (s, d) in src.iter().zip(dst) {
        let (px, py) = (f64::from(s[0] - ms[0]), f64::from(s[1] - ms[1]));
        let (qx, qy) = (f64::from(d[0] - md[0]), f64::from(d[1] - md[1]));
        sxx += px * qx + py * qy;
        sxy += px * qy - py * qx;
        var += px * px + py * py;
    }
    if var <= f64::from(f32::EPSILON) {
        return None;
    }
    let (a, b) = ((sxx / var) as f32, (sxy / var) as f32);
    Some(Similarity {
        a,
        b,
        tx: md[0] - (a * ms[0] - b * ms[1]),
        ty: md[1] - (b * ms[0] + a * ms[1]),
    })
}

/// The colour at `(x, y)`, between pixels by bilinear interpolation (pixel
/// `i` is at `i`); black outside the picture.
fn sample(img: &RgbImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let px = |xi: i64, yi: i64| -> [f32; 3] {
        if xi < 0 || yi < 0 || xi >= w || yi >= h {
            [0.0; 3]
        } else {
            let p = img.get_pixel(xi as u32, yi as u32).0;
            [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]
        }
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

/// The `size` × `size` picture `t` makes of `img`.
pub fn warp(img: &RgbImage, t: &Similarity, size: u32) -> Option<RgbImage> {
    let mut out = RgbImage::new(size, size);
    for v in 0..size {
        for u in 0..size {
            let [x, y] = t.invert([u as f32, v as f32])?;
            let c = sample(img, x, y);
            out.put_pixel(u, v, Rgb(c.map(|c| c.round().clamp(0.0, 255.0) as u8)));
        }
    }
    Some(out)
}

/// The face whose landmarks (in `img`'s pixels) are `landmarks`,
/// straightened to [`SIZE`] × [`SIZE`]. `None` if the landmarks are all in
/// one place.
pub fn align(img: &RgbImage, landmarks: &[[f32; 2]; 5]) -> Option<RgbImage> {
    let t = similarity(landmarks, &TEMPLATE)?;
    warp(img, &t, SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn the_template_maps_onto_itself() {
        let t = similarity(&TEMPLATE, &TEMPLATE).unwrap();
        assert!((t.a - 1.0).abs() < 1e-5 && t.b.abs() < 1e-5, "{t:?}");
        assert!(t.tx.abs() < 1e-3 && t.ty.abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn a_turned_scaled_moved_face_is_put_back() {
        // The template, turned 30°, made 3 times bigger and moved: as a
        // tilted face in a photo would be.
        let (s, th) = (3.0f32, 30f32.to_radians());
        let src: Vec<[f32; 2]> = TEMPLATE
            .iter()
            .map(|&[x, y]| {
                [
                    s * (th.cos() * x - th.sin() * y) + 400.0,
                    s * (th.sin() * x + th.cos() * y) + 250.0,
                ]
            })
            .collect();
        let t = similarity(&src, &TEMPLATE).unwrap();
        for (p, q) in src.iter().zip(TEMPLATE) {
            assert!(close(t.apply(*p), q), "{:?} → {:?}", p, t.apply(*p));
            assert!(close(t.invert(q).unwrap(), *p));
        }
        assert!((t.scale() - 1.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn a_mirrored_face_is_not_mirrored_back() {
        // Left and right swapped: the closest similarity cannot mirror, so
        // the points do not all land on the template.
        let src: Vec<[f32; 2]> = TEMPLATE.iter().map(|&[x, y]| [112.0 - x, y]).collect();
        let t = similarity(&src, &TEMPLATE).unwrap();
        assert!(t.a > 0.0, "no reflection: {t:?}");
        assert!(!src.iter().zip(TEMPLATE).all(|(p, q)| close(t.apply(*p), q)));
    }

    #[test]
    fn points_in_one_place_give_nothing() {
        assert_eq!(similarity(&[[5.0, 5.0]; 5], &TEMPLATE), None);
        assert_eq!(similarity(&[], &[]), None);
    }

    #[test]
    fn the_face_is_read_out_where_its_landmarks_are() {
        // A grey picture with a white square on the "nose tip" at
        // (300, 200): after aligning, the white is at the template's nose.
        let mut img = RgbImage::from_pixel(600, 400, Rgb([60, 60, 60]));
        for y in 195..=205 {
            for x in 295..=305 {
                img.put_pixel(x, y, Rgb([255, 255, 255]));
            }
        }
        let s = 2.0;
        let nose = TEMPLATE[2];
        let lm: [[f32; 2]; 5] =
            TEMPLATE.map(|[x, y]| [300.0 + s * (x - nose[0]), 200.0 + s * (y - nose[1])]);
        let face = align(&img, &lm).unwrap();
        assert_eq!(face.dimensions(), (SIZE, SIZE));
        let at = |x: f32, y: f32| face.get_pixel(x.round() as u32, y.round() as u32).0;
        assert_eq!(at(nose[0], nose[1]), [255, 255, 255]);
        assert_eq!(at(10.0, 10.0), [60, 60, 60]);
    }

    #[test]
    fn outside_the_picture_is_black() {
        let img = RgbImage::from_pixel(20, 20, Rgb([200, 100, 50]));
        // Landmarks near the corner: much of the face is off the picture.
        let lm = TEMPLATE.map(|[x, y]| [x / 10.0 - 2.0, y / 10.0 - 2.0]);
        let face = align(&img, &lm).unwrap();
        assert_eq!(face.get_pixel(0, 0).0, [0, 0, 0]);
    }
}
