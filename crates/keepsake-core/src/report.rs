//! The library report as a CSV file (Tools → Report → Export, and
//! `keepsake report --csv`). The numbers come from
//! [`crate::catalog::Report`].

use std::path::Path;

use crate::catalog::{AircraftReport, AircraftRow, LibraryReport, ReportRow};

/// The CSV columns.
pub const CSV_HEADER: &str = "Breakdown,Group,Photos,Videos,Items,Files,Bytes,Size";

/// A size for people: `1.5 GB` (decimal units, like the app).
pub fn human_bytes(n: u64) -> String {
    // Precision loss above 2^53 bytes (9 PB) does not matter for display.
    let f = n as f64;
    if f >= 1e12 {
        format!("{:.2} TB", f / 1e12)
    } else if f >= 1e9 {
        format!("{:.1} GB", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.1} MB", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.0} KB", f / 1e3)
    } else {
        format!("{n} B")
    }
}

/// Quote a CSV field when it needs it (RFC 4180). A leading `=`, `+`, `-`
/// or `@` gets a `'` so spreadsheets do not run a camera name as a formula.
fn field(s: &str) -> String {
    let s = match s.chars().next() {
        Some('=' | '+' | '-' | '@') => format!("'{s}"),
        _ => s.to_owned(),
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn opt(n: Option<u64>) -> String {
    n.map(|n| n.to_string()).unwrap_or_default()
}

/// Photos and videos are blank where the report does not split them (file
/// types and drives).
fn line(out: &mut String, breakdown: &str, group: &str, r: &ReportRow) {
    out.push_str(&format!(
        "{},{},{},{},{},{},{},{}\r\n",
        field(breakdown),
        field(group),
        opt(r.photos),
        opt(r.videos),
        r.items,
        r.files,
        r.bytes,
        field(&human_bytes(r.bytes)),
    ));
}

/// The whole report, one breakdown after another (CRLF line ends).
pub fn to_csv(r: &LibraryReport) -> String {
    let mut out = String::new();
    out.push_str(CSV_HEADER);
    out.push_str("\r\n");
    line(&mut out, "Total", "All", &r.totals);
    let sections: [(&str, &[ReportRow]); 6] = [
        ("Year", &r.years),
        ("Month", &r.months),
        ("Day", &r.days),
        ("Camera", &r.cameras),
        ("Lens", &r.lenses),
        ("File type", &r.file_types),
    ];
    for (name, rows) in sections {
        for row in rows {
            line(&mut out, name, row.label(), row);
        }
    }
    for d in &r.drives {
        let name = if d.online {
            d.row.label().to_owned()
        } else {
            format!("{} (unplugged)", d.row.label())
        };
        line(&mut out, "Drive", &name, &d.row);
    }
    out
}

/// Write [`to_csv`] to `path` (UTF-8 with a byte-order mark, so Excel reads
/// accented names correctly).
pub fn write_csv(r: &LibraryReport, path: &Path) -> std::io::Result<()> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(to_csv(r).as_bytes());
    std::fs::write(path, bytes)
}

/// The aircraft report's CSV columns.
pub const AIRCRAFT_CSV_HEADER: &str =
    "Table,Name,Photos,Registrations,Airlines,Types,First seen,Last seen,Built";

fn aircraft_line(out: &mut String, table: &str, r: &AircraftRow) {
    out.push_str(&format!(
        "{},{},{},{},{},{},{},{},{}\r\n",
        field(table),
        field(&r.name),
        r.photos,
        r.registrations,
        field(&r.airlines.join("; ")),
        field(&r.types.join("; ")),
        r.first.as_deref().unwrap_or_default(),
        r.last.as_deref().unwrap_or_default(),
        r.built.as_deref().unwrap_or_default(),
    ));
}

/// The aircraft report: the totals as name–value lines, then airlines,
/// types and registrations, then the drill-down (CRLF line ends).
pub fn aircraft_to_csv(r: &AircraftReport) -> String {
    let t = &r.totals;
    let mut out = String::new();
    out.push_str(AIRCRAFT_CSV_HEADER);
    out.push_str("\r\n");
    for (name, n) in [
        ("Aircraft photos", t.photos),
        ("Distinct shots", t.shots),
        ("With airline, type or registration", t.with_marks),
        ("With an airline", t.with_airline),
        ("With a type", t.with_type),
        ("With a registration", t.with_registration),
        ("Airlines", t.airlines),
        ("Types", t.types),
        ("Registrations", t.registrations),
        ("Reads waiting for review", t.waiting_review),
    ] {
        out.push_str(&format!("Total,{},{n},,,,,,\r\n", field(name)));
    }
    for (table, rows) in [
        ("Airline", &r.airlines),
        ("Type", &r.types),
        ("Registration", &r.registrations),
    ] {
        for row in rows {
            aircraft_line(&mut out, table, row);
        }
    }
    // The drill-down, one line per airline, type and registration together
    // (a blank where none is named).
    for a in &r.by_airline {
        for t in &a.children {
            for reg in &t.children {
                out.push_str(&format!(
                    "Airline > Type > Registration,{},{},,{},{},,,\r\n",
                    field(reg.name.as_deref().unwrap_or_default()),
                    reg.photos,
                    field(a.name.as_deref().unwrap_or_default()),
                    field(t.name.as_deref().unwrap_or_default()),
                ));
            }
        }
    }
    out
}

/// Write [`aircraft_to_csv`] to `path` (UTF-8 with a byte-order mark).
pub fn write_aircraft_csv(r: &AircraftReport, path: &Path) -> std::io::Result<()> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(aircraft_to_csv(r).as_bytes());
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_the_app() {
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_500), "2 KB");
        assert_eq!(human_bytes(2_500_000), "2.5 MB");
        assert_eq!(human_bytes(600_000_000_000), "600.0 GB");
        assert_eq!(human_bytes(1_234_000_000_000), "1.23 TB");
    }

    #[test]
    fn fields_are_quoted_and_formulas_defused() {
        assert_eq!(field("ILCE-7M4"), "ILCE-7M4");
        assert_eq!(field("FE 24-70mm, F2.8"), "\"FE 24-70mm, F2.8\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("=cmd()"), "'=cmd()");
    }
}
