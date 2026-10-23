//! AVCHD structures: playlists (`.MPL`), clip information (`.CPI`) and the
//! Sony/Panasonic `MDPM` capture date embedded in the H.264 stream.
//!
//! Cameras split long recordings into several `.MTS` segments (FAT32's 4 GB
//! limit). A playlist lists every clip as a PlayItem; an item whose
//! `connection_condition` is 5 or 6 continues the previous item seamlessly,
//! so consecutive items joined that way are one recording.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::error::ProbeError;

/// One PlayItem of a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayItem {
    /// Five-digit clip name, e.g. `00001`.
    pub clip: String,
    pub connection_condition: u8,
    /// 45 kHz ticks.
    pub in_time: u32,
    pub out_time: u32,
}

impl PlayItem {
    /// True when this item continues the previous one without a break.
    pub fn is_seamless(&self) -> bool {
        matches!(self.connection_condition, 5 | 6)
    }

    pub fn duration_ms(&self) -> u64 {
        u64::from(self.out_time.saturating_sub(self.in_time)) / 45
    }
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn bad(what: &'static str, detail: &str) -> ProbeError {
    ProbeError::parse(what, detail)
}

/// Parse an `MPLS` playlist.
pub fn parse_mpl(b: &[u8]) -> Result<Vec<PlayItem>, ProbeError> {
    const W: &str = "AVCHD playlist";
    if b.get(..4) != Some(b"MPLS") {
        return Err(bad(W, "missing MPLS header"));
    }
    let pl = be32(b, 8).ok_or_else(|| bad(W, "truncated header"))? as usize;
    let count = be16(b, pl + 6).ok_or_else(|| bad(W, "truncated PlayList"))?;
    let mut items = Vec::with_capacity(usize::from(count));
    let mut at = pl + 10;
    for _ in 0..count {
        let len = usize::from(be16(b, at).ok_or_else(|| bad(W, "truncated PlayItem"))?);
        let body = b
            .get(at + 2..at + 2 + len)
            .ok_or_else(|| bad(W, "PlayItem runs past end"))?;
        if body.len() < 20 {
            return Err(bad(W, "PlayItem too short"));
        }
        let clip = std::str::from_utf8(&body[0..5])
            .map_err(|_| bad(W, "clip name is not ASCII"))?
            .to_owned();
        if !clip.bytes().all(|c| c.is_ascii_digit()) {
            return Err(bad(W, "clip name is not five digits"));
        }
        // body[5..9] = codec id ("M2TS"); body[9..11] = 11 reserved bits,
        // is_multi_angle, connection_condition (4 bits).
        let flags = u16::from_be_bytes([body[9], body[10]]);
        items.push(PlayItem {
            clip,
            connection_condition: (flags & 0x0F) as u8,
            in_time: u32::from_be_bytes([body[12], body[13], body[14], body[15]]),
            out_time: u32::from_be_bytes([body[16], body[17], body[18], body[19]]),
        });
        at += 2 + len;
    }
    Ok(items)
}

/// The parts of a `HDMV` clip-information file Keepsake uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipInfo {
    pub ts_recording_rate: u32,
    pub source_packets: u32,
}

impl ClipInfo {
    /// Expected `.MTS` size: 192-byte source packets.
    pub fn expected_size(&self) -> u64 {
        u64::from(self.source_packets) * 192
    }
}

/// Parse a `HDMV` clip-information (`.CPI`) file.
pub fn parse_cpi(b: &[u8]) -> Result<ClipInfo, ProbeError> {
    const W: &str = "AVCHD clip info";
    if b.get(..4) != Some(b"HDMV") {
        return Err(bad(W, "missing HDMV header"));
    }
    // ClipInfo() starts at byte 40: length(4) reserved(2) stream_type(1)
    // application_type(1) flags(4) TS_recording_rate(4) source_packets(4).
    Ok(ClipInfo {
        ts_recording_rate: be32(b, 52).ok_or_else(|| bad(W, "truncated ClipInfo"))?,
        source_packets: be32(b, 56).ok_or_else(|| bad(W, "truncated ClipInfo"))?,
    })
}

/// Join playlists into recordings: ordered lists of clip names.
///
/// Every playlist contributes "B continues A" links; a clip appears in
/// exactly one recording even when several playlists list it.
pub fn recordings(playlists: &[Vec<PlayItem>]) -> Vec<Vec<String>> {
    let mut next: BTreeMap<String, String> = BTreeMap::new();
    let mut has_prev: BTreeSet<String> = BTreeSet::new();
    let mut all: Vec<String> = Vec::new();
    for pl in playlists {
        for (i, item) in pl.iter().enumerate() {
            if !all.contains(&item.clip) {
                all.push(item.clip.clone());
            }
            if i > 0 && item.is_seamless() {
                let prev = &pl[i - 1].clip;
                if prev != &item.clip && !next.contains_key(prev) && !has_prev.contains(&item.clip)
                {
                    next.insert(prev.clone(), item.clip.clone());
                    has_prev.insert(item.clip.clone());
                }
            }
        }
    }
    all.sort();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for start in all.iter().filter(|c| !has_prev.contains(*c)) {
        let mut chain = vec![start.clone()];
        seen.insert(start.clone());
        let mut cur = start.clone();
        while let Some(n) = next.get(&cur) {
            if !seen.insert(n.clone()) {
                break; // malformed cycle
            }
            chain.push(n.clone());
            cur = n.clone();
        }
        out.push(chain);
    }
    // Clips only reachable through a cycle.
    for c in all {
        if !seen.contains(&c) {
            seen.insert(c.clone());
            out.push(vec![c]);
        }
    }
    out
}

/// Date and time read from an MDPM block (local camera time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MdpmDate {
    /// Local time as Unix milliseconds *as if it were UTC*.
    pub local_ms: i64,
}

/// The H.264 SEI "user data unregistered" UUID used for MDPM, then `MDPM`.
const MDPM_MAGIC: [u8; 20] = [
    0x17, 0xEE, 0x8C, 0x60, 0xF8, 0x4D, 0x11, 0xD9, 0x8C, 0xD6, 0x08, 0x00, 0x20, 0x0C, 0x9A, 0x66,
    b'M', b'D', b'P', b'M',
];

/// Find the first MDPM date/time in a byte buffer.
///
/// The block is a count byte followed by 5-byte entries `tag d0 d1 d2 d3`.
/// Tag 0x18 holds `?? YY YY MM` and tag 0x19 `DD hh mm ss`, all BCD.
pub fn scan_mdpm(buf: &[u8]) -> Option<MdpmDate> {
    let mut from = 0;
    while let Some(pos) = find(&buf[from..], &MDPM_MAGIC) {
        let start = from + pos + MDPM_MAGIC.len();
        if let Some(d) = decode_mdpm(&buf[start..]) {
            return Some(d);
        }
        from = start;
    }
    None
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn bcd(b: u8) -> Option<u32> {
    let (hi, lo) = (u32::from(b >> 4), u32::from(b & 0x0F));
    (hi <= 9 && lo <= 9).then_some(hi * 10 + lo)
}

fn decode_mdpm(b: &[u8]) -> Option<MdpmDate> {
    let count = usize::from(*b.first()?);
    let mut date: Option<[u8; 4]> = None;
    let mut time: Option<[u8; 4]> = None;
    for i in 0..count {
        let e = b.get(1 + i * 5..1 + i * 5 + 5)?;
        match e[0] {
            0x18 => date = Some([e[1], e[2], e[3], e[4]]),
            0x19 => time = Some([e[1], e[2], e[3], e[4]]),
            _ => {}
        }
    }
    let (d, t) = (date?, time?);
    let year = bcd(d[1])? * 100 + bcd(d[2])?;
    let month = bcd(d[3])?;
    let (day, h, m, s) = (bcd(t[0])?, bcd(t[1])?, bcd(t[2])?, bcd(t[3])?);
    let dt = NaiveDate::from_ymd_opt(year as i32, month, day)?.and_hms_opt(h, m, s)?;
    Some(MdpmDate {
        local_ms: dt.and_utc().timestamp_millis(),
    })
}

/// Scan the first `limit` bytes of a file for an MDPM date.
pub fn read_mdpm(path: &Path, limit: usize) -> Result<Option<MdpmDate>, ProbeError> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(limit.min(1 << 24));
    f.by_ref().take(limit as u64).read_to_end(&mut buf)?;
    Ok(scan_mdpm(&buf))
}

/// Build MPL/CPI/MDPM bytes for tests and fixtures.
pub mod synth {
    use super::*;

    /// A playlist with the given `(clip, connection_condition, in, out)` items.
    pub fn mpl(items: &[(&str, u8, u32, u32)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"MPLS0100");
        let pl_start: u32 = 40;
        b.extend_from_slice(&pl_start.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes()); // PlayListMark
        b.extend_from_slice(&0u32.to_be_bytes()); // ExtensionData
        b.resize(pl_start as usize, 0);
        let mut body = Vec::new();
        body.extend_from_slice(&0u16.to_be_bytes()); // reserved
        body.extend_from_slice(&(items.len() as u16).to_be_bytes());
        body.extend_from_slice(&0u16.to_be_bytes()); // SubPaths
        for (clip, cc, tin, tout) in items {
            let mut it = Vec::new();
            it.extend_from_slice(clip.as_bytes());
            it.extend_from_slice(b"M2TS");
            it.extend_from_slice(&u16::from(*cc & 0x0F).to_be_bytes());
            it.push(0); // ref_to_STC_id
            it.extend_from_slice(&tin.to_be_bytes());
            it.extend_from_slice(&tout.to_be_bytes());
            it.extend_from_slice(&[0u8; 12]); // UO mask etc.
            body.extend_from_slice(&(it.len() as u16).to_be_bytes());
            body.extend_from_slice(&it);
        }
        b.extend_from_slice(&(body.len() as u32).to_be_bytes());
        b.extend_from_slice(&body);
        b
    }

    /// A clip-information file.
    pub fn cpi(source_packets: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"HDMV0100");
        b.resize(52, 0);
        b.extend_from_slice(&24_000_000u32.to_be_bytes());
        b.extend_from_slice(&source_packets.to_be_bytes());
        b.resize(80, 0);
        b
    }

    /// An MDPM block (magic + entries) for the given local date/time.
    pub fn mdpm(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> Vec<u8> {
        let to_bcd = |v: u8| ((v / 10) << 4) | (v % 10);
        let mut b = MDPM_MAGIC.to_vec();
        b.push(2);
        b.extend_from_slice(&[
            0x18,
            0x00,
            to_bcd((y / 100) as u8),
            to_bcd((y % 100) as u8),
            to_bcd(mo),
        ]);
        b.extend_from_slice(&[0x19, to_bcd(d), to_bcd(h), to_bcd(mi), to_bcd(s)]);
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_round_trip() {
        let b = synth::mpl(&[("00001", 1, 0, 45_000), ("00002", 5, 45_000, 90_000)]);
        let items = parse_mpl(&b).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].clip, "00001");
        assert!(!items[0].is_seamless());
        assert!(items[1].is_seamless());
        assert_eq!(items[1].duration_ms(), 1000);
    }

    #[test]
    fn rejects_bad_playlists() {
        assert!(parse_mpl(b"STUB_MPL").is_err());
        let mut b = synth::mpl(&[("00001", 1, 0, 1)]);
        b.truncate(b.len() - 5);
        assert!(parse_mpl(&b).is_err());
        assert!(parse_cpi(b"nope").is_err());
    }

    #[test]
    fn recordings_join_seamless_segments() {
        let pl = parse_mpl(&synth::mpl(&[
            ("00001", 1, 0, 1),
            ("00002", 5, 0, 1),
            ("00003", 6, 0, 1),
            ("00004", 1, 0, 1),
            ("00005", 1, 0, 1),
            ("00006", 5, 0, 1),
        ]))
        .unwrap();
        // A second playlist listing a clip again must not duplicate it.
        let pl2 = parse_mpl(&synth::mpl(&[("00004", 1, 0, 1)])).unwrap();
        assert_eq!(
            recordings(&[pl, pl2]),
            vec![
                vec!["00001", "00002", "00003"],
                vec!["00004"],
                vec!["00005", "00006"]
            ]
        );
    }

    #[test]
    fn cpi_size() {
        let c = parse_cpi(&synth::cpi(1000)).unwrap();
        assert_eq!(c.expected_size(), 192_000);
    }

    #[test]
    fn mdpm_scan() {
        let mut buf = vec![0u8; 1000];
        buf.extend(synth::mdpm(2013, 5, 3, 10, 12, 13));
        buf.extend(vec![0u8; 100]);
        let d = scan_mdpm(&buf).unwrap();
        let expect = NaiveDate::from_ymd_opt(2013, 5, 3)
            .unwrap()
            .and_hms_opt(10, 12, 13)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(d.local_ms, expect);
        assert!(scan_mdpm(&[0u8; 100]).is_none());
    }
}
