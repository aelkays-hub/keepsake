use rusqlite::{params, OptionalExtension, Row};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, Event, EventId, Rw};

define_repo!(
    /// Events (time-and-place clusters).
    Events
);

/// First and last capture time of an event's members, and how many there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSpan {
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub assets: u64,
}

const COLS: &str = "id, title, place_name, lat, lon, created_ms";

fn map(r: &Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        id: r.get(0)?,
        title: r.get(1)?,
        place_name: r.get(2)?,
        lat: r.get(3)?,
        lon: r.get(4)?,
        created_ms: r.get(5)?,
    })
}

impl<M> Events<'_, M> {
    pub fn get(&self, id: EventId) -> Result<Option<Event>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM events WHERE id = ?1"),
                [id],
                map,
            )
            .optional()?)
    }

    pub fn list(&self) -> Result<Vec<Event>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM events ORDER BY created_ms, id"
        ))?;
        let rows = st.query_map([], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Time span and size of an event, from member capture times.
    pub fn span(&self, id: EventId) -> Result<Option<EventSpan>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT start_ms, end_ms, n FROM event_spans WHERE event_id = ?1",
                [id],
                |r| {
                    Ok(EventSpan {
                        start_ms: r.get(0)?,
                        end_ms: r.get(1)?,
                        assets: super::u64_col(r, 2)?,
                    })
                },
            )
            .optional()?)
    }
}

impl Events<'_, Rw> {
    pub fn upsert(&self, e: &Event) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO events (id, title, place_name, lat, lon, created_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title, place_name = excluded.place_name,
               lat = excluded.lat, lon = excluded.lon, created_ms = excluded.created_ms",
            params![e.id, e.title, e.place_name, e.lat, e.lon, e.created_ms],
        )?;
        Ok(())
    }

    /// Put assets into an event (or take them out with `None`).
    pub fn assign(&self, assets: &[AssetId], event: Option<EventId>) -> Result<(), CatalogError> {
        let mut st = self
            .conn
            .prepare("UPDATE assets SET event_id = ?2 WHERE id = ?1")?;
        for a in assets {
            if st.execute(params![a, event])? == 0 {
                return Err(CatalogError::not_found("asset", a));
            }
        }
        Ok(())
    }

    /// Delete an event; member assets keep existing with no event.
    pub fn delete(&self, id: EventId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM events WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Delete events with no member assets. Returns how many.
    pub fn delete_empty(&self) -> Result<u64, CatalogError> {
        let n = self.conn.execute(
            "DELETE FROM events WHERE NOT EXISTS (SELECT 1 FROM assets a WHERE a.event_id = events.id)",
            [],
        )?;
        Ok(n as u64)
    }
}
