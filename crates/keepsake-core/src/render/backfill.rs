//! Thumbnail backfill: make every missing thumbnail in the library, in
//! batches, from where the last run stopped.
//!
//! - The walk follows `assets.seq` (insertion order), so it is stable while
//!   the app adds assets: new ones come after the saved position.
//! - Each batch (100 assets by default) is rendered first: thumbnail files
//!   go into the rendition cache but nothing is written to the catalog.
//!   Then one short transaction records the batch's renditions and the new
//!   position (`library_meta` key [`CURSOR_KEY`]). A run that is killed
//!   loses at most the batch in progress, whose files are made again (and
//!   replaced) next time.
//! - One bad file never stops the run: it is reported and passed. A file
//!   whose media is offline is passed too, except that a batch where every
//!   asset is offline (the network share is down, say) stops the run before
//!   its position is saved, unless [`BackfillOptions::skip_offline`].
//! - Only the two thumbnails ([`THUMBS`]) are made, never the preview or a
//!   proxy: a camera JPEG's built-in preview is used when it has one, so on
//!   a network share only a few hundred KB of each file are read.
//! - Ctrl-C (the cancel flag) stops after the files being rendered and
//!   saves what was finished.
//! - Once the walk reaches the end, the position stays there: the next run
//!   only looks at assets added since. [`BackfillOptions::restart`] walks
//!   the whole library again, which retries files that failed or were
//!   offline (assets with thumbnails are passed quickly).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::source::{locate, source_hash};
use super::{still, Cache, RenderError, RenderTools, Variant};
use crate::catalog::{AssetId, Catalog, CatalogError, Rendition};
use crate::library::Library;
use crate::probe::ExifTool;

/// Where the walk is: the `seq` of the last asset whose batch was committed.
pub const CURSOR_KEY: &str = "render.backfill.cursor";
/// Assets per batch (and per catalog transaction).
pub const DEFAULT_BATCH: usize = 100;
/// The renditions the backfill makes.
pub const THUMBS: [Variant; 2] = [Variant::Thumb1024, Variant::Thumb256];

/// How long a batch's commit keeps trying while another writer (the app)
/// holds the catalog.
const COMMIT_PATIENCE: Duration = Duration::from_secs(120);
const BUSY_RETRY: Duration = Duration::from_millis(500);

/// How the backfill runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillOptions {
    /// Assets per batch; each batch is committed in one transaction.
    pub batch_size: usize,
    /// Assets rendered at the same time.
    pub workers: usize,
    /// Stop after this many committed batches (`None`: the whole library).
    pub max_batches: Option<u64>,
    /// Forget the saved position and walk the whole library again.
    pub restart: bool,
    /// Move past batches whose media is all offline instead of stopping.
    pub skip_offline: bool,
}

impl Default for BackfillOptions {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH,
            workers: 1,
            max_batches: None,
            restart: false,
            skip_offline: false,
        }
    }
}

/// Progress, delivered on the calling thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BackfillEvent {
    /// The walk starts after asset `from_seq`; `remaining` assets follow it.
    Started { from_seq: i64, remaining: u64 },
    /// An asset whose thumbnails could not be made (it is passed).
    ItemFailed {
        asset: AssetId,
        seq: i64,
        error: String,
    },
    /// A batch and the position after it are saved.
    BatchCommitted { batch: u64, stats: BatchStats },
    /// Every asset of the batch starting after `cursor` is offline: stopped
    /// without moving past them.
    StoppedOffline { cursor: i64, items: u64 },
    /// The walk reached the last asset.
    Complete,
}

/// What happened to the assets of one batch (or a whole run).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct BatchStats {
    /// Assets looked at.
    pub items: u64,
    /// Assets that got at least one thumbnail.
    pub made: u64,
    /// Assets that already had both.
    pub already: u64,
    /// Assets with no original file in the catalog (nothing to render).
    pub no_original: u64,
    /// Assets whose media is not on a mounted volume.
    pub offline: u64,
    pub failed: u64,
    /// Position after this batch.
    pub cursor: i64,
}

impl BatchStats {
    fn add(&mut self, b: &BatchStats) {
        self.items += b.items;
        self.made += b.made;
        self.already += b.already;
        self.no_original += b.no_original;
        self.offline += b.offline;
        self.failed += b.failed;
        self.cursor = b.cursor;
    }

    fn all_offline(&self) -> bool {
        self.items > 0 && self.offline == self.items
    }
}

/// The result of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BackfillReport {
    /// Batches committed by this run.
    pub batches: u64,
    /// Totals of the committed batches; `cursor` is the saved position.
    pub totals: BatchStats,
    /// The walk reached the last asset.
    pub complete: bool,
    pub cancelled: bool,
    /// Stopped at a batch whose media is all offline.
    pub stopped_offline: bool,
}

/// What rendering one asset came to (errors are [`RenderError`]s).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    /// Rendition rows to record (their files are in the cache).
    Made(Vec<Rendition>),
    /// Both thumbnails exist.
    Already,
    /// The asset has no original file.
    NoOriginal,
}

/// Renders one asset: `(reader, asset)`. Replaces the real renderer in tests.
pub type ItemRenderer =
    Arc<dyn Fn(&Catalog, AssetId) -> Result<ItemOutcome, RenderError> + Send + Sync>;

/// The saved position (0: from the start).
pub fn cursor(cat: &Catalog) -> Result<i64, CatalogError> {
    let v = cat.read(|tx| tx.meta().get(CURSOR_KEY))?;
    Ok(v.and_then(|s| s.trim().parse().ok()).unwrap_or(0))
}

/// Make the missing thumbnails of one asset. Writes rendition files, never
/// the catalog (the caller records the returned rows).
pub fn render_item(
    cat: &Catalog,
    lib: &Library,
    cache: &Cache,
    tools: &RenderTools,
    exiftool: Option<&ExifTool>,
    asset: AssetId,
) -> Result<ItemOutcome, RenderError> {
    let Some(hash) = source_hash(cat, asset)? else {
        return Ok(ItemOutcome::NoOriginal);
    };
    let mut missing = Vec::new();
    for v in THUMBS {
        if !cache.has(cat, &hash, v)? {
            missing.push(v);
        }
    }
    if missing.is_empty() {
        return Ok(ItemOutcome::Already);
    }
    let src = locate(cat, lib, asset)?;
    let made = still::make_stills(cache, &src, tools, exiftool, &missing)?;
    Ok(ItemOutcome::Made(
        made.into_iter().map(|(_, _, r)| r).collect(),
    ))
}

/// Result of one asset inside a batch.
enum Done {
    Ok(ItemOutcome),
    Offline,
    Failed(String),
}

/// Walks the library making missing thumbnails.
pub struct Backfill {
    lib: Library,
    cache: Cache,
    tools: RenderTools,
    opts: BackfillOptions,
    renderer: Option<ItemRenderer>,
}

impl Backfill {
    pub fn new(lib: &Library, cache: Cache, tools: RenderTools, opts: BackfillOptions) -> Self {
        Self {
            lib: lib.clone(),
            cache,
            tools,
            opts,
            renderer: None,
        }
    }

    /// Use `r` instead of [`render_item`] (tests).
    pub fn with_renderer(mut self, r: ItemRenderer) -> Self {
        self.renderer = Some(r);
        self
    }

    /// Run until the end of the library, `max_batches`, a fully offline
    /// batch, or `cancel`. Errors are the catalog's (or the cache folder's)
    /// and stop the run; the position of the last committed batch stays.
    pub fn run(
        &self,
        cancel: &AtomicBool,
        events: &mut dyn FnMut(&BackfillEvent),
    ) -> Result<BackfillReport, RenderError> {
        let batch_size = self.opts.batch_size.max(1);
        let workers = self.opts.workers.clamp(1, batch_size);
        let mut writer = self.lib.open_catalog()?;
        if self.opts.restart {
            commit(&mut writer, &[], 0)?;
        }
        let mut pos = cursor(&writer)?;
        let remaining = writer.read(|tx| tx.assets().count_after(pos))?;
        events(&BackfillEvent::Started {
            from_seq: pos,
            remaining,
        });
        std::fs::create_dir_all(self.cache.root())?;
        self.cache.clear_temp();
        let exiftool = if self.renderer.is_none() {
            ExifTool::start(&self.tools.exiftool, Duration::from_secs(30)).ok()
        } else {
            None
        };
        let mut readers = (0..workers)
            .map(|_| self.lib.open_catalog())
            .collect::<Result<Vec<_>, _>>()?;
        let mut report = BackfillReport {
            totals: BatchStats {
                cursor: pos,
                ..BatchStats::default()
            },
            ..BackfillReport::default()
        };
        loop {
            if cancel.load(Ordering::SeqCst) {
                report.cancelled = true;
                break;
            }
            if self.opts.max_batches.is_some_and(|m| report.batches >= m) {
                break;
            }
            let page = writer.read(|tx| tx.assets().page_after(pos, batch_size))?;
            if page.is_empty() {
                report.complete = true;
                events(&BackfillEvent::Complete);
                break;
            }
            let results = self.render_batch(&page, &mut readers, exiftool.as_ref(), cancel)?;
            // Workers stop early on cancel; what they finished is a prefix.
            let mut stats = BatchStats {
                cursor: pos,
                ..BatchStats::default()
            };
            let mut rows = Vec::new();
            for ((seq, asset), done) in page.iter().zip(results) {
                let Some(done) = done else { break };
                stats.items += 1;
                stats.cursor = *seq;
                match done {
                    Done::Ok(ItemOutcome::Made(r)) => {
                        stats.made += 1;
                        rows.extend(r);
                    }
                    Done::Ok(ItemOutcome::Already) => stats.already += 1,
                    Done::Ok(ItemOutcome::NoOriginal) => stats.no_original += 1,
                    Done::Offline => stats.offline += 1,
                    Done::Failed(error) => {
                        stats.failed += 1;
                        tracing::warn!(seq, asset = %asset, "thumbnail failed: {error}");
                        events(&BackfillEvent::ItemFailed {
                            asset: *asset,
                            seq: *seq,
                            error,
                        });
                    }
                }
            }
            if stats.items == 0 {
                report.cancelled = true;
                break;
            }
            if stats.all_offline() && !self.opts.skip_offline {
                // Offline items wrote nothing: there is nothing to save.
                report.stopped_offline = true;
                events(&BackfillEvent::StoppedOffline {
                    cursor: pos,
                    items: stats.items,
                });
                break;
            }
            commit(&mut writer, &rows, stats.cursor)?;
            pos = stats.cursor;
            report.batches += 1;
            report.totals.add(&stats);
            events(&BackfillEvent::BatchCommitted {
                batch: report.batches,
                stats,
            });
        }
        Ok(report)
    }

    /// Render a batch with up to `readers.len()` threads. `None`: not
    /// started (cancelled). Started items always finish, so the finished
    /// ones are a prefix of `page`.
    fn render_batch(
        &self,
        page: &[(i64, AssetId)],
        readers: &mut [Catalog],
        exiftool: Option<&ExifTool>,
        cancel: &AtomicBool,
    ) -> Result<Vec<Option<Done>>, RenderError> {
        let next = AtomicUsize::new(0);
        let fatal: Mutex<Option<RenderError>> = Mutex::new(None);
        let slots: Vec<Mutex<Option<Done>>> = page.iter().map(|_| Mutex::new(None)).collect();
        std::thread::scope(|s| {
            for cat in readers.iter_mut() {
                let (next, fatal, slots) = (&next, &fatal, &slots);
                s.spawn(move || loop {
                    if cancel.load(Ordering::SeqCst) || has_fatal(fatal) {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some((_, asset)) = page.get(i) else {
                        return;
                    };
                    let done = match self.render_one(cat, *asset, exiftool) {
                        Ok(d) => d,
                        Err(e) => {
                            if let Ok(mut f) = fatal.lock() {
                                f.get_or_insert(e);
                            }
                            return;
                        }
                    };
                    if let Some(slot) = slots.get(i) {
                        if let Ok(mut g) = slot.lock() {
                            *g = Some(done);
                        }
                    }
                });
            }
        });
        if let Some(e) = fatal.into_inner().ok().flatten() {
            return Err(e);
        }
        Ok(slots
            .into_iter()
            .map(|m| m.into_inner().ok().flatten())
            .collect())
    }

    /// One asset. `Err`: the run must stop (the catalog failed).
    fn render_one(
        &self,
        cat: &Catalog,
        asset: AssetId,
        exiftool: Option<&ExifTool>,
    ) -> Result<Done, RenderError> {
        let r = catch_unwind(AssertUnwindSafe(|| match &self.renderer {
            Some(f) => f(cat, asset),
            None => render_item(cat, &self.lib, &self.cache, &self.tools, exiftool, asset),
        }));
        Ok(match r {
            Ok(Ok(o)) => Done::Ok(o),
            Ok(Err(RenderError::Offline)) => Done::Offline,
            // Busy reads are not the file's fault, but a later pass
            // (`restart`) retries it like any failure.
            Ok(Err(e)) if is_fatal(&e) => return Err(e),
            Ok(Err(e)) => Done::Failed(e.to_string()),
            Err(p) => Done::Failed(format!("the renderer crashed: {}", panic_text(&p))),
        })
    }
}

fn has_fatal(f: &Mutex<Option<RenderError>>) -> bool {
    f.lock().map(|g| g.is_some()).unwrap_or(true)
}

/// A catalog error other than "busy": every later asset would fail too.
fn is_fatal(e: &RenderError) -> bool {
    let cat = match e {
        RenderError::Catalog(c)
        | RenderError::Library(crate::library::LibraryError::Catalog(c)) => c,
        _ => return false,
    };
    !cat.is_busy()
}

fn panic_text(p: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".to_owned()
    }
}

/// Record a batch's renditions and the position after it in one
/// transaction, waiting (a while) for other writers.
fn commit(cat: &mut Catalog, rows: &[Rendition], cursor: i64) -> Result<(), RenderError> {
    let start = Instant::now();
    let value = cursor.to_string();
    loop {
        let r = cat.write(|tx| {
            for row in rows {
                tx.renditions().upsert(row)?;
            }
            tx.meta().set(CURSOR_KEY, &value)
        });
        match r {
            Err(e) if e.is_busy() && start.elapsed() < COMMIT_PATIENCE => {
                tracing::warn!("catalog busy; saving the thumbnail batch again shortly");
                std::thread::sleep(BUSY_RETRY);
            }
            r => return Ok(r?),
        }
    }
}
