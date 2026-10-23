//! `keepsake library …` — create a library, index folders, rebuild, edit metadata.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Subcommand;

use keepsake_core::catalog::{Catalog, Rating, TagPath, TagSource, VolumeKind};
use keepsake_core::library::Library;
use keepsake_core::metadata::MetadataService;
use keepsake_core::rebuild::{add_volume, rebuild_library, RebuildReport};

#[derive(Subcommand)]
pub enum LibraryAction {
    /// Create a new, empty library folder.
    Init {
        /// Library folder to create.
        library: PathBuf,
    },

    /// Register a folder (card copy, drive, backup) as a volume and index it.
    AddVolume {
        library: PathBuf,
        /// Root folder of the volume.
        mount: PathBuf,
        /// Human-readable name.
        #[arg(long)]
        label: String,
        /// card, internal, external or network.
        #[arg(long, default_value = "external")]
        kind: String,
        /// Treat the volume as read-only: sidecars go to the library mirror.
        #[arg(long)]
        read_only: bool,
    },

    /// Rebuild the catalog from files and sidecars on every registered volume.
    Rebuild {
        library: PathBuf,
        /// Delete the catalog database first and rebuild from scratch.
        #[arg(long)]
        fresh: bool,
    },

    /// Print counts, calendar days, events and tags.
    Stats { library: PathBuf },

    /// Full-text search over file names, comments and tags.
    Search { library: PathBuf, query: String },

    /// Edit an asset's metadata, addressed by the path of any of its files.
    /// Changes are written to the catalog and to the XMP sidecar.
    Edit {
        library: PathBuf,
        /// Path of a catalogued file.
        file: PathBuf,
        /// Star rating, 0-5.
        #[arg(long)]
        rating: Option<u8>,
        /// Colour label (Red, Yellow, Green, Blue, Purple or any name); "" clears it.
        #[arg(long)]
        label: Option<String>,
        /// Comment; "" clears it.
        #[arg(long)]
        comment: Option<String>,
        /// Replace user tags. Use `|` for hierarchy, e.g. --tag "Travel|Japan". Repeatable.
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Shift the capture time by this many seconds (negative = earlier).
        #[arg(long, allow_hyphen_values = true)]
        shift_seconds: Option<i64>,
    },
}

pub fn run(action: LibraryAction) -> Result<()> {
    match action {
        LibraryAction::Init { library } => {
            let lib = Library::create(&library)?;
            lib.open_catalog()?;
            println!("Created library {} at {}", lib.id(), library.display());
        }
        LibraryAction::AddVolume {
            library,
            mount,
            label,
            kind,
            read_only,
        } => {
            let lib = open(&library)?;
            let mut cat = lib.open_catalog()?;
            let kind = VolumeKind::parse(&kind).with_context(|| {
                format!("unknown kind {kind:?}; use card, internal, external or network")
            })?;
            let (rec, report) = add_volume(&mut cat, &lib, &label, kind, &mount, read_only)?;
            println!(
                "Volume {} ({}) - {}, {}",
                rec.label,
                rec.id,
                if rec.read_only {
                    "read-only"
                } else {
                    "writable"
                },
                if rec.case.is_sensitive() {
                    "case-sensitive"
                } else {
                    "case-insensitive"
                }
            );
            print_report(&report);
        }
        LibraryAction::Rebuild { library, fresh } => {
            let lib = open(&library)?;
            if fresh {
                Catalog::delete_files(&lib.catalog_path())?;
                println!("Deleted catalog; rebuilding from files and sidecars.");
            }
            let mut cat = lib.open_catalog()?;
            for (rec, report) in rebuild_library(&mut cat, &lib)? {
                match report {
                    Some(r) => {
                        println!("Volume {}:", rec.label);
                        print_report(&r);
                    }
                    None => println!("Volume {}: offline, skipped", rec.label),
                }
            }
        }
        LibraryAction::Stats { library } => {
            let lib = open(&library)?;
            let cat = lib.open_catalog()?;
            cat.read(|tx| -> Result<()> {
                println!("Assets: {}", tx.assets().count()?);
                println!("Files:  {}", tx.files().count()?);
                println!("Volumes:");
                for v in tx.volumes().list()? {
                    println!(
                        "  {} ({}) {}",
                        v.label,
                        v.id,
                        if v.online { "online" } else { "offline" }
                    );
                }
                println!("Days:");
                for d in tx.assets().days()? {
                    println!("  {}  {}", d.date, d.count);
                }
                println!("Events:");
                for e in tx.events().list()? {
                    let n = tx.events().span(e.id)?.map(|s| s.assets).unwrap_or(0);
                    println!("  {} ({n} assets)", e.title);
                }
                println!("Tags:");
                for t in tx.tags().all_paths()? {
                    println!("  {t}");
                }
                Ok(())
            })?;
        }
        LibraryAction::Search { library, query } => {
            let lib = open(&library)?;
            let cat = lib.open_catalog()?;
            let ids = cat.read(|tx| tx.search().text(&query, 100))?;
            if ids.is_empty() {
                println!("No matches.");
            }
            for id in ids {
                let (a, files, tags) = cat.read(|tx| -> Result<_> {
                    Ok((
                        tx.assets().require(id)?,
                        tx.files().originals(id)?,
                        tx.tags().for_asset(id)?,
                    ))
                })?;
                let path = files
                    .first()
                    .map(|f| f.rel_path.to_string())
                    .unwrap_or_default();
                let tags: Vec<String> = tags
                    .iter()
                    .filter(|t| t.source == TagSource::User)
                    .map(|t| t.path.to_string())
                    .collect();
                println!(
                    "{path}  rating={} label={} tags=[{}] comment={:?}",
                    a.rating.get(),
                    a.label.as_deref().unwrap_or("-"),
                    tags.join(", "),
                    a.comment.as_deref().unwrap_or("")
                );
            }
        }
        LibraryAction::Edit {
            library,
            file,
            rating,
            label,
            comment,
            tags,
            shift_seconds,
        } => {
            let lib = open(&library)?;
            let mut cat = lib.open_catalog()?;
            let f = lib.locate(&cat, &file)?.with_context(|| {
                format!(
                    "{} is not in the catalog (is its volume added?)",
                    file.display()
                )
            })?;
            let mut svc = MetadataService::new(&lib, &mut cat);
            let id = f.asset_id;
            let mut changed = false;
            if let Some(r) = rating {
                svc.set_rating(id, Rating::new(r)?)?;
                changed = true;
            }
            if let Some(l) = &label {
                svc.set_label(id, Some(l.as_str()))?;
                changed = true;
            }
            if let Some(c) = &comment {
                svc.set_comment(id, Some(c.as_str()))?;
                changed = true;
            }
            if !tags.is_empty() {
                let paths = tags
                    .iter()
                    .map(|t| TagPath::parse_lightroom(t))
                    .collect::<Result<Vec<_>, _>>()?;
                svc.set_user_tags(id, &paths)?;
                changed = true;
            }
            if let Some(s) = shift_seconds {
                svc.shift_capture_time(id, s)?;
                changed = true;
            }
            if !changed {
                bail!("nothing to change; pass --rating, --label, --comment, --tag or --shift-seconds");
            }
            let sidecars = cat.read(|tx| tx.sidecars().for_asset(id))?;
            println!("Updated asset {id}. Sidecars:");
            for s in sidecars {
                println!("  {} ({})", s.rel_path, s.location);
            }
        }
    }
    Ok(())
}

fn open(library: &Path) -> Result<Library> {
    Library::open(library)
        .with_context(|| format!("{} is not a Keepsake library", library.display()))
}

fn print_report(r: &RebuildReport) {
    println!(
        "  {} assets, {} files indexed, {} sidecars read, {} hidden/system files ignored",
        r.assets, r.files_indexed, r.sidecars_used, r.hidden_skipped
    );
    for s in &r.skipped {
        println!("  skipped {}: {}", s.path, s.reason);
    }
    for p in &r.problems {
        println!("  problem: {p}");
    }
}
