//! Stopping background work when the app exits.
//!
//! Tauri ends the process without dropping managed state or statics, so
//! nothing stops by itself. ExifTool (`-stay_open`) in particular never
//! quits on its own and would run on after the app, locking its file. On
//! Windows the job in `keepsake_platform::children` ends it anyway, even
//! after a crash; this hook is the clean way out on every OS.

use std::time::{Duration, Instant};

use crate::state::AppState;

/// How long the exit waits for background work to stop.
const WAIT: Duration = Duration::from_secs(5);

/// Called once on `RunEvent::Exit`.
pub fn on_exit(state: &AppState) {
    let started = Instant::now();
    let all = stop(state, WAIT);
    tracing::info!(
        ms = started.elapsed().as_millis() as u64,
        all_stopped = all,
        "background work stopped for exit"
    );
}

/// Ask the render service (AI analysis runs in it), an import, a
/// maintenance job and an AI model download to stop, wait up to `wait`
/// for them, then stop the app's shared ExifTool. True when everything
/// stopped in time.
pub(crate) fn stop(state: &AppState, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    state.render.stop();
    crate::import::request_stop(state);
    state.maint.request_stop();
    state.ai.stop_install();
    let stopped = loop {
        let idle = state.render.finished()
            && !state
                .import_running
                .load(std::sync::atomic::Ordering::SeqCst)
            && !state.maint.is_running();
        if idle || Instant::now() >= deadline {
            break idle;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if !stopped {
        tracing::warn!("background work still running at exit");
    }
    crate::import::stop_exiftool();
    stopped
}

#[cfg(test)]
mod tests {
    use super::stop;
    use crate::state::AppState;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Set when dropped: stands in for the service's ExifTool.
    struct Owned(Arc<AtomicBool>);

    impl Drop for Owned {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// A render service that runs until cancelled and owns `dropped`.
    fn fake_service(state: &AppState, dropped: &Arc<AtomicBool>) {
        let cancel = Arc::new(AtomicBool::new(false));
        let (c, owned) = (Arc::clone(&cancel), Owned(Arc::clone(dropped)));
        let worker = std::thread::spawn(move || {
            let _owned = owned;
            while !c.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        state.render.adopt(cancel, worker);
    }

    #[test]
    fn exit_stops_the_services_and_drops_what_they_own() {
        let state = Arc::new(AppState::default());
        let dropped = Arc::new(AtomicBool::new(false));
        fake_service(&state, &dropped);
        // An import that stops when asked.
        let cancel = Arc::new(AtomicBool::new(false));
        *state.import_cancel.lock().unwrap() = Some(Arc::clone(&cancel));
        state.import_running.store(true, Ordering::SeqCst);
        let s = Arc::clone(&state);
        let import = std::thread::spawn(move || {
            while !cancel.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            s.import_running.store(false, Ordering::SeqCst);
        });
        assert!(stop(&state, Duration::from_secs(5)));
        assert!(
            dropped.load(Ordering::SeqCst),
            "the service's tools live on"
        );
        assert!(state.render.finished());
        import.join().unwrap();
    }

    #[test]
    fn exit_does_not_wait_forever() {
        let state = AppState::default();
        // An import that never ends.
        state.import_running.store(true, Ordering::SeqCst);
        let started = Instant::now();
        assert!(!stop(&state, Duration::from_millis(200)));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn exit_with_nothing_running_is_quick() {
        let started = Instant::now();
        assert!(stop(&AppState::default(), Duration::from_secs(5)));
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}
