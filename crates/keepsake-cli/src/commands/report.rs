//! `keepsake report [--by year|month|day|camera|lens|type|drive] [--csv PATH]`

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};

use keepsake_core::catalog::ReportRow;
use keepsake_core::report::{human_bytes, write_csv};

use super::common::{path_str, Ctx, EXIT_OK};

#[derive(Clone, Copy, ValueEnum)]
pub enum By {
    Year,
    Month,
    Day,
    Camera,
    Lens,
    Type,
    Drive,
}

#[derive(Args)]
pub struct ReportArgs {
    /// Which breakdown to print (the CSV and --json have all of them).
    #[arg(long, value_enum, default_value_t = By::Year)]
    pub by: By,
    /// Also write every breakdown to this CSV file.
    #[arg(long, value_name = "PATH")]
    pub csv: Option<PathBuf>,
}

fn n(v: Option<u64>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
}

fn print_row(name: &str, r: &ReportRow) {
    println!(
        "{name:<32} {:>9} {:>9} {:>9} {:>9} {:>11}",
        n(r.photos),
        n(r.videos),
        r.items,
        r.files,
        human_bytes(r.bytes)
    );
}

pub fn run(ctx: &Ctx, a: ReportArgs) -> Result<u8> {
    let lib = ctx.library()?;
    let cat = lib.open_catalog()?;
    let r = cat.read(|tx| tx.report().library())?;
    if let Some(p) = &a.csv {
        write_csv(&r, p).with_context(|| format!("could not write {}", p.display()))?;
    }
    ctx.emit(
        &serde_json::json!({
            "report": r,
            "csv": a.csv.as_deref().map(path_str),
        }),
        || {
            let (title, rows): (&str, Vec<(String, &ReportRow)>) = match a.by {
                By::Year => (
                    "Year",
                    r.years.iter().map(|x| (x.label().into(), x)).collect(),
                ),
                By::Month => (
                    "Month",
                    r.months.iter().map(|x| (x.label().into(), x)).collect(),
                ),
                By::Day => (
                    "Day",
                    r.days.iter().map(|x| (x.label().into(), x)).collect(),
                ),
                By::Camera => (
                    "Camera",
                    r.cameras.iter().map(|x| (x.label().into(), x)).collect(),
                ),
                By::Lens => (
                    "Lens",
                    r.lenses.iter().map(|x| (x.label().into(), x)).collect(),
                ),
                By::Type => (
                    "File type",
                    r.file_types.iter().map(|x| (x.label().into(), x)).collect(),
                ),
                By::Drive => (
                    "Drive (every copy)",
                    r.drives
                        .iter()
                        .map(|d| {
                            let name = if d.online {
                                d.row.label().to_owned()
                            } else {
                                format!("{} (unplugged)", d.row.label())
                            };
                            (name, &d.row)
                        })
                        .collect(),
                ),
            };
            println!(
                "{title:<32} {:>9} {:>9} {:>9} {:>9} {:>11}",
                "Photos", "Videos", "Items", "Files", "Size"
            );
            for (name, row) in &rows {
                print_row(name, row);
            }
            print_row("Total", &r.totals);
            if let Some(p) = &a.csv {
                println!("\nWrote {}", path_str(p));
            }
        },
    )?;
    Ok(EXIT_OK)
}
