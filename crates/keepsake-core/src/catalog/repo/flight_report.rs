//! The flight report (Tools → Report → Flights): every flight photographed,
//! from the photos' route keywords (`Aircraft|From|YYZ`, `Aircraft|To|…`,
//! `Aircraft|Flight|AC856`, with their countries) and aircraft keywords
//! (`Aircraft|Registration|…`, `Aircraft|Airline|…`, `Aircraft|Type|…`).
//!
//! A flight is the photos of one day (the camera's date) with the same
//! registration, flight number, from and to; a photo with neither from,
//! to nor a flight number is not a flight (counted apart: Add route or
//! Fill in route keywords gives it one).
//!
//! Read-only: one scan of `asset_tags` and one of the photos found.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::aircraft_report::AIRCRAFT_KEYWORDS;
use super::define_repo;
use super::tags::PATHS_CTE;
use crate::catalog::error::CatalogError;
use crate::catalog::AssetId;

define_repo!(
    /// Flight report queries (read-only).
    FlightReportRepo
);

/// One flight photographed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightRow {
    /// The camera's date (`2026-06-07`).
    pub date: Option<String>,
    /// The first photo's time, the camera's (`19:00`).
    pub time: Option<String>,
    pub flight: Option<String>,
    pub from: Option<String>,
    pub from_country: Option<String>,
    pub to: Option<String>,
    pub to_country: Option<String>,
    pub airline: Option<String>,
    pub registration: Option<String>,
    pub aircraft_type: Option<String>,
    pub photos: u64,
    /// The first photo's file name.
    pub first_photo: Option<String>,
    /// Its id (the report opens it).
    pub first_id: AssetId,
}

/// The flight report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightReport {
    /// Newest first.
    pub flights: Vec<FlightRow>,
    /// Photos with a registration but no route keyword.
    pub without_route: u64,
}

/// What one photo's keywords say.
#[derive(Default)]
struct Marks {
    flight: Option<String>,
    from: Option<String>,
    from_country: Option<String>,
    to: Option<String>,
    to_country: Option<String>,
    airline: Option<String>,
    registration: Option<String>,
    aircraft_type: Option<String>,
}

impl Marks {
    fn set(&mut self, kind: &str, name: &str) {
        let slot = match kind {
            "Flight" => &mut self.flight,
            "From" => &mut self.from,
            "From country" => &mut self.from_country,
            "To" => &mut self.to,
            "To country" => &mut self.to_country,
            "Airline" => &mut self.airline,
            "Registration" => &mut self.registration,
            "Type" => &mut self.aircraft_type,
            _ => return,
        };
        // Several: the first in name order, so the report is the same each time.
        if slot.as_deref().is_none_or(|s| name < s) {
            *slot = Some(name.to_owned());
        }
    }

    fn routed(&self) -> bool {
        self.flight.is_some() || self.from.is_some() || self.to.is_some()
    }
}

/// `HH:MM` of a capture time read with its offset (the camera's clock).
fn local_time(ts_ms: i64, offset_min: Option<i64>) -> Option<String> {
    let t = ts_ms + offset_min.unwrap_or(0) * 60_000;
    chrono::DateTime::from_timestamp_millis(t).map(|d| d.format("%H:%M").to_string())
}

impl<M> FlightReportRepo<'_, M> {
    /// Build the flight report.
    pub fn flights(&self) -> Result<FlightReport, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE}
             SELECT DISTINCT at.asset_id, tp.path
               FROM asset_tags at JOIN tag_paths tp ON tp.id = at.tag_id
              WHERE tp.path LIKE '{AIRCRAFT_KEYWORDS}|%|%'"
        ))?;
        let mut marks: HashMap<AssetId, Marks> = HashMap::new();
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let id: AssetId = r.get(0)?;
            let path: String = r.get(1)?;
            let mut parts = path.splitn(3, '|');
            if let (_, Some(kind), Some(name)) = (parts.next(), parts.next(), parts.next()) {
                marks.entry(id).or_default().set(kind, name.trim());
            }
        }
        drop(q);
        let without_route = marks
            .values()
            .filter(|m| !m.routed() && m.registration.is_some())
            .count() as u64;
        marks.retain(|_, m| m.routed());

        let mut st = self.conn.prepare(
            "SELECT capture_local_date, capture_ts_ms, capture_utc_offset_min, primary_name
               FROM assets WHERE id = ?1",
        )?;
        // The flight's key: day, registration, flight, from, to.
        type Key = (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let mut flights: BTreeMap<Key, (FlightRow, Option<i64>)> = BTreeMap::new();
        for (id, m) in marks {
            let Some((date, ts, offset, name)) = st
                .query_row([id], |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                })
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?
            else {
                continue;
            };
            let key: Key = (
                date.clone(),
                m.registration.clone(),
                m.flight.clone(),
                m.from.clone(),
                m.to.clone(),
            );
            let e = flights.entry(key).or_insert_with(|| {
                (
                    FlightRow {
                        date,
                        time: None,
                        flight: m.flight.clone(),
                        from: m.from.clone(),
                        from_country: None,
                        to: m.to.clone(),
                        to_country: None,
                        airline: None,
                        registration: m.registration.clone(),
                        aircraft_type: None,
                        photos: 0,
                        first_photo: None,
                        first_id: id,
                    },
                    None,
                )
            });
            let row = &mut e.0;
            row.photos += 1;
            for (slot, v) in [
                (&mut row.from_country, &m.from_country),
                (&mut row.to_country, &m.to_country),
                (&mut row.airline, &m.airline),
                (&mut row.aircraft_type, &m.aircraft_type),
            ] {
                if slot.is_none() {
                    slot.clone_from(v);
                }
            }
            // The first photo (by capture time; undated last).
            let first = e.1.is_none_or(|t| ts.is_some_and(|ts| ts < t));
            if first && (ts.is_some() || e.1.is_none()) {
                e.1 = ts;
                row.first_id = id;
                row.first_photo = name;
                row.time = ts.and_then(|t| local_time(t, offset));
            }
        }
        let mut rows: Vec<FlightRow> = flights.into_values().map(|(r, _)| r).collect();
        rows.sort_by(|a, b| {
            (b.date.as_deref(), b.time.as_deref()).cmp(&(a.date.as_deref(), a.time.as_deref()))
        });
        Ok(FlightReport {
            flights: rows,
            without_route,
        })
    }
}
