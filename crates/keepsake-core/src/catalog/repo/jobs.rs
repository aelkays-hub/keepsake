use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, Rw};

define_repo!(
    /// The persistent job queue (see [`crate::jobs`]).
    Jobs
);

/// Scheduling class; lower runs first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// On screen right now.
    Visible = 0,
    /// From a recent import.
    Recent = 1,
    /// Everything else.
    Backlog = 2,
}

impl Priority {
    pub fn from_i64(v: i64) -> Self {
        match v {
            0 => Priority::Visible,
            1 => Priority::Recent,
            _ => Priority::Backlog,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Visible => "visible",
            Priority::Recent => "recent",
            Priority::Backlog => "backlog",
        }
    }
}

/// State of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "running" => JobState::Running,
            "done" => JobState::Done,
            "failed" => JobState::Failed,
            _ => JobState::Queued,
        }
    }
}

/// One job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub asset_id: AssetId,
    pub priority: Priority,
    pub state: JobState,
    pub attempts: u32,
    pub error: Option<String>,
    pub enqueued_ms: i64,
    pub not_before_ms: i64,
}

/// A job to add: the asset, its rank (cheaper kinds first) and its sort key
/// (lower first; callers use minus the capture time for newest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewJob<'a> {
    pub kind: &'a str,
    pub asset_id: AssetId,
    pub rank: i64,
    pub sort_key: i64,
}

/// Counts by kind and state, plus queued jobs by priority.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct JobCounts {
    /// `kind → state → count`.
    pub by_kind: BTreeMap<String, BTreeMap<String, u64>>,
    /// Queued jobs (all kinds) by priority name.
    pub queued_by_priority: BTreeMap<String, u64>,
}

impl JobCounts {
    pub fn total(&self, state: JobState) -> u64 {
        self.by_kind
            .values()
            .filter_map(|m| m.get(state.as_str()))
            .sum()
    }
}

const COLS: &str =
    "id, kind, asset_id, priority, state, attempts, error, enqueued_ms, not_before_ms";

fn map(r: &Row<'_>) -> rusqlite::Result<Job> {
    let state: String = r.get(4)?;
    let attempts: i64 = r.get(5)?;
    Ok(Job {
        id: r.get(0)?,
        kind: r.get(1)?,
        asset_id: r.get(2)?,
        priority: Priority::from_i64(r.get(3)?),
        state: JobState::parse(&state),
        attempts: u32::try_from(attempts).unwrap_or(0),
        error: r.get(6)?,
        enqueued_ms: r.get(7)?,
        not_before_ms: r.get(8)?,
    })
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

impl<M> Jobs<'_, M> {
    pub fn get(&self, id: i64) -> Result<Option<Job>, CatalogError> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {COLS} FROM jobs WHERE id = ?1"), [id], map)
            .optional()?)
    }

    pub fn find(&self, kind: &str, asset: AssetId) -> Result<Option<Job>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM jobs WHERE kind = ?1 AND asset_id = ?2"),
                params![kind, asset],
                map,
            )
            .optional()?)
    }

    /// Jobs in a state, in claim order.
    pub fn list(&self, state: JobState, limit: u32) -> Result<Vec<Job>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM jobs WHERE state = ?1
             ORDER BY priority, rank, sort_key, id LIMIT ?2"
        ))?;
        let rows = st
            .query_map(params![state.as_str(), limit], map)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn counts(&self) -> Result<JobCounts, CatalogError> {
        let mut out = JobCounts::default();
        let mut st = self
            .conn
            .prepare("SELECT kind, state, count(*) FROM jobs GROUP BY kind, state")?;
        for row in st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (k, s, n) = row?;
            out.by_kind
                .entry(k)
                .or_default()
                .insert(s, u64::try_from(n).unwrap_or(0));
        }
        let mut st = self.conn.prepare(
            "SELECT priority, count(*) FROM jobs WHERE state = 'queued' GROUP BY priority",
        )?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (p, n) = row?;
            out.queued_by_priority.insert(
                Priority::from_i64(p).as_str().to_owned(),
                u64::try_from(n).unwrap_or(0),
            );
        }
        Ok(out)
    }

    /// The next job to run among `kinds` (all kinds if empty), without
    /// claiming it.
    pub fn next(&self, kinds: &[&str], now_ms: i64) -> Result<Option<Job>, CatalogError> {
        let filter = if kinds.is_empty() {
            String::new()
        } else {
            format!(" AND kind IN ({})", placeholders(kinds.len()))
        };
        let sql = format!(
            "SELECT {COLS} FROM jobs WHERE state = 'queued' AND not_before_ms <= ?{filter}
             ORDER BY priority, rank, sort_key, id LIMIT 1"
        );
        let mut args: Vec<&dyn rusqlite::ToSql> = vec![&now_ms];
        for k in kinds {
            args.push(k);
        }
        Ok(self.conn.query_row(&sql, args.as_slice(), map).optional()?)
    }
}

impl Jobs<'_, Rw> {
    /// Add jobs in class `class`, or re-queue existing ones that are done
    /// or failed when `requeue_finished`. A job already queued or running
    /// keeps its place, but moves up if `class` is better than its own.
    /// `Visible` is not a class of its own: new jobs get `Backlog` and
    /// [`Self::set_visible`] raises them. Returns how many rows were added
    /// or re-queued.
    pub fn enqueue(
        &self,
        jobs: &[NewJob<'_>],
        class: Priority,
        now_ms: i64,
        requeue_finished: bool,
    ) -> Result<u64, CatalogError> {
        let visible = class == Priority::Visible;
        let base = if visible { Priority::Backlog } else { class } as i64;
        let mut n = 0u64;
        let mut ins = self.conn.prepare_cached(
            "INSERT INTO jobs (kind, asset_id, priority, base_priority, rank, sort_key, state,
                               enqueued_ms)
             VALUES (?1, ?2, ?3, ?3, ?4, ?5, 'queued', ?6)
             ON CONFLICT(kind, asset_id) DO NOTHING",
        )?;
        let mut better = self.conn.prepare_cached(
            "UPDATE jobs SET base_priority = min(base_priority, ?3),
                             priority = min(priority, ?3)
             WHERE kind = ?1 AND asset_id = ?2 AND state IN ('queued','running')",
        )?;
        let mut again = self.conn.prepare_cached(
            "UPDATE jobs SET state = 'queued', attempts = 0, error = NULL, not_before_ms = 0,
                             priority = min(base_priority, ?3),
                             base_priority = min(base_priority, ?3), enqueued_ms = ?4,
                             started_ms = NULL, finished_ms = NULL
             WHERE kind = ?1 AND asset_id = ?2 AND state IN ('done','failed')",
        )?;
        for j in jobs {
            let added = ins.execute(params![
                j.kind, j.asset_id, base, j.rank, j.sort_key, now_ms
            ])?;
            if added > 0 {
                n += 1;
                continue;
            }
            if !visible {
                better.execute(params![j.kind, j.asset_id, base])?;
            }
            if requeue_finished {
                n += again.execute(params![j.kind, j.asset_id, base, now_ms])? as u64;
            }
        }
        Ok(n)
    }

    /// Make exactly these assets "visible": their queued jobs jump ahead;
    /// jobs that were visible before fall back to their class.
    pub fn set_visible(&self, assets: &[AssetId]) -> Result<u64, CatalogError> {
        self.conn.execute(
            "UPDATE jobs SET priority = base_priority WHERE priority = 0",
            [],
        )?;
        let mut st = self.conn.prepare_cached(
            "UPDATE jobs SET priority = 0 WHERE asset_id = ?1 AND state IN ('queued','running')",
        )?;
        let mut n = 0u64;
        for a in assets {
            n += st.execute([a])? as u64;
        }
        Ok(n)
    }

    /// Claim the next job among `kinds` (all if empty): mark it running and
    /// count the attempt.
    pub fn claim(&self, kinds: &[&str], now_ms: i64) -> Result<Option<Job>, CatalogError> {
        let Some(job) = self.next(kinds, now_ms)? else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE jobs SET state = 'running', attempts = attempts + 1, started_ms = ?2
             WHERE id = ?1 AND state = 'queued'",
            params![job.id, now_ms],
        )?;
        self.get(job.id)
    }

    pub fn complete(&self, id: i64, now_ms: i64) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE jobs SET state = 'done', error = NULL, finished_ms = ?2, priority = base_priority
             WHERE id = ?1",
            params![id, now_ms],
        )?;
        Ok(())
    }

    /// Put a running job back: to `queued` (not before `retry_at_ms`), or
    /// `failed` for good when `retry_at_ms` is `None`.
    pub fn fail(
        &self,
        id: i64,
        error: &str,
        retry_at_ms: Option<i64>,
        now_ms: i64,
    ) -> Result<(), CatalogError> {
        match retry_at_ms {
            Some(at) => self.conn.execute(
                "UPDATE jobs SET state = 'queued', error = ?2, not_before_ms = ?3 WHERE id = ?1",
                params![id, error, at],
            )?,
            None => self.conn.execute(
                "UPDATE jobs SET state = 'failed', error = ?2, finished_ms = ?3 WHERE id = ?1",
                params![id, error, now_ms],
            )?,
        };
        Ok(())
    }

    /// Put a running job back without counting the attempt (e.g. its media
    /// is offline): queued again from `retry_at_ms`.
    pub fn defer(&self, id: i64, reason: &str, retry_at_ms: i64) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE jobs SET state = 'queued', attempts = max(attempts - 1, 0), error = ?2,
                             not_before_ms = ?3 WHERE id = ?1",
            params![id, reason, retry_at_ms],
        )?;
        Ok(())
    }

    /// After a crash or restart: running jobs go back to the queue.
    pub fn recover(&self) -> Result<u64, CatalogError> {
        Ok(self.conn.execute(
            "UPDATE jobs SET state = 'queued', attempts = max(attempts - 1, 0)
             WHERE state = 'running'",
            [],
        )? as u64)
    }

    /// Failed jobs (of `kinds`, all if empty) go back to the queue.
    pub fn retry_failed(&self, kinds: &[&str]) -> Result<u64, CatalogError> {
        let filter = if kinds.is_empty() {
            String::new()
        } else {
            format!(" AND kind IN ({})", placeholders(kinds.len()))
        };
        let args: Vec<&dyn rusqlite::ToSql> =
            kinds.iter().map(|k| k as &dyn rusqlite::ToSql).collect();
        Ok(self.conn.execute(
            &format!(
                "UPDATE jobs SET state = 'queued', attempts = 0, error = NULL, not_before_ms = 0
                 WHERE state = 'failed'{filter}"
            ),
            args.as_slice(),
        )? as u64)
    }
}
