//! Add route for the UI (ADR-044): the setting and the OpenSky API client
//! (kept in the OS's credential store, never in a file or the log),
//! queueing photos, and the window's photos with the flights offered. The
//! work is in `keepsake_core::ai::aircraft::route`; asking runs in the
//! render service's route worker.

use std::sync::Arc;

use keepsake_core::ai::aircraft::route::{
    self, Airports, At, Choice, Clock, OpenSky, Pick, State as RouteState,
};
use keepsake_core::ai::aircraft::scan;
use keepsake_core::catalog::{AssetId, JobState, TagPath};
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{
    CloudStarted, CloudStatusDto, RouteChoiceDto, RouteItemDto, RouteReviewDto, RouteStatusDto,
    Selection,
};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// How Add route reads the camera's clock (the setting).
pub fn clock(s: &Settings) -> Clock {
    if s.ai.aircraft_route_eastern {
        Clock::Eastern
    } else {
        Clock::AsRecorded
    }
}

/// The client the render service uses.
pub fn opensky() -> Arc<dyn route::Flights> {
    Arc::new(OpenSky::new(
        Arc::new(keepsake_core::ai::cloud::UreqHttp::new()),
        Arc::new(|| keepsake_platform::secrets::opensky().ok().flatten()),
    ))
}

#[tauri::command(async)]
pub fn aircraft_route_status() -> CmdResult<RouteStatusDto> {
    let s = Settings::load().map_err(err)?;
    Ok(RouteStatusDto {
        enabled: s.ai.aircraft_route,
        credentials: keepsake_platform::secrets::opensky()
            .map_err(err)?
            .is_some(),
        lookup: s.ai.aircraft_lookup,
        eastern: s.ai.aircraft_route_eastern,
    })
}

fn restart(app: &AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    if state.try_open().is_some() {
        state.reload_settings()?;
        crate::render::restart_service(app)?;
    }
    let _ = app.emit("aircraft://status", ());
    Ok(())
}

/// Turn Find the route online (OpenSky) on or off.
#[tauri::command(async)]
pub fn aircraft_set_route(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.aircraft_route = enabled;
    s.save().map_err(err)?;
    restart(&app)
}

/// The camera's clock showed Eastern time, changed for daylight saving
/// (on), or read the time as the camera recorded it (off).
#[tauri::command(async)]
pub fn aircraft_set_route_eastern(app: AppHandle, eastern: bool) -> CmdResult<()> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.aircraft_route_eastern = eastern;
    s.save().map_err(err)?;
    restart(&app)
}

/// Save the OpenSky API client (both blank: remove it).
#[tauri::command(async)]
pub fn opensky_set_credentials(app: AppHandle, client_id: String, secret: String) -> CmdResult<()> {
    let (id, secret) = (client_id.trim(), secret.trim());
    let client = match (id.is_empty(), secret.is_empty()) {
        (true, true) => None,
        (false, false) => Some((id, secret)),
        _ => return Err("Fill in both the client ID and the secret.".into()),
    };
    keepsake_platform::secrets::set_opensky(client).map_err(err)?;
    restart(&app)
}

/// Ask OpenSky for a token with the saved client: whether it works.
#[tauri::command(async)]
pub fn opensky_test() -> CmdResult<String> {
    let sky = OpenSky::new(
        Arc::new(keepsake_core::ai::cloud::UreqHttp::new()),
        Arc::new(|| keepsake_platform::secrets::opensky().ok().flatten()),
    );
    sky.test().map_err(err)?;
    Ok("OpenSky accepted the client ID and secret.".into())
}

fn dto((s, problem): (keepsake_core::ai::cloud::CloudStatus, Option<String>)) -> CloudStatusDto {
    CloudStatusDto {
        items: s.items,
        analysed: s.done,
        pending: s.pending,
        failed: s.failed,
        problem,
    }
}

/// Find the routes of the selected photos (in the background).
#[tauri::command(async)]
pub fn aircraft_route(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
) -> CmdResult<CloudStarted> {
    let s = Settings::load().map_err(err)?;
    if !s.ai.aircraft_route {
        return Err(
            "Turn on Find the route online (OpenSky) in Settings → Read aircraft first.".into(),
        );
    }
    if keepsake_platform::secrets::opensky()
        .map_err(err)?
        .is_none()
    {
        return Err(
            "Add your OpenSky client ID and secret in Settings → Read aircraft first.".into(),
        );
    }
    let mut ids: Vec<AssetId> = Vec::new();
    for id in crate::edit::resolve(&state, &selection)? {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err("Select the photos first.".into());
    }
    let open = state.open()?;
    {
        let mut cat = open.writer()?;
        // Only those with a registration are asked about.
        let mut ask = Vec::new();
        for id in &ids {
            if route::photo(&cat, *id, clock(&s))
                .map_err(err)?
                .registration
                .is_some()
            {
                ask.push(*id);
            }
        }
        route::enqueue(&mut cat, &ask).map_err(err)?;
    }
    crate::render::start_service(&app)?;
    let cat = open.cat()?;
    Ok(CloudStarted {
        provider: route::SERVICE.to_owned(),
        ids: ids.iter().map(ToString::to_string).collect(),
        status: dto(route::status(&cat, &ids).map_err(err)?),
    })
}

/// Ask again: forget what OpenSky (and the lookup, when it did not know
/// the aircraft) said about `ids`, and ask once more in the background.
#[tauri::command(async)]
pub fn aircraft_route_again(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<u64> {
    let ids = convert::ids(&ids)?;
    let s = Settings::load().map_err(err)?;
    let open = state.open()?;
    let n = {
        let mut cat = open.writer()?;
        route::again(&mut cat, &ids, clock(&s)).map_err(err)?
    };
    crate::render::start_service(&app)?;
    Ok(n)
}

/// Keep the transponder code the user typed for `registration` (blank:
/// forget it).
#[tauri::command(async)]
pub fn aircraft_route_set_code(
    state: State<'_, AppState>,
    registration: String,
    code: Option<String>,
) -> CmdResult<()> {
    let reg = registration.trim();
    if reg.is_empty() {
        return Err("No registration.".into());
    }
    let open = state.open()?;
    let mut cat = open.writer()?;
    keepsake_core::ai::aircraft::lookup::set_typed_code(&mut cat, reg, code.as_deref()).map_err(err)
}

#[tauri::command(async)]
pub fn aircraft_route_stop(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    let ids = convert::ids(&ids)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    route::stop(&mut cat, &ids).map_err(err)?;
    Ok(())
}

fn iso(s: i64) -> String {
    chrono::DateTime::from_timestamp(s, 0)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

fn choice_dto(c: &Choice, airports: &Airports) -> CmdResult<RouteChoiceDto> {
    let end = |code: &Option<String>| {
        code.as_deref().map(|c| {
            let (shown, country) = airports.place(c);
            (shown, c.to_owned(), country)
        })
    };
    let (from, to) = (end(&c.flight.departure), end(&c.flight.arrival));
    Ok(RouteChoiceDto {
        callsign: c.flight.callsign.clone(),
        flight: c.flight.callsign.as_deref().map(route::flight_number),
        from: from.as_ref().map(|f| f.0.clone()),
        from_icao: from.as_ref().map(|f| f.1.clone()),
        from_country: from.and_then(|f| f.2),
        to: to.as_ref().map(|f| f.0.clone()),
        to_icao: to.as_ref().map(|f| f.1.clone()),
        to_country: to.and_then(|f| f.2),
        departed: iso(c.flight.first_seen),
        arrived: iso(c.flight.last_seen),
        at: match c.at {
            At::Departure => "departure",
            At::Arrival => "arrival",
        }
        .to_owned(),
        gap_min: u32::try_from(c.gap_s / 60).unwrap_or(u32::MAX),
        keywords: route::keywords(&c.flight, airports)
            .map_err(err)?
            .iter()
            .map(TagPath::to_lightroom)
            .collect(),
    })
}

/// The Add route window: each photo, and the flights offered for it.
#[tauri::command(async)]
pub fn aircraft_route_review(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<RouteReviewDto> {
    let ids = convert::ids(&ids)?;
    let open = state.open()?;
    let cat = open.cat()?;
    let airports = Airports::builtin();
    let clock = clock(&Settings::load().map_err(err)?);
    let mut items = Vec::new();
    for id in &ids {
        let it = route::review(&cat, &airports, *id, clock).map_err(err)?;
        let job = cat
            .read(|tx| tx.jobs().find(route::JOB_AI_ROUTE, *id))
            .map_err(err)?;
        let error = job.as_ref().and_then(|j| j.error.clone());
        let failed = job.as_ref().is_some_and(|j| j.state == JobState::Failed);
        let mut choices = Vec::new();
        let state = match &it.state {
            RouteState::NoRegistration => "no-registration",
            RouteState::NoCaptureTime => "no-time",
            RouteState::NoTransponder if failed => "failed",
            RouteState::NoTransponder => "no-transponder",
            RouteState::Waiting if failed => "failed",
            RouteState::Waiting => "waiting",
            RouteState::Found(Pick::Nothing) => "nothing",
            RouteState::Found(Pick::NoneNear) => "none-near",
            RouteState::Found(Pick::One(c)) => {
                choices.push(choice_dto(c, &airports)?);
                "found"
            }
            RouteState::Found(Pick::Several(v)) => {
                for c in v {
                    choices.push(choice_dto(c, &airports)?);
                }
                "several"
            }
        };
        let (name, thumb) = crate::aircraft::photo_of(&cat, &open.cache, *id)?;
        let kept = match it.registration.as_deref() {
            Some(r) => keepsake_core::ai::aircraft::lookup::kept(&cat, r).map_err(err)?,
            None => None,
        };
        let code_typed = kept.as_ref().is_some_and(|f| f.typed_mode_s.is_some());
        let code = kept
            .and_then(|f| f.typed_mode_s.or(f.mode_s))
            .map(|c| c.to_ascii_uppercase());
        let zone_set = cat
            .read(|tx| tx.assets().get(*id))
            .map_err(err)?
            .is_some_and(|a| {
                a.capture_source == Some(keepsake_core::catalog::CaptureSource::Sidecar)
            });
        items.push(RouteItemDto {
            id: id.to_string(),
            name,
            thumb,
            registration: it.registration,
            taken: it.taken_s.map(iso),
            local: it
                .taken_s
                .zip(it.offset_min)
                .map(|(t, o)| iso(t + i64::from(o) * 60).trim_end_matches('Z').to_owned()),
            offset_min: it.offset_min,
            zone_set,
            code,
            code_typed,
            state: state.to_owned(),
            error,
            choices,
            has: it.has.iter().map(TagPath::to_lightroom).collect(),
        });
    }
    let (st, _) = route::status(&cat, &ids).map_err(err)?;
    Ok(RouteReviewDto {
        items,
        pending: st.pending,
        failed: st.failed,
    })
}

/// Accept a photo's route: `keywords` (route keywords only, those left
/// ticked) are added to it. Returns how many photos got something new.
#[tauri::command(async)]
pub fn aircraft_route_accept(
    state: State<'_, AppState>,
    id: String,
    keywords: Vec<String>,
) -> CmdResult<u64> {
    let asset: AssetId = convert::id(&id, "asset")?;
    let paths = keywords
        .iter()
        .map(|k| TagPath::parse_lightroom(k).map_err(err))
        .collect::<CmdResult<Vec<_>>>()?;
    if let Some(p) = paths.iter().find(|p| !route::is_route_keyword(p)) {
        return Err(format!("{} is not a route keyword", p.to_lightroom()));
    }
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::add_to(&mut cat, &open.lib, &[asset], &paths).map_err(err)
}
