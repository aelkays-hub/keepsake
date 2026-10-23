//! Probe grouped assets: read every source of metadata for each group,
//! resolve capture times, borrow missing offsets and flag sequences.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::avchd;
use super::capture::{self, Confidence, Evidence, Neighbour, Resolved};
use super::error::ProbeError;
use super::exiftool::ExifTool;
use super::ffprobe::{self, MediaStreams, VideoStream};
use super::sony_xml::{self, Gps, SonyXml};
use super::tools::ToolPaths;
use crate::catalog::FileRole;
use crate::grouping::sequences::{self, Frame, Sequence};
use crate::grouping::{GroupKind, Grouping};
use crate::relpath::RelPath;

/// Timeouts and limits.
#[derive(Debug, Clone)]
pub struct ProbeOptions {
    /// Per ExifTool request (a batch of files).
    pub exiftool_timeout: Duration,
    pub ffprobe_timeout: Duration,
    /// Files per ExifTool request.
    pub exiftool_batch: usize,
    /// Bytes of an `.MTS` scanned for the MDPM date when ExifTool has none.
    pub mdpm_scan_bytes: usize,
}

impl Default for ProbeOptions {
    fn default() -> Self {
        Self {
            exiftool_timeout: Duration::from_secs(60),
            ffprobe_timeout: Duration::from_secs(30),
            exiftool_batch: 64,
            mdpm_scan_bytes: 8 << 20,
        }
    }
}

/// Everything learned about one asset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbedAsset {
    pub capture: Option<Resolved>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub lens: Option<String>,
    pub duration_ms: Option<i64>,
    pub gps: Option<Gps>,
    pub video: Option<VideoStream>,
    pub timecode: Option<String>,
    pub recording_mode: Option<String>,
    pub problems: Vec<String>,
}

/// Results for a whole grouping; `assets[i]` belongs to `grouping.groups[i]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeReport {
    pub assets: Vec<ProbedAsset>,
    /// Members are group indices.
    pub sequences: Vec<Sequence>,
}

/// Owns the ExifTool process and knows where ffprobe is.
pub struct Prober {
    exif: ExifTool,
    ffprobe: PathBuf,
    opts: ProbeOptions,
}

fn jstr(m: &Map<String, Value>, k: &str) -> Option<String> {
    match m.get(k)? {
        Value::String(s) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn jf64(m: &Map<String, Value>, k: &str) -> Option<f64> {
    match m.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn exif_local_time(m: &Map<String, Value>) -> Option<capture::LocalTime> {
    let raw = jstr(m, "DateTimeOriginal")?;
    let mut t = capture::parse_exif_date(&raw)?;
    if t.offset_min.is_none() {
        t.offset_min = jstr(m, "OffsetTimeOriginal")
            .or_else(|| jstr(m, "OffsetTime"))
            .and_then(|o| capture::parse_offset(&o));
    }
    Some(capture::with_subsec(
        t,
        jstr(m, "SubSecTimeOriginal").as_deref(),
    ))
}

fn exif_gps(m: &Map<String, Value>) -> Option<Gps> {
    let lat = jf64(m, "GPSLatitude")?;
    let lon = jf64(m, "GPSLongitude")?;
    ((-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
        && !(lat == 0.0 && lon == 0.0))
        .then(|| Gps {
            lat,
            lon,
            alt_m: jf64(m, "GPSAltitude"),
        })
}

impl Prober {
    pub fn new(tools: &ToolPaths, opts: ProbeOptions) -> Result<Self, ProbeError> {
        tools.check()?;
        Ok(Self {
            exif: ExifTool::start(&tools.exiftool, opts.exiftool_timeout)?,
            ffprobe: tools.ffprobe.clone(),
            opts,
        })
    }

    pub fn exiftool(&self) -> &ExifTool {
        &self.exif
    }

    /// Read tags for many files in batches; failures are recorded per file.
    fn exif_batch(&self, paths: &[PathBuf]) -> Vec<Result<Map<String, Value>, String>> {
        let mut out = Vec::with_capacity(paths.len());
        for chunk in paths.chunks(self.opts.exiftool_batch.max(1)) {
            let refs: Vec<&Path> = chunk.iter().map(PathBuf::as_path).collect();
            match self.exif.read_tags(&refs) {
                Ok(maps) => out.extend(maps.into_iter().map(Ok)),
                Err(_) if chunk.len() > 1 => {
                    // Retry one by one so a single bad file does not hide the rest.
                    for p in chunk {
                        out.push(
                            self.exif
                                .read_tags(&[p.as_path()])
                                .map(|mut v| v.pop().unwrap_or_default())
                                .map_err(|e| e.to_string()),
                        );
                    }
                }
                Err(e) => out.push(Err(e.to_string())),
            }
        }
        out
    }

    fn ffprobe(&self, path: &Path) -> Result<MediaStreams, ProbeError> {
        ffprobe::probe(&self.ffprobe, path, self.opts.ffprobe_timeout)
    }

    /// Probe every group. Never fails as a whole: per-asset problems are reported.
    pub fn probe(&self, root: &Path, files: &[RelPath], grouping: &Grouping) -> ProbeReport {
        let groups = &grouping.groups;
        let mut assets: Vec<ProbedAsset> = vec![ProbedAsset::default(); groups.len()];
        let mut evidence: Vec<Evidence> = vec![Evidence::default(); groups.len()];
        let mut release: Vec<(Option<i64>, Option<i64>)> = vec![(None, None); groups.len()];
        let full = |i: usize| files[i].to_fs_path(root);

        // Which file ExifTool reads for each group (stills and non-XAVC video).
        let exif_targets: Vec<(usize, PathBuf)> = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.kind != GroupKind::Xavc)
            .map(|(gi, g)| (gi, full(g.original())))
            .collect();
        let paths: Vec<PathBuf> = exif_targets.iter().map(|(_, p)| p.clone()).collect();
        let tags = self.exif_batch(&paths);
        // Stills whose original has no date: try the RAW/HEIF companions.
        let mut fallback: Vec<(usize, PathBuf)> = Vec::new();
        for ((gi, _), t) in exif_targets.iter().zip(&tags) {
            match t {
                Ok(m) => {
                    let a = &mut assets[*gi];
                    a.make = jstr(m, "Make");
                    a.model = jstr(m, "Model");
                    a.serial = jstr(m, "SerialNumber");
                    a.lens = jstr(m, "LensModel").or_else(|| jstr(m, "LensID"));
                    a.gps = exif_gps(m);
                    release[*gi] = (
                        jf64(m, "ReleaseMode2").map(|v| v as i64),
                        jf64(m, "SequenceNumber").map(|v| v as i64),
                    );
                    let lt = exif_local_time(m);
                    match groups[*gi].kind {
                        GroupKind::Avchd => evidence[*gi].mdpm = lt,
                        _ => evidence[*gi].exif = lt,
                    }
                    if let Some(utc) = jstr(m, "CreateDate")
                        .or_else(|| jstr(m, "MediaCreateDate"))
                        .filter(|_| groups[*gi].kind != GroupKind::Still)
                        .and_then(|d| capture::parse_exif_date(&d))
                    {
                        // QuickTime CreateDate is UTC by specification.
                        evidence[*gi].container_utc_ms.get_or_insert(utc.local_ms);
                    }
                    if lt.is_none() && groups[*gi].kind == GroupKind::Still {
                        for m in &groups[*gi].members[1..] {
                            if m.role == FileRole::RawPair {
                                fallback.push((*gi, full(m.index)));
                            }
                        }
                    }
                }
                Err(e) => assets[*gi].problems.push(format!("exiftool: {e}")),
            }
        }
        if !fallback.is_empty() {
            let paths: Vec<PathBuf> = fallback.iter().map(|(_, p)| p.clone()).collect();
            for ((gi, _), t) in fallback.iter().zip(self.exif_batch(&paths)) {
                if let Ok(m) = t {
                    if evidence[*gi].exif.is_none() {
                        evidence[*gi].exif = exif_local_time(&m);
                    }
                    let a = &mut assets[*gi];
                    a.make = a.make.take().or_else(|| jstr(&m, "Make"));
                    a.model = a.model.take().or_else(|| jstr(&m, "Model"));
                    a.lens = a.lens.take().or_else(|| jstr(&m, "LensModel"));
                }
            }
        }

        for (gi, g) in groups.iter().enumerate() {
            let a = &mut assets[gi];
            match g.kind {
                GroupKind::Xavc => {
                    let xml = g.members.iter().find(|m| {
                        m.role == FileRole::Sidecar
                            && files[m.index]
                                .file_name()
                                .to_ascii_lowercase()
                                .ends_with(".xml")
                    });
                    let mut sx = SonyXml::default();
                    match xml.map(|m| sony_xml::read(&full(m.index))) {
                        Some(Ok(x)) => sx = x,
                        Some(Err(e)) => a.problems.push(format!("Sony XML: {e}")),
                        None => a.problems.push("no Sony XML sidecar".into()),
                    }
                    evidence[gi].sony_xml = sx.creation;
                    a.make = sx.make.clone();
                    a.model = sx.model.clone();
                    a.serial = sx.serial.clone();
                    a.lens = sx.lens.clone();
                    a.gps = sx.gps;
                    a.recording_mode = sx.recording_mode.clone();
                    a.timecode = sx.timecode.map(|t| t.to_string());
                    a.duration_ms = sx.duration_ms;
                    match self.ffprobe(&full(g.original())) {
                        Ok(s) => {
                            evidence[gi].container_utc_ms = s.creation_utc_ms;
                            a.duration_ms = a.duration_ms.or(s.duration_ms);
                            a.timecode = a.timecode.take().or(s.timecode);
                            a.video = s.video;
                        }
                        Err(e) => a.problems.push(format!("ffprobe: {e}")),
                    }
                }
                GroupKind::Video => match self.ffprobe(&full(g.original())) {
                    Ok(s) => {
                        evidence[gi].container_utc_ms =
                            s.creation_utc_ms.or(evidence[gi].container_utc_ms);
                        a.duration_ms = s.duration_ms;
                        a.timecode = s.timecode;
                        a.video = s.video;
                    }
                    Err(e) => a.problems.push(format!("ffprobe: {e}")),
                },
                GroupKind::Avchd => {
                    let segs = g.segments();
                    let mut total: i64 = 0;
                    let mut all_known = true;
                    for (n, s) in segs.iter().enumerate() {
                        match self.ffprobe(&full(*s)) {
                            Ok(st) => {
                                match st.duration_ms {
                                    Some(d) => total += d,
                                    None => all_known = false,
                                }
                                if n == 0 {
                                    a.video = st.video;
                                    a.timecode = st.timecode;
                                }
                            }
                            Err(e) => {
                                all_known = false;
                                a.problems.push(format!("ffprobe {}: {e}", files[*s]));
                            }
                        }
                    }
                    a.duration_ms = (all_known && !segs.is_empty()).then_some(total);
                    if evidence[gi].mdpm.is_none() {
                        match avchd::read_mdpm(&full(g.original()), self.opts.mdpm_scan_bytes) {
                            Ok(Some(d)) => {
                                evidence[gi].mdpm = Some(capture::LocalTime {
                                    local_ms: d.local_ms,
                                    offset_min: None,
                                })
                            }
                            Ok(None) => {}
                            Err(e) => a.problems.push(format!("MDPM scan: {e}")),
                        }
                    }
                }
                GroupKind::Still => {}
            }
            a.capture = capture::resolve(&evidence[gi]);
            if a.capture.is_none() {
                a.problems.push("no capture time found".into());
            }
        }

        // Borrow offsets between assets of the same camera.
        let cameras: Vec<Option<String>> = assets
            .iter()
            .map(|a| a.serial.clone().or_else(|| a.model.clone()))
            .collect();
        let labels: Vec<String> = groups
            .iter()
            .map(|g| files[g.original()].to_string())
            .collect();
        {
            let mut items: Vec<Neighbour<'_>> = assets
                .iter_mut()
                .zip(&cameras)
                .zip(&labels)
                .map(|((a, c), l)| Neighbour {
                    camera: c.as_deref(),
                    resolved: &mut a.capture,
                    label: l.as_str(),
                })
                .collect();
            capture::borrow_offsets(&mut items);
        }

        // Sequences among stills.
        let frames: Vec<Frame> = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.kind == GroupKind::Still)
            .filter_map(|(gi, _)| {
                let c = assets[gi].capture.as_ref()?;
                let local = c.time.utc_ms + i64::from(c.time.utc_offset_min.unwrap_or(0)) * 60_000;
                Some(Frame {
                    id: gi,
                    time_ms: local,
                    camera: cameras[gi].clone(),
                    release_mode2: release[gi].0,
                    sequence_number: release[gi].1,
                })
            })
            .collect();
        ProbeReport {
            assets,
            sequences: sequences::detect(&frames),
        }
    }
}

/// Lowest confidence across a report (for summaries).
pub fn weakest(report: &ProbeReport) -> Option<Confidence> {
    report
        .assets
        .iter()
        .filter_map(|a| a.capture.as_ref().map(|c| c.confidence))
        .min()
}
