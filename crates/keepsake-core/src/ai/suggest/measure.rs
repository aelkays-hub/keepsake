//! What Keepsake measures on a photo's thumbnail (ADR-042): how bright it
//! is, how much is blown out or pure black, its contrast, a colour cast,
//! noise, softness and a tilted horizon. Plain arithmetic on the pixels of
//! the 1024 px preview, on this computer; the original is not read.
//!
//! The preview is a small JPEG, so noise and sharpness are rough: fine
//! grain is averaged away when the photo is made smaller. The thresholds in
//! [`super::rules`] are set with that in mind.

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use crate::ai::AiError;

/// The smallest side that can be measured.
const MIN_SIDE: u32 = 48;
/// A pixel is blown out when every channel is at least this.
const BLOWN: u8 = 250;
/// A pixel is pure black when every channel is at most this.
const BLACK: u8 = 4;
/// Pixels less saturated than this (and neither dark nor bright) are taken
/// for grey things, to look for a colour cast.
const NEUTRAL_SAT: f32 = 0.2;
/// The share of the photo that must be near-grey to judge a cast.
const NEUTRAL_SHARE: f64 = 0.03;
/// Noise is measured in blocks of this many pixels a side.
const BLOCK: usize = 16;
/// How far from level a horizon is looked for (degrees).
const TILT_RANGE: f32 = 15.0;
/// The tilt histogram's bins per degree.
const TILT_BINS: f32 = 4.0;
/// An edge counts for the tilt when its gradient is at least this.
const TILT_EDGE: f32 = 60.0;
/// The share of the edges' weight within one degree of the commonest angle
/// for a tilt to be believed.
const TILT_SHARE: f32 = 0.4;

/// A colour cast, measured on the photo's near-grey areas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cast {
    /// Red against blue: above 0 warm (yellow), below 0 cool (blue).
    pub temp: f32,
    /// Green against red and blue: above 0 green, below 0 magenta.
    pub tint: f32,
}

/// What was measured. Tones are 0 (black) to 1 (white); shares are 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Measures {
    /// The size of what was measured (the thumbnail).
    pub width: u32,
    pub height: u32,
    /// The average tone.
    pub mean: f32,
    /// The middle tone: half the photo is darker.
    pub median: f32,
    /// The tone 5% of the photo is darker than.
    pub p5: f32,
    /// The tone 95% of the photo is darker than.
    pub p95: f32,
    /// The share of the photo that is blown out (white, no detail).
    pub highlights: f32,
    /// The share that is pure black.
    pub shadows: f32,
    /// How colourful it is on average (0 grey, 1 pure colours).
    pub saturation: f32,
    /// `None`: too little of the photo is near-grey to tell.
    pub cast: Option<Cast>,
    /// Grain in the smoothest areas, in levels of 255.
    pub noise: f32,
    /// How strong the strongest edges are (0 to 1; low is soft).
    pub sharpness: f32,
    /// A tilted horizon, in degrees: above 0 its right end is high (turn
    /// the photo clockwise to level it). `None`: level, or no clear line.
    pub tilt: Option<f32>,
}

/// The tone `q` (0 to 1) of the pixels are darker than.
fn percentile(hist: &[u64; 256], total: u64, q: f64) -> f32 {
    let want = (total as f64 * q).ceil() as u64;
    let mut seen = 0u64;
    for (level, n) in hist.iter().enumerate() {
        seen += n;
        if seen >= want {
            return level as f32 / 255.0;
        }
    }
    1.0
}

/// Measure a photo (its thumbnail).
pub fn measure(img: &DynamicImage) -> Result<Measures, AiError> {
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();
    if width < MIN_SIDE || height < MIN_SIDE {
        return Err(AiError::Image(
            "the preview is too small to measure".to_owned(),
        ));
    }
    let (w, h) = (width as usize, height as usize);
    let total = (w * h) as u64;
    let mut luma: Vec<f32> = Vec::with_capacity(w * h);
    let mut hist = [0u64; 256];
    let (mut blown, mut black) = (0u64, 0u64);
    let (mut tone_sum, mut sat_sum) = (0f64, 0f64);
    let mut grey = [0f64; 3];
    let mut greys = 0u64;
    for p in rgb.pixels() {
        let [r, g, b] = p.0;
        let y = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
        luma.push(y);
        hist[(y.round() as usize).min(255)] += 1;
        tone_sum += f64::from(y);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        blown += u64::from(min >= BLOWN);
        black += u64::from(max <= BLACK);
        let sat = if max == 0 {
            0.0
        } else {
            f32::from(max - min) / f32::from(max)
        };
        sat_sum += f64::from(sat);
        if sat < NEUTRAL_SAT && (40.0..=220.0).contains(&y) {
            grey[0] += f64::from(r);
            grey[1] += f64::from(g);
            grey[2] += f64::from(b);
            greys += 1;
        }
    }
    let n = total as f64;
    let cast = (greys as f64 >= n * NEUTRAL_SHARE).then(|| {
        let m = (grey[0] + grey[1] + grey[2]) / 3.0;
        Cast {
            temp: ((grey[0] - grey[2]) / m) as f32,
            tint: ((grey[1] - (grey[0] + grey[2]) / 2.0) / m) as f32,
        }
    });
    Ok(Measures {
        width,
        height,
        mean: (tone_sum / n / 255.0) as f32,
        median: percentile(&hist, total, 0.5),
        p5: percentile(&hist, total, 0.05),
        p95: percentile(&hist, total, 0.95),
        highlights: (blown as f64 / n) as f32,
        shadows: (black as f64 / n) as f32,
        saturation: (sat_sum / n) as f32,
        cast,
        noise: noise(&luma, w, h),
        sharpness: sharpness(&luma, w, h),
        tilt: tilt(&luma, w, h),
    })
}

/// Grain: Immerkær's estimate (the mean of a Laplacian-like mask that
/// cancels edges and gradients), taken block by block. Detail raises a
/// block's figure and grain is everywhere, so the figure of the smoother
/// blocks (the fifth lowest in a hundred... the 20th percentile) is the
/// grain. Blocks that are nearly black or white are left out: there is no
/// grain to see there.
fn noise(l: &[f32], w: usize, h: usize) -> f32 {
    let at = |x: usize, y: usize| l[y * w + x];
    let mut blocks: Vec<f32> = Vec::new();
    let mut y0 = 1;
    while y0 + BLOCK < h {
        let mut x0 = 1;
        while x0 + BLOCK < w {
            let (mut sum, mut tone) = (0f32, 0f32);
            for y in y0..y0 + BLOCK {
                for x in x0..x0 + BLOCK {
                    let v = at(x - 1, y - 1) - 2.0 * at(x, y - 1) + at(x + 1, y - 1)
                        - 2.0 * at(x - 1, y)
                        + 4.0 * at(x, y)
                        - 2.0 * at(x + 1, y)
                        + at(x - 1, y + 1)
                        - 2.0 * at(x, y + 1)
                        + at(x + 1, y + 1);
                    sum += v.abs();
                    tone += at(x, y);
                }
            }
            let count = (BLOCK * BLOCK) as f32;
            if (12.0..=243.0).contains(&(tone / count)) {
                blocks.push((std::f32::consts::FRAC_PI_2).sqrt() / 6.0 * sum / count);
            }
            x0 += BLOCK;
        }
        y0 += BLOCK;
    }
    if blocks.is_empty() {
        return 0.0;
    }
    blocks.sort_by(f32::total_cmp);
    blocks[blocks.len() / 5]
}

/// Sharpness: how strong the strongest 1% of edges are (the Laplacian of
/// each pixel against its four neighbours), 0 to 1. A soft photo has no
/// strong edges anywhere.
fn sharpness(l: &[f32], w: usize, h: usize) -> f32 {
    let mut hist = [0u64; 256];
    let mut total = 0u64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let v = (4.0 * l[i] - l[i - 1] - l[i + 1] - l[i - w] - l[i + w]).abs();
            hist[(v.round() as usize).min(255)] += 1;
            total += 1;
        }
    }
    percentile(&hist, total, 0.99)
}

/// A tilted horizon: the commonest angle of the strong, nearly level edges.
/// Only edges within [`TILT_RANGE`] of level count (uprights lean with
/// perspective, so they are no guide), and the angle is believed only when
/// much of the edges' weight is close to it.
fn tilt(l: &[f32], w: usize, h: usize) -> Option<f32> {
    // Half size: quicker, and less grain in the gradients.
    let (hw, hh) = (w / 2, h / 2);
    let mut small = vec![0f32; hw * hh];
    for y in 0..hh {
        for x in 0..hw {
            let i = 2 * y * w + 2 * x;
            small[y * hw + x] = (l[i] + l[i + 1] + l[i + w] + l[i + w + 1]) / 4.0;
        }
    }
    let at = |x: usize, y: usize| small[y * hw + x];
    let bins = (2.0 * TILT_RANGE * TILT_BINS) as usize + 1;
    let mut hist = vec![0f32; bins];
    let mut edges = 0u64;
    for y in 1..hh.saturating_sub(1) {
        for x in 1..hw.saturating_sub(1) {
            let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x, y - 1)
                - at(x + 1, y - 1);
            let mag = gx.hypot(gy);
            if mag < TILT_EDGE {
                continue;
            }
            // The gradient of a level edge points straight up or down
            // (90°). `off` is how far this one is from that.
            let angle = gy.atan2(gx).to_degrees().rem_euclid(180.0);
            let off = angle - 90.0;
            if off.abs() > TILT_RANGE {
                continue;
            }
            let bin = ((off + TILT_RANGE) * TILT_BINS).round() as usize;
            hist[bin.min(bins - 1)] += mag;
            edges += 1;
        }
    }
    // Too few level edges to speak of a horizon.
    if (edges as f64) < (hw * hh) as f64 * 0.004 {
        return None;
    }
    let total: f32 = hist.iter().sum();
    let near = |i: usize, reach: usize| -> f32 {
        hist[i.saturating_sub(reach)..=(i + reach).min(bins - 1)]
            .iter()
            .sum()
    };
    let (peak, _) = (0..bins)
        .map(|i| (i, near(i, 1)))
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let reach = TILT_BINS as usize;
    if near(peak, reach) < total * TILT_SHARE {
        return None;
    }
    // The weighted mean angle around the peak.
    let (mut sum, mut weight) = (0f32, 0f32);
    for (i, v) in hist
        .iter()
        .enumerate()
        .take((peak + reach).min(bins - 1) + 1)
        .skip(peak.saturating_sub(reach))
    {
        sum += (i as f32 / TILT_BINS - TILT_RANGE) * v;
        weight += v;
    }
    let off = sum / weight;
    // With y downwards, a horizon whose right end is high has its gradient
    // short of 90°: `off` below 0.
    let deg = (-off * 10.0).round() / 10.0;
    (deg.abs() >= 0.7 && deg.abs() <= 10.0).then_some(deg)
}

#[cfg(test)]
mod tests {
    use image::{Rgb, RgbImage};

    use super::*;

    fn photo(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| Rgb(f(x, y))))
    }

    /// A cheap, repeatable "random" number for pixel (x, y): 0 to 1.
    fn rnd(x: u32, y: u32) -> f32 {
        let mut v = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
        v = (v ^ (v >> 13)).wrapping_mul(1_274_126_177);
        ((v ^ (v >> 16)) & 0xffff) as f32 / 65535.0
    }

    #[test]
    fn tones_and_clipping_are_shares_of_the_photo() {
        // Left quarter white, right quarter black, the middle mid-grey.
        let m = measure(&photo(400, 200, |x, _| match x {
            0..=99 => [255; 3],
            300.. => [0; 3],
            _ => [128; 3],
        }))
        .unwrap();
        assert!((m.highlights - 0.25).abs() < 0.001, "{m:?}");
        assert!((m.shadows - 0.25).abs() < 0.001, "{m:?}");
        assert!((m.median - 128.0 / 255.0).abs() < 0.01, "{m:?}");
        assert_eq!((m.p5, m.p95), (0.0, 1.0));
        assert_eq!(m.saturation, 0.0);
        assert_eq!((m.width, m.height), (400, 200));
    }

    #[test]
    fn a_warm_grey_wall_has_a_warm_cast_and_colours_do_not() {
        let warm = measure(&photo(200, 200, |_, _| [150, 138, 126])).unwrap();
        let cast = warm.cast.unwrap();
        assert!(cast.temp > 0.15 && cast.tint.abs() < 0.02, "{cast:?}");
        let cool = measure(&photo(200, 200, |_, _| [126, 138, 150])).unwrap();
        assert!(cool.cast.unwrap().temp < -0.15);
        let green = measure(&photo(200, 200, |_, _| [125, 145, 125])).unwrap();
        assert!(green.cast.unwrap().tint > 0.1);
        // A red wall is a red wall, not a cast.
        let red = measure(&photo(200, 200, |_, _| [200, 40, 40])).unwrap();
        assert_eq!(red.cast, None);
        assert!(red.saturation > 0.7);
    }

    #[test]
    fn grain_is_measured_and_a_smooth_photo_has_none() {
        let smooth = measure(&photo(320, 240, |x, y| [(60 + (x + y) / 6) as u8; 3])).unwrap();
        assert!(smooth.noise < 0.3, "{smooth:?}");
        // The same slope with grain of about ±10 levels.
        let grainy = measure(&photo(320, 240, |x, y| {
            let v = 60.0 + (x + y) as f32 / 6.0 + (rnd(x, y) - 0.5) * 20.0;
            [v as u8; 3]
        }))
        .unwrap();
        assert!(grainy.noise > 3.0, "{grainy:?}");
    }

    #[test]
    fn hard_edges_are_sharp_and_slow_ones_soft() {
        let sharp = measure(&photo(320, 240, |x, y| {
            [if (x / 8 + y / 8) % 2 == 0 { 40 } else { 210 }; 3]
        }))
        .unwrap();
        // The same change spread over 40 pixels.
        let soft = measure(&photo(320, 240, |x, _| {
            let t = (x % 80) as f32 / 40.0;
            [(40.0 + 170.0 * if t > 1.0 { 2.0 - t } else { t }) as u8; 3]
        }))
        .unwrap();
        assert!(sharp.sharpness > 0.5, "{sharp:?}");
        assert!(soft.sharpness < 0.05, "{soft:?}");
    }

    /// Sky over sea, the horizon's right end `deg` degrees high.
    fn horizon(deg: f32) -> DynamicImage {
        let slope = deg.to_radians().tan();
        photo(640, 480, move |x, y| {
            // How far below the line this pixel is, softened over a pixel.
            let line = 240.0 - (x as f32 - 320.0) * slope;
            let t = (y as f32 - line + 0.5).clamp(0.0, 1.0);
            let v = 200.0 - 140.0 * t;
            [v as u8, v as u8, (v + 30.0) as u8]
        })
    }

    #[test]
    fn a_tilted_horizon_is_found_with_its_direction() {
        let right_high = measure(&horizon(3.0)).unwrap().tilt.unwrap();
        assert!((right_high - 3.0).abs() < 0.5, "{right_high}");
        let left_high = measure(&horizon(-2.0)).unwrap().tilt.unwrap();
        assert!((left_high + 2.0).abs() < 0.5, "{left_high}");
        assert_eq!(measure(&horizon(0.0)).unwrap().tilt, None, "level");
        // No lines at all: nothing to say.
        let flat = measure(&photo(320, 240, |_, _| [120; 3])).unwrap();
        assert_eq!(flat.tilt, None);
    }

    #[test]
    fn a_tiny_picture_is_refused() {
        let e = measure(&photo(20, 20, |_, _| [9; 3])).unwrap_err();
        assert!(e.to_string().contains("too small"), "{e}");
    }
}
