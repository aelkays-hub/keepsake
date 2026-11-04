//! Resolve one capture time per asset from all available evidence.
//!
//! Precedence (ARCHITECTURE.md, "Capture Date Precedence"):
//! 1. Sony XML `CreationDate` (carries the UTC offset),
//! 2. EXIF `DateTimeOriginal` + `SubSecTimeOriginal` + `OffsetTimeOriginal`,
//! 3. AVCHD in-stream MDPM date,
//! 4. container atoms (`mvhd` / `©day`, UTC),
//! 5. never file-system dates.
//!
//! Confidence:
//! - **high**: the winning source gives both the local time and its offset,
//!   and no other source disagrees by more than [`CONFLICT_MS`];
//! - **medium**: the offset was borrowed from a nearby asset of the same
//!   camera, or a high result was contradicted by another source;
//! - **low**: the offset is unknown. `utc_ms` then holds the best clock
//!   reading available (the camera's local clock for EXIF/MDPM, UTC for
//!   container times) and `utc_offset_min` is `None`.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::catalog::CaptureTime;
pub use crate::catalog::{CaptureSource, Confidence};

/// Sources disagreeing by more than this lower the confidence.
pub const CONFLICT_MS: i64 = 2_000;
/// Borrow an offset only from assets captured within this window.
pub const OFFSET_BORROW_WINDOW_MS: i64 = 12 * 3_600_000;

/// A camera-local clock reading with its offset when known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTime {
    /// Local wall-clock time as Unix milliseconds *as if it were UTC*.
    pub local_ms: i64,
    pub offset_min: Option<i16>,
}

impl LocalTime {
    fn instant(self) -> Option<i64> {
        self.offset_min
            .map(|o| self.local_ms - i64::from(o) * 60_000)
    }

    fn to_capture(self) -> CaptureTime {
        match self.offset_min {
            Some(o) => CaptureTime {
                utc_ms: self.local_ms - i64::from(o) * 60_000,
                utc_offset_min: Some(o),
            },
            None => CaptureTime {
                utc_ms: self.local_ms,
                utc_offset_min: None,
            },
        }
    }
}

/// All capture-time evidence for one asset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    pub sony_xml: Option<CaptureTime>,
    pub exif: Option<LocalTime>,
    pub mdpm: Option<LocalTime>,
    pub container_utc_ms: Option<i64>,
}

/// The resolved time and how it was obtained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolved {
    pub time: CaptureTime,
    pub source: CaptureSource,
    pub confidence: Confidence,
    /// What a human should know (conflicts, borrowed offsets).
    pub notes: Vec<String>,
}

impl Resolved {
    /// True when the offset is unknown and could be borrowed.
    pub fn needs_offset(&self) -> bool {
        self.time.utc_offset_min.is_none()
    }
}

/// Resolve one asset's evidence (no cross-asset inference).
pub fn resolve(ev: &Evidence) -> Option<Resolved> {
    let (time, source) = if let Some(t) = ev.sony_xml {
        (t, CaptureSource::SonyXml)
    } else if let Some(l) = ev.exif {
        (l.to_capture(), CaptureSource::Exif)
    } else if let Some(l) = ev.mdpm {
        (l.to_capture(), CaptureSource::AvchdMdpm)
    } else {
        let u = ev.container_utc_ms?;
        (
            CaptureTime {
                utc_ms: u,
                utc_offset_min: None,
            },
            CaptureSource::Container,
        )
    };
    let mut r = Resolved {
        time,
        source,
        confidence: if time.utc_offset_min.is_some() {
            Confidence::High
        } else {
            Confidence::Low
        },
        notes: Vec::new(),
    };

    // Cross-check every source that pins down a real instant.
    let instants = [
        (
            CaptureSource::SonyXml,
            ev.sony_xml.and_then(|t| t.utc_offset_min.map(|_| t.utc_ms)),
        ),
        (CaptureSource::Exif, ev.exif.and_then(LocalTime::instant)),
        (
            CaptureSource::AvchdMdpm,
            ev.mdpm.and_then(LocalTime::instant),
        ),
        (CaptureSource::Container, ev.container_utc_ms),
    ];
    if let Some(chosen) = r.time.utc_offset_min.map(|_| r.time.utc_ms) {
        for (src, t) in instants {
            if let Some(t) = t {
                if src != source && (t - chosen).abs() > CONFLICT_MS {
                    r.notes.push(format!(
                        "{} differs from {} by {} s",
                        src.as_str(),
                        source.as_str(),
                        (t - chosen) / 1000
                    ));
                    r.confidence = r.confidence.min(Confidence::Medium);
                }
            }
        }
    } else if let (Some(local), Some(utc)) = (
        ev.exif
            .or(ev.mdpm)
            .filter(|_| source != CaptureSource::Container),
        ev.container_utc_ms,
    ) {
        // A local clock plus a UTC container time reveal the offset, if it
        // looks like a real one (whole quarter hours within ±18 h).
        let diff_min = (local.local_ms - utc) as f64 / 60_000.0;
        let q = (diff_min / 15.0).round() * 15.0;
        if (diff_min - q).abs() * 60.0 <= CONFLICT_MS as f64 / 1000.0 && q.abs() <= 18.0 * 60.0 {
            let o = q as i16;
            r.time = CaptureTime {
                utc_ms: local.local_ms - i64::from(o) * 60_000,
                utc_offset_min: Some(o),
            };
            r.confidence = Confidence::Medium;
            r.notes
                .push("UTC offset derived from the container's UTC time".into());
        }
    }
    Some(r)
}

/// One asset's input to [`borrow_offsets`].
pub struct Neighbour<'a> {
    pub camera: Option<&'a str>,
    pub resolved: &'a mut Option<Resolved>,
    pub label: &'a str,
}

/// Give assets with an unknown offset the offset of the nearest asset from
/// the same camera (within [`OFFSET_BORROW_WINDOW_MS`]); confidence becomes
/// medium.
pub fn borrow_offsets(items: &mut [Neighbour<'_>]) {
    // Donors: (camera, clock reading, offset, label).
    let donors: Vec<(Option<String>, i64, i16, String)> = items
        .iter()
        .filter_map(|n| {
            let r = n.resolved.as_ref()?;
            let o = r.time.utc_offset_min?;
            if r.notes.iter().any(|x| x.starts_with("UTC offset borrowed")) {
                return None;
            }
            // Compare on the local clock, which is what offset-less sources hold.
            let local = r.time.utc_ms + i64::from(o) * 60_000;
            Some((n.camera.map(str::to_owned), local, o, n.label.to_owned()))
        })
        .collect();
    for n in items.iter_mut() {
        let Some(r) = n.resolved.as_mut() else {
            continue;
        };
        if !r.needs_offset() {
            continue;
        }
        let best = donors
            .iter()
            .filter(|d| d.0.as_deref() == n.camera && n.camera.is_some())
            .map(|d| ((d.1 - r.time.utc_ms).abs(), d))
            .filter(|(gap, _)| *gap <= OFFSET_BORROW_WINDOW_MS)
            .min_by_key(|(gap, _)| *gap);
        if let Some((_, (_, _, o, label))) = best {
            let o = *o;
            r.time = match r.source {
                // Container times are already UTC; only the offset was missing.
                CaptureSource::Container => CaptureTime {
                    utc_ms: r.time.utc_ms,
                    utc_offset_min: Some(o),
                },
                _ => CaptureTime {
                    utc_ms: r.time.utc_ms - i64::from(o) * 60_000,
                    utc_offset_min: Some(o),
                },
            };
            r.confidence = Confidence::Medium;
            r.notes.push(format!("UTC offset borrowed from {label}"));
        }
    }
}

/// Parse an EXIF/QuickTime/ExifTool date: `2026:10:14 17:56:12`, optionally
/// with sub-seconds and an offset (`+02:00`, `Z`, and a trailing ` DST`).
/// Returns `None` for zero dates.
pub fn parse_exif_date(s: &str) -> Option<LocalTime> {
    let s = s.trim().trim_end_matches(" DST").trim();
    if s.starts_with("0000") {
        return None;
    }
    let (body, offset) = split_offset(s);
    let normalized = body.replacen(':', "-", 2).replacen('T', " ", 1);
    let dt = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ]
    .iter()
    .find_map(|f| NaiveDateTime::parse_from_str(&normalized, f).ok())?;
    Some(LocalTime {
        local_ms: dt.and_utc().timestamp_millis(),
        offset_min: offset,
    })
}

fn split_offset(s: &str) -> (&str, Option<i16>) {
    if let Some(b) = s.strip_suffix('Z') {
        return (b, Some(0));
    }
    // Look for ±HH:MM (or ±HHMM) after the time part.
    if s.len() > 6 {
        for cut in [6usize, 5] {
            if s.len() <= cut {
                continue;
            }
            let (b, tail) = s.split_at(s.len() - cut);
            if let Some(o) = parse_offset(tail) {
                if b.len() >= 16 {
                    return (b, Some(o));
                }
            }
        }
    }
    (s, None)
}

/// `+05:30`, `-0500` → minutes.
pub fn parse_offset(s: &str) -> Option<i16> {
    let s = s.trim();
    let sign = match s.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = s[1..].chars().filter(|c| *c != ':').collect();
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let h: i16 = digits[..2].parse().ok()?;
    let m: i16 = digits[2..].parse().ok()?;
    (h <= 18 && m < 60).then_some(sign * (h * 60 + m))
}

/// Apply EXIF `SubSecTimeOriginal` (`"285"` → 285 ms, `"05"` → 50 ms).
pub fn with_subsec(t: LocalTime, subsec: Option<&str>) -> LocalTime {
    let Some(ss) = subsec.map(str::trim).filter(|s| !s.is_empty()) else {
        return t;
    };
    if !ss.chars().all(|c| c.is_ascii_digit()) {
        return t;
    }
    let mut d: String = ss.chars().take(3).collect();
    while d.len() < 3 {
        d.push('0');
    }
    let ms: i64 = d.parse().unwrap_or(0);
    // Only add when the date itself carried no fraction.
    if t.local_ms % 1000 == 0 {
        LocalTime {
            local_ms: t.local_ms + ms,
            ..t
        }
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: i64 = 1_792_000_000_000; // some local clock reading

    fn lt(ms: i64, o: Option<i16>) -> LocalTime {
        LocalTime {
            local_ms: ms,
            offset_min: o,
        }
    }

    #[test]
    fn precedence_order() {
        let xml = CaptureTime {
            utc_ms: LOCAL + 300 * 60_000,
            utc_offset_min: Some(-300),
        };
        let all = Evidence {
            sony_xml: Some(xml),
            exif: Some(lt(LOCAL, Some(-300))),
            mdpm: Some(lt(LOCAL, Some(-300))),
            container_utc_ms: Some(LOCAL + 300 * 60_000),
        };
        let r = resolve(&all).unwrap();
        assert_eq!(
            (r.source, r.confidence),
            (CaptureSource::SonyXml, Confidence::High)
        );
        assert!(r.notes.is_empty());

        let r = resolve(&Evidence {
            sony_xml: None,
            ..all.clone()
        })
        .unwrap();
        assert_eq!(r.source, CaptureSource::Exif);
        let r = resolve(&Evidence {
            sony_xml: None,
            exif: None,
            ..all.clone()
        })
        .unwrap();
        assert_eq!(r.source, CaptureSource::AvchdMdpm);
        let r = resolve(&Evidence {
            container_utc_ms: Some(5),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            (r.source, r.confidence),
            (CaptureSource::Container, Confidence::Low)
        );
        assert!(resolve(&Evidence::default()).is_none());
    }

    #[test]
    fn exif_with_offset_is_high_and_converts_to_utc() {
        let r = resolve(&Evidence {
            exif: Some(lt(LOCAL, Some(-300))),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.time.utc_ms, LOCAL + 300 * 60_000);
        assert_eq!(r.time.utc_offset_min, Some(-300));
        assert_eq!(r.confidence, Confidence::High);
    }

    #[test]
    fn missing_offset_is_low_and_keeps_local_clock() {
        let r = resolve(&Evidence {
            exif: Some(lt(LOCAL, None)),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            r.time,
            CaptureTime {
                utc_ms: LOCAL,
                utc_offset_min: None
            }
        );
        assert_eq!(r.confidence, Confidence::Low);
    }

    #[test]
    fn conflicts_lower_confidence() {
        let r = resolve(&Evidence {
            exif: Some(lt(LOCAL, Some(0))),
            container_utc_ms: Some(LOCAL + 60_000),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.confidence, Confidence::Medium);
        assert_eq!(r.notes.len(), 1);
    }

    #[test]
    fn local_clock_plus_container_utc_gives_offset() {
        let r = resolve(&Evidence {
            mdpm: Some(lt(LOCAL, None)),
            container_utc_ms: Some(LOCAL + 330 * 60_000 + 400), // +05:30 camera
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.time.utc_offset_min, Some(-330));
        assert_eq!(r.confidence, Confidence::Medium);
    }

    #[test]
    fn borrowing() {
        let mut a = resolve(&Evidence {
            exif: Some(lt(LOCAL, Some(120))),
            ..Default::default()
        });
        let mut b = resolve(&Evidence {
            exif: Some(lt(LOCAL + 60_000, None)),
            ..Default::default()
        });
        let mut c = resolve(&Evidence {
            container_utc_ms: Some(LOCAL),
            ..Default::default()
        });
        let mut far = resolve(&Evidence {
            exif: Some(lt(LOCAL + 13 * 3_600_000, None)),
            ..Default::default()
        });
        let mut other = resolve(&Evidence {
            exif: Some(lt(LOCAL, None)),
            ..Default::default()
        });
        let mut items = vec![
            Neighbour {
                camera: Some("A"),
                resolved: &mut a,
                label: "a",
            },
            Neighbour {
                camera: Some("A"),
                resolved: &mut b,
                label: "b",
            },
            Neighbour {
                camera: Some("A"),
                resolved: &mut c,
                label: "c",
            },
            Neighbour {
                camera: Some("A"),
                resolved: &mut far,
                label: "far",
            },
            Neighbour {
                camera: Some("B"),
                resolved: &mut other,
                label: "other",
            },
        ];
        borrow_offsets(&mut items);
        let b = b.unwrap();
        assert_eq!(b.time.utc_offset_min, Some(120));
        assert_eq!(b.time.utc_ms, LOCAL + 60_000 - 120 * 60_000);
        assert_eq!(b.confidence, Confidence::Medium);
        let c = c.unwrap();
        assert_eq!(
            c.time,
            CaptureTime {
                utc_ms: LOCAL,
                utc_offset_min: Some(120)
            }
        );
        assert!(far.unwrap().needs_offset());
        assert!(other.unwrap().needs_offset());
    }

    #[test]
    fn exif_dates() {
        let base = NaiveDateTime::parse_from_str("2026-10-14 17:56:12", "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(parse_exif_date("2026:10:14 17:56:12"), Some(lt(base, None)));
        assert_eq!(
            parse_exif_date("2026:10:14 17:56:12-05:00"),
            Some(lt(base, Some(-300)))
        );
        assert_eq!(
            parse_exif_date("2026:10:14 17:56:12+02:00 DST"),
            Some(lt(base, Some(120)))
        );
        assert_eq!(
            parse_exif_date("2026:10:14 17:56:12Z"),
            Some(lt(base, Some(0)))
        );
        assert_eq!(
            parse_exif_date("2026:10:14 17:56:12.25"),
            Some(lt(base + 250, None))
        );
        assert_eq!(parse_exif_date("0000:00:00 00:00:00"), None);
        assert_eq!(
            with_subsec(lt(base, None), Some("285")).local_ms,
            base + 285
        );
        assert_eq!(with_subsec(lt(base, None), Some("05")).local_ms, base + 50);
        assert_eq!(with_subsec(lt(base, None), Some("5")).local_ms, base + 500);
        assert_eq!(parse_offset("+0530"), Some(330));
        assert_eq!(parse_offset("-19:00"), None);
    }
}
