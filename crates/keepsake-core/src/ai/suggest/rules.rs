//! From measurements to edit suggestions (ADR-042), by fixed rules: each
//! says what was found and how to fix it in Photoshop and in Topaz Photo,
//! naming the tool and a value to start from. The rules cannot see what the
//! photo is of (a night scene is meant to be dark), so each says what it
//! measured and leaves the choice to the user.

use super::measure::Measures;
use super::Suggestion;

/// This much of the photo blown out is worth recovering.
const HIGHLIGHTS: f32 = 0.01;
/// This much of the photo pure black is worth opening up.
const SHADOWS: f32 = 0.03;
/// A middle tone below this is dark, above [`BRIGHT`] bright.
const DARK: f32 = 0.25;
const BRIGHT: f32 = 0.72;
/// The middle tone an exposure change aims for.
const AIM: f32 = 0.42;
/// A spread from the darkest 5% to the brightest 5% below this is flat.
const FLAT: f32 = 0.4;
/// A cast this strong on the near-grey areas shows.
const CAST_TEMP: f32 = 0.1;
const CAST_TINT: f32 = 0.06;
/// Grain (levels, on the preview) that shows, and that is strong.
const NOISE: f32 = 2.5;
const NOISE_STRONG: f32 = 5.0;
/// Edges weaker than this: the photo is soft.
const SOFT: f32 = 0.08;
/// Average saturation below this is dull; below [`MONO`] it is black and
/// white, on purpose.
const DULL: f32 = 0.08;
const MONO: f32 = 0.02;

/// One line of what was measured, for people: "Highlights", "3% blown out".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measured {
    pub label: &'static str,
    pub value: String,
}

fn pct(share: f32) -> String {
    let p = share * 100.0;
    if p >= 10.0 {
        format!("{p:.0}%")
    } else if p >= 0.1 {
        format!("{p:.1}%")
    } else if share > 0.0 {
        "under 0.1%".to_owned()
    } else {
        "0%".to_owned()
    }
}

fn is_dark(m: &Measures) -> bool {
    m.median < DARK && m.p95 < 0.8 && m.highlights < HIGHLIGHTS / 2.0
}

fn is_bright(m: &Measures) -> bool {
    m.median > BRIGHT && m.p5 > 0.25 && m.shadows < SHADOWS / 2.0
}

/// The cast in words ("warm (yellow)", "cool (blue) and green"), if it shows.
fn cast_words(m: &Measures) -> Option<String> {
    let c = m.cast?;
    let mut parts: Vec<&str> = Vec::new();
    if c.temp >= CAST_TEMP {
        parts.push("warm (yellow)");
    } else if c.temp <= -CAST_TEMP {
        parts.push("cool (blue)");
    }
    if c.tint >= CAST_TINT {
        parts.push("green");
    } else if c.tint <= -CAST_TINT {
        parts.push("magenta");
    }
    (!parts.is_empty()).then(|| parts.join(" and "))
}

/// What was measured, a line each, in the order shown.
pub fn describe(m: &Measures) -> Vec<Measured> {
    let line = |label, value: String| Measured { label, value };
    let tone = format!("middle tone {:.0}%", m.median * 100.0);
    vec![
        line(
            "Brightness",
            if is_dark(m) {
                format!("Dark ({tone})")
            } else if is_bright(m) {
                format!("Bright ({tone})")
            } else {
                format!("Fine ({tone})")
            },
        ),
        line("Highlights", format!("{} blown out", pct(m.highlights))),
        line("Shadows", format!("{} pure black", pct(m.shadows))),
        line(
            "Contrast",
            if m.p95 - m.p5 < FLAT {
                "Low".to_owned()
            } else {
                "Fine".to_owned()
            },
        ),
        line(
            "Colour",
            match (cast_words(m), m.cast) {
                (Some(words), _) => format!("Looks {words}"),
                (None, Some(_)) => "Greys look neutral".to_owned(),
                (None, None) => "Could not tell (nothing grey to judge by)".to_owned(),
            },
        ),
        line(
            "Noise",
            if m.noise >= NOISE_STRONG {
                "Strong".to_owned()
            } else if m.noise >= NOISE {
                "Shows".to_owned()
            } else {
                "Low".to_owned()
            },
        ),
        line(
            "Sharpness",
            if m.sharpness < SOFT {
                "Soft".to_owned()
            } else {
                "Fine".to_owned()
            },
        ),
        line(
            "Level",
            match m.tilt {
                Some(t) => format!(
                    "Tilted about {:.1}°, {} end high",
                    t.abs(),
                    if t > 0.0 { "right" } else { "left" }
                ),
                None => "No tilt found".to_owned(),
            },
        ),
    ]
}

fn suggestion(title: &str, why: String, photoshop: String, topaz: &str) -> Suggestion {
    Suggestion {
        title: title.to_owned(),
        why,
        photoshop,
        topaz: topaz.to_owned(),
    }
}

/// The exposure change, in stops, that brings the middle tone towards
/// [`AIM`]: a value to start from, not a correction to trust.
fn stops(median: f32) -> f32 {
    let s = 1.5 * (AIM / median.max(0.03)).log2();
    let s = s.clamp(-2.0, 2.0);
    (s * 10.0).round() / 10.0
}

/// The suggestions for what was measured, most important first. Empty:
/// nothing stands out.
pub fn suggest(m: &Measures) -> Vec<Suggestion> {
    let mut out = Vec::new();
    if is_dark(m) {
        out.push(suggestion(
            "Brighten the photo",
            format!(
                "Half of it is darker than {:.0}% grey and nothing is blown out, so there is room to lift it (unless it is meant to be dark).",
                m.median * 100.0
            ),
            format!(
                "Camera Raw > Light: Exposure {:+.1}, then Shadows +20 if the dark parts are still heavy.",
                stops(m.median).max(0.3)
            ),
            "Adjust Lighting: start at Strength 30 and raise it until the subject looks right.",
        ));
    } else if is_bright(m) {
        out.push(suggestion(
            "Darken the photo",
            format!(
                "Half of it is brighter than {:.0}% grey and it has no deep shadows (unless it is meant to be light and airy).",
                m.median * 100.0
            ),
            format!(
                "Camera Raw > Light: Exposure {:+.1}, then Blacks -10 to give it a dark point.",
                stops(m.median).min(-0.3)
            ),
            "Adjust Lighting: Topaz mostly brightens; do this one in Photoshop.",
        ));
    }
    if m.highlights >= HIGHLIGHTS {
        out.push(suggestion(
            "Recover the highlights",
            format!(
                "{} of the photo is blown out to white with no detail.",
                pct(m.highlights)
            ),
            "Camera Raw > Light: Highlights -60, Whites -15. A raw file has more to give back than a JPEG; what is truly white stays white.".to_owned(),
            "",
        ));
    }
    if m.shadows >= SHADOWS {
        out.push(suggestion(
            "Open up the shadows",
            format!("{} of the photo is pure black.", pct(m.shadows)),
            "Camera Raw > Light: Shadows +40, Blacks +10. Lifted shadows show noise, so check them at 100%.".to_owned(),
            "Adjust Lighting: start at Strength 30; add Denoise if the lifted shadows are grainy.",
        ));
    }
    if m.p95 - m.p5 < FLAT {
        out.push(suggestion(
            "Add contrast",
            format!(
                "Its tones only run from {:.0}% to {:.0}% grey, so it looks flat or hazy.",
                m.p5 * 100.0,
                m.p95 * 100.0
            ),
            "Camera Raw > Light: Contrast +20, then Effects: Dehaze +10. Or Image > Adjustments > Levels: drag the black and white sliders in to where the histogram starts and ends.".to_owned(),
            "Adjust Lighting: start at Strength 30.",
        ));
    }
    if let (Some(words), Some(c)) = (cast_words(m), m.cast) {
        let mut ps: Vec<&str> = Vec::new();
        if c.temp >= CAST_TEMP {
            ps.push("move Temperature towards blue");
        } else if c.temp <= -CAST_TEMP {
            ps.push("move Temperature towards yellow");
        }
        if c.tint >= CAST_TINT {
            ps.push("move Tint towards magenta");
        } else if c.tint <= -CAST_TINT {
            ps.push("move Tint towards green");
        }
        out.push(suggestion(
            "Correct the colour cast",
            format!(
                "Things that should be grey look {words} (unless that is the light you wanted, such as a sunset)."
            ),
            format!(
                "Camera Raw > Color: pick the White Balance tool (I) and click something that should be grey or white; or {}.",
                ps.join(" and ")
            ),
            "Balance Color: let it set the white balance, then fine-tune Temperature and Tint.",
        ));
    }
    if m.noise >= NOISE {
        let strong = m.noise >= NOISE_STRONG;
        out.push(suggestion(
            "Reduce the noise",
            format!(
                "The smooth areas are grainy even in the small preview{}; at full size it will show more.",
                if strong { ", strongly" } else { "" }
            ),
            format!(
                "Camera Raw > Detail: Denoise (for a raw file) at {}, or Manual Noise Reduction: Luminance {}, Color 25.",
                if strong { 60 } else { 40 },
                if strong { 40 } else { 25 }
            ),
            if strong {
                "Denoise: the Strong model for a raw file, else Standard; lower Strength if skin or feathers turn waxy."
            } else {
                "Denoise: the Standard model on Auto; lower Strength if fine detail turns waxy."
            },
        ));
    }
    if m.sharpness < SOFT {
        out.push(suggestion(
            "Sharpen it",
            "No edge in the preview is crisp: the photo is soft all over (missed focus, movement or haze)."
                .to_owned(),
            "Filter > Sharpen > Smart Sharpen: Amount 120%, Radius 1.0 px, Reduce Noise 10%, Remove: Lens Blur (or Motion Blur with the angle of the movement).".to_owned(),
            "Sharpen: Standard on Auto; Lens Blur for missed focus, Motion Blur for movement. Super Focus if it is badly out of focus.",
        ));
    }
    if let Some(t) = m.tilt {
        let turn = if t > 0.0 {
            "clockwise"
        } else {
            "counter-clockwise"
        };
        out.push(suggestion(
            "Straighten it",
            format!(
                "The strongest level line (a horizon, a roof, a shore) is about {:.1}° off, its {} end high.",
                t.abs(),
                if t > 0.0 { "right" } else { "left" }
            ),
            format!(
                "Crop tool (C) > Straighten: drag along the line that should be level (about {:.1}° {turn}). Or Camera Raw > Geometry: Level.",
                t.abs()
            ),
            "",
        ));
    }
    if m.saturation < DULL && m.saturation >= MONO {
        out.push(suggestion(
            "Lift the colour",
            "Its colours are faint on average.".to_owned(),
            "Camera Raw > Color: Vibrance +20 (it spares skin and colours that are already strong).".to_owned(),
            "",
        ));
    }
    out
}

/// One sentence on what the rules found.
pub fn summary(found: usize) -> String {
    match found {
        0 => {
            "Nothing stands out: exposure, contrast, colour, noise and sharpness all measure fine."
                .to_owned()
        }
        1 => "Keepsake measured the photo and found one thing to look at.".to_owned(),
        n => format!("Keepsake measured the photo and found {n} things to look at."),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::measure::Cast;
    use super::*;

    /// A photo with nothing to fix.
    pub(crate) fn fine() -> Measures {
        Measures {
            width: 1024,
            height: 683,
            mean: 0.45,
            median: 0.44,
            p5: 0.08,
            p95: 0.9,
            highlights: 0.0,
            shadows: 0.001,
            saturation: 0.3,
            cast: Some(Cast {
                temp: 0.02,
                tint: 0.0,
            }),
            noise: 0.8,
            sharpness: 0.3,
            tilt: None,
        }
    }

    fn titles(m: &Measures) -> Vec<String> {
        suggest(m).into_iter().map(|s| s.title).collect()
    }

    #[test]
    fn a_fine_photo_gets_no_suggestions() {
        assert!(suggest(&fine()).is_empty());
        assert!(summary(0).starts_with("Nothing stands out"));
        let lines = describe(&fine());
        assert_eq!(lines[0].value, "Fine (middle tone 44%)");
        assert_eq!(lines[1].value, "0% blown out");
        assert_eq!(lines[4].value, "Greys look neutral");
        assert_eq!(lines[7].value, "No tilt found");
    }

    #[test]
    fn a_dark_photo_is_brightened_by_a_stated_amount() {
        let m = Measures {
            median: 0.15,
            mean: 0.17,
            p95: 0.55,
            ..fine()
        };
        let s = suggest(&m);
        assert_eq!(s[0].title, "Brighten the photo");
        assert!(
            s[0].photoshop.contains("Exposure +2.0"),
            "{}",
            s[0].photoshop
        );
        assert!(s[0].topaz.starts_with("Adjust Lighting"));
        assert_eq!(describe(&m)[0].value, "Dark (middle tone 15%)");
        // Dark with blown lights (a night street) is not simply lifted.
        let night = Measures {
            highlights: 0.02,
            ..m
        };
        assert_eq!(titles(&night), ["Recover the highlights"]);
    }

    #[test]
    fn a_bright_photo_is_darkened() {
        let m = Measures {
            median: 0.8,
            p5: 0.4,
            shadows: 0.0,
            ..fine()
        };
        let s = suggest(&m);
        assert_eq!(s[0].title, "Darken the photo");
        assert!(
            s[0].photoshop.contains("Exposure -1.4"),
            "{}",
            s[0].photoshop
        );
    }

    #[test]
    fn clipping_contrast_noise_softness_and_colour_each_have_a_rule() {
        let m = Measures {
            highlights: 0.034,
            shadows: 0.12,
            p5: 0.3,
            p95: 0.6,
            noise: 6.0,
            sharpness: 0.03,
            saturation: 0.05,
            ..fine()
        };
        assert_eq!(
            titles(&m),
            [
                "Recover the highlights",
                "Open up the shadows",
                "Add contrast",
                "Reduce the noise",
                "Sharpen it",
                "Lift the colour",
            ]
        );
        let s = suggest(&m);
        assert!(s[0].why.starts_with("3.4% of the photo"), "{}", s[0].why);
        assert_eq!(s[0].topaz, "", "Topaz cannot bring back blown highlights");
        assert!(s[1].why.starts_with("12% of the photo"));
        assert!(s[3].topaz.contains("Strong"));
        assert!(s[4].topaz.contains("Super Focus"));
        let lines = describe(&m);
        assert_eq!(lines[3].value, "Low");
        assert_eq!(lines[5].value, "Strong");
        assert_eq!(lines[6].value, "Soft");
        // Black and white is left alone.
        let mono = Measures {
            saturation: 0.0,
            ..fine()
        };
        assert!(suggest(&mono).is_empty());
    }

    #[test]
    fn a_cast_says_which_way_to_move_the_sliders() {
        let m = Measures {
            cast: Some(Cast {
                temp: 0.2,
                tint: -0.08,
            }),
            ..fine()
        };
        let s = suggest(&m);
        assert_eq!(s[0].title, "Correct the colour cast");
        assert!(
            s[0].why.contains("warm (yellow) and magenta"),
            "{}",
            s[0].why
        );
        assert!(s[0]
            .photoshop
            .contains("move Temperature towards blue and move Tint towards green"));
        assert!(s[0].topaz.starts_with("Balance Color"));
        let unknown = Measures {
            cast: None,
            ..fine()
        };
        assert!(suggest(&unknown).is_empty());
        assert!(describe(&unknown)[4].value.starts_with("Could not tell"));
    }

    #[test]
    fn a_tilt_says_which_way_to_turn() {
        let m = Measures {
            tilt: Some(2.3),
            ..fine()
        };
        let s = suggest(&m);
        assert_eq!(s[0].title, "Straighten it");
        assert!(s[0].photoshop.contains("about 2.3° clockwise"));
        assert_eq!(describe(&m)[7].value, "Tilted about 2.3°, right end high");
        let other = Measures {
            tilt: Some(-1.0),
            ..fine()
        };
        assert!(suggest(&other)[0]
            .photoshop
            .contains("1.0° counter-clockwise"));
        assert_eq!(
            summary(1),
            "Keepsake measured the photo and found one thing to look at."
        );
        assert!(summary(3).contains("3 things"));
    }
}
