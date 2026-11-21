//! Report → Flickr (ADR-046 addition): what is on the user's Flickr
//! account, read from Flickr itself (photos uploaded with Keepsake or any
//! other way), as a tree by upload date: year › month › day, with counts.
//!
//! - [`list`] pages through `flickr.people.getPhotos` (`user_id=me`, 500 a
//!   page, Flickr's most), with the upload date, the date taken, who may
//!   see each photo and Flickr's 150-pixel thumbnail (`url_q`). The signed-in
//!   token sees private photos too. 10,000 photos is 20 requests (Flickr
//!   allows 3,600 an hour).
//! - Nothing is kept in the catalog: the app holds the list in memory and
//!   reads it again when asked.
//! - [`tree`] groups the photos by upload date in a time zone (the
//!   computer's, in the app), newest first.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::TimeZone;
use serde_json::Value;

use super::client::{photo_url, Account, App, Flickr};
use super::FlickrError;

/// Photos per request (Flickr's most).
pub const PER_PAGE: u32 = 500;
/// Most pages read (1,000,000 photos): a guard against a server that
/// never says it is done.
const MAX_PAGES: u32 = 2000;

/// One photo on the account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountPhoto {
    pub id: String,
    pub title: String,
    /// When it was uploaded (Unix seconds).
    pub uploaded_s: i64,
    /// When it was taken, as Flickr has it (`2024-06-15 14:02:11`).
    pub taken: Option<String>,
    /// `public`, `friends_family`, `friends`, `family` or `private`.
    pub privacy: String,
    /// Flickr's 150-pixel square thumbnail.
    pub thumb: Option<String>,
    /// Its Flickr page.
    pub url: String,
}

/// A JSON number or a number in a string (Flickr gives both).
fn num(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn privacy(p: &Value) -> &'static str {
    let on = |k: &str| num(&p[k]).unwrap_or(0) != 0;
    match (on("ispublic"), on("isfriend"), on("isfamily")) {
        (true, _, _) => "public",
        (false, true, true) => "friends_family",
        (false, true, false) => "friends",
        (false, false, true) => "family",
        _ => "private",
    }
}

/// One page of `flickr.people.getPhotos`: the photos, and how many pages
/// and photos there are.
pub fn page(
    client: &Flickr,
    app: &App,
    account: &Account,
    n: u32,
) -> Result<(Vec<AccountPhoto>, u32, u64), FlickrError> {
    let (per, pg) = (PER_PAGE.to_string(), n.to_string());
    let v = client.rest(
        app,
        account,
        "flickr.people.getPhotos",
        &[
            ("user_id", "me"),
            ("extras", "date_upload,date_taken,url_q"),
            ("per_page", &per),
            ("page", &pg),
        ],
    )?;
    let p = &v["photos"];
    let pages = num(&p["pages"])
        .and_then(|x| u32::try_from(x).ok())
        .unwrap_or(0);
    let total = num(&p["total"])
        .and_then(|x| u64::try_from(x).ok())
        .unwrap_or(0);
    let list = p["photo"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(list.len());
    for ph in &list {
        let Some(id) = ph["id"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        out.push(AccountPhoto {
            id: id.to_owned(),
            title: ph["title"].as_str().unwrap_or("").to_owned(),
            uploaded_s: num(&ph["dateupload"]).unwrap_or(0),
            taken: ph["datetaken"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            privacy: privacy(ph).to_owned(),
            thumb: ph["url_q"]
                .as_str()
                .filter(|u| u.starts_with("https://"))
                .map(str::to_owned),
            url: photo_url(&account.nsid, id),
        });
    }
    Ok((out, pages, total))
}

/// Every photo on the account, newest upload first. `progress` gets
/// `(read so far, total)` after each page; `cancel` stops between pages.
pub fn list(
    client: &Flickr,
    app: &App,
    account: &Account,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<Vec<AccountPhoto>, FlickrError> {
    let mut all: Vec<AccountPhoto> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut n = 1;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(FlickrError::Refused("Stopped.".into()));
        }
        let (photos, pages, total) = page(client, app, account, n)?;
        let empty = photos.is_empty();
        // A photo uploaded while reading moves the pages along by one:
        // keep each once.
        all.extend(photos.into_iter().filter(|p| seen.insert(p.id.clone())));
        progress(all.len() as u64, total.max(all.len() as u64));
        if empty || n >= pages || n >= MAX_PAGES {
            break;
        }
        n += 1;
    }
    all.sort_by(|a, b| b.uploaded_s.cmp(&a.uploaded_s).then(b.id.cmp(&a.id)));
    Ok(all)
}

/// A day of the tree: `2024-06-15` and how many photos were uploaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    pub date: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Month {
    /// `2024-06`.
    pub month: String,
    pub count: u64,
    /// Newest first.
    pub days: Vec<Day>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Year {
    pub year: i32,
    pub count: u64,
    /// Newest first.
    pub months: Vec<Month>,
}

/// The upload day of `uploaded_s` in `tz`: `2024-06-15`.
pub fn day_of<Tz: TimeZone>(uploaded_s: i64, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    match tz.timestamp_opt(uploaded_s, 0).single() {
        Some(t) => t.format("%Y-%m-%d").to_string(),
        None => "0000-00-00".into(),
    }
}

/// The photos as a tree by upload date in `tz`, newest first.
pub fn tree<Tz: TimeZone>(photos: &[AccountPhoto], tz: &Tz) -> Vec<Year>
where
    Tz::Offset: std::fmt::Display,
{
    let mut days: BTreeMap<String, u64> = BTreeMap::new();
    for p in photos {
        *days.entry(day_of(p.uploaded_s, tz)).or_default() += 1;
    }
    let mut years: Vec<Year> = Vec::new();
    for (date, count) in days.into_iter().rev() {
        let year: i32 = date.get(..4).and_then(|y| y.parse().ok()).unwrap_or(0);
        let month = date.get(..7).unwrap_or("").to_owned();
        if years.last().is_none_or(|y| y.year != year) {
            years.push(Year {
                year,
                count: 0,
                months: Vec::new(),
            });
        }
        let Some(y) = years.last_mut() else { continue };
        y.count += count;
        if y.months.last().is_none_or(|m| m.month != month) {
            y.months.push(Month {
                month,
                count: 0,
                days: Vec::new(),
            });
        }
        if let Some(m) = y.months.last_mut() {
            m.count += count;
            m.days.push(Day { date, count });
        }
    }
    years
}

/// The photos uploaded on `date` (`2024-06-15`) in `tz`, newest first.
pub fn on_day<'a, Tz: TimeZone>(
    photos: &'a [AccountPhoto],
    date: &str,
    tz: &Tz,
) -> Vec<&'a AccountPhoto>
where
    Tz::Offset: std::fmt::Display,
{
    photos
        .iter()
        .filter(|p| day_of(p.uploaded_s, tz) == date)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};

    fn photo(id: &str, uploaded_s: i64) -> AccountPhoto {
        AccountPhoto {
            id: id.into(),
            title: String::new(),
            uploaded_s,
            taken: None,
            privacy: "public".into(),
            thumb: None,
            url: String::new(),
        }
    }

    #[test]
    fn the_tree_counts_years_months_and_days_newest_first() {
        // 2024-06-15 10:00, 2024-06-15 11:00, 2024-06-02, 2023-12-31 UTC.
        let p = [
            photo("1", 1_718_445_600),
            photo("2", 1_718_449_200),
            photo("3", 1_717_322_400),
            photo("4", 1_704_024_000),
        ];
        let t = tree(&p, &Utc);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].year, t[0].count), (2024, 3));
        assert_eq!(t[0].months.len(), 1);
        assert_eq!(t[0].months[0].month, "2024-06");
        assert_eq!(
            t[0].months[0].days,
            [
                Day {
                    date: "2024-06-15".into(),
                    count: 2
                },
                Day {
                    date: "2024-06-02".into(),
                    count: 1
                }
            ]
        );
        assert_eq!((t[1].year, t[1].count), (2023, 1));
        assert_eq!(on_day(&p, "2024-06-15", &Utc).len(), 2);
        assert!(tree(&[], &Utc).is_empty());
    }

    #[test]
    fn the_day_is_the_time_zones() {
        // 2023-12-31 23:30 UTC is 2023-12-31 18:30 in Toronto (UTC−5) and
        // 2024-01-01 in Tokyo.
        let p = [photo("1", 1_704_065_400)];
        let toronto = FixedOffset::west_opt(5 * 3600).unwrap();
        let tokyo = FixedOffset::east_opt(9 * 3600).unwrap();
        assert_eq!(tree(&p, &toronto)[0].year, 2023);
        assert_eq!(tree(&p, &tokyo)[0].months[0].days[0].date, "2024-01-01");
    }

    #[test]
    fn privacy_comes_from_flickrs_flags() {
        let v = |a: i64, b: i64, c: i64| serde_json::json!({ "ispublic": a, "isfriend": b, "isfamily": c });
        assert_eq!(privacy(&v(1, 0, 0)), "public");
        assert_eq!(privacy(&v(0, 1, 1)), "friends_family");
        assert_eq!(privacy(&v(0, 1, 0)), "friends");
        assert_eq!(privacy(&v(0, 0, 1)), "family");
        assert_eq!(privacy(&v(0, 0, 0)), "private");
        assert_eq!(num(&serde_json::json!("1234")), Some(1234));
    }
}
