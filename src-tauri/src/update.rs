//! App updates: Update mode and Channel, and the one update state the About row shows.
//! The feed is picked here from the Channel (JS can't set updater endpoints). Download and install are
//! separate: install waits for Restart now and never runs while dictating.

use rusqlite::{Connection, OptionalExtension};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

/// Check and download, Check only, Off.
pub const MODES: [&str; 3] = ["auto", "check", "off"];
pub const CHANNELS: [&str; 2] = ["stable", "beta"];
/// `<FEEDS>/stable.json` and `<FEEDS>/beta.json`, rewritten by the release workflow.
pub const FEEDS: &str = "https://github.com/dhruvsaxena1998/myna/releases/download/updater";
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
pub const BUSY: &str = "Finish recording first";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Prefs {
    pub mode: String,
    pub channel: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum State {
    #[default]
    Idle,
    Checking,
    /// `notes`: the release's Markdown, kept through Downloaded so the pop-up can show it at any point.
    Available { version: String, notes: String },
    Downloading { version: String, notes: String, pct: u64 },
    Downloaded { version: String, notes: String },
    /// `kind`: "check", "download", "install", or "signature" (always shown, even from an automatic check).
    Error { kind: String, message: String },
    Uptodate,
}

#[derive(Default)]
pub struct Updates {
    pub state: Mutex<State>,
    found: Mutex<Option<Update>>,
    bytes: Mutex<Option<Vec<u8>>>,
    /// The last check, manual or not. In memory only, so every launch checks.
    pub last: Mutex<Option<SystemTime>>,
    /// Stops the download in progress.
    cancel: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

fn read(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read the update settings: {e}"))
}

/// `prerelease`: the running build is a beta. Its Channel is saved on first launch, so a beta install
/// that later updates to a stable release stays on beta.
pub fn get(conn: &Connection, prerelease: bool) -> Result<Prefs, String> {
    let own = if prerelease { "beta" } else { "stable" };
    conn.execute("INSERT INTO setting(key, value) VALUES ('update_channel', ?1) ON CONFLICT(key) DO NOTHING", [own])
        .map_err(|e| format!("Couldn't save the update settings: {e}"))?;
    Ok(Prefs {
        mode: read(conn, "update_mode")?.filter(|m| MODES.contains(&m.as_str())).unwrap_or(MODES[0].into()),
        channel: read(conn, "update_channel")?.filter(|c| CHANNELS.contains(&c.as_str())).unwrap_or(own.into()),
    })
}

pub fn set(conn: &Connection, p: &Prefs) -> Result<(), String> {
    if !MODES.contains(&p.mode.as_str()) || !CHANNELS.contains(&p.channel.as_str()) {
        return Err(format!("{} {} isn't one of the choices.", p.mode, p.channel));
    }
    for (k, v) in [("update_mode", &p.mode), ("update_channel", &p.channel)] {
        conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [k, v])
            .map_err(|e| format!("Couldn't save the update settings: {e}"))?;
    }
    Ok(())
}

/// Whether an automatic check is due: never when Off, else at launch and every 24 h of clock time.
pub fn due(mode: &str, last: Option<SystemTime>, now: SystemTime) -> bool {
    mode != "off" && last.is_none_or(|t| !now.duration_since(t).is_ok_and(|d| d < DAY))
}

pub fn can_install(busy: bool, s: &State) -> Result<(), String> {
    if busy {
        return Err(BUSY.into());
    }
    if !matches!(s, State::Downloaded { .. }) {
        return Err("Download the update first.".into());
    }
    Ok(())
}

fn show<R: Runtime>(app: &AppHandle<R>, s: State) {
    *app.state::<Updates>().state.lock().unwrap() = s.clone();
    let _ = app.emit("update-state", s);
}

fn info(up: &Update) -> (String, String) {
    (up.version.clone(), up.body.clone().unwrap_or_default())
}

fn available(up: &Update) -> State {
    let (version, notes) = info(up);
    State::Available { version, notes }
}

fn bad_signature(e: &Error) -> bool {
    matches!(e, Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) | Error::SignedVersionMismatch { .. } | Error::MissingSignedVersion)
}

/// Shows the error and returns its message.
fn fail<R: Runtime>(app: &AppHandle<R>, e: &Error, kind: &str) -> String {
    let (kind, message) = if bad_signature(e) {
        ("signature", "This update failed its security check and wasn’t installed.".to_string())
    } else if matches!(e, Error::Reqwest(_) | Error::Network(_)) {
        (kind, "Couldn’t reach GitHub. Check your connection.".to_string())
    } else {
        (kind, e.to_string())
    };
    show(app, State::Error { kind: kind.into(), message: message.clone() });
    message
}

/// The Channel's feed at `base`. The updater only offers a newer version, so switching beta → stable never downgrades.
pub async fn find<R: Runtime>(app: &AppHandle<R>, base: &str, channel: &str) -> Result<Option<Update>, Error> {
    let url = format!("{base}/{channel}.json").parse()?;
    match app.updater_builder().endpoints(vec![url])?.build()?.check().await {
        // shortcut: any non-2xx reads as "nothing yet" (stable.json 404s until the first stable release); tell 404 from 5xx if GitHub outages confuse people.
        Err(Error::ReleaseNotFound) => Ok(None),
        r => r,
    }
}

/// `manual`: from the button. It stops at Available and reports errors; an automatic check stays quiet on
/// errors and, in Check and download, downloads what it finds.
pub async fn check<R: Runtime>(app: &AppHandle<R>, base: &str, p: &Prefs, manual: bool) -> Result<(), String> {
    let u = app.state::<Updates>();
    let was = {
        let s = u.state.lock().unwrap();
        if matches!(*s, State::Checking | State::Downloading { .. } | State::Downloaded { .. }) {
            return Ok(()); // one at a time, and never over an update already on its way
        }
        s.clone()
    };
    *u.last.lock().unwrap() = Some(SystemTime::now());
    show(app, State::Checking);
    match find(app, base, &p.channel).await {
        Ok(None) => {
            *u.found.lock().unwrap() = None;
            show(app, if manual { State::Uptodate } else { State::Idle });
        }
        Ok(Some(up)) => {
            show(app, available(&up));
            *u.found.lock().unwrap() = Some(up);
            if !manual && p.mode == "auto" {
                return download(app, true).await;
            }
        }
        Err(e) if manual => return Err(fail(app, &e, "check")),
        Err(_) => show(app, was), // retried at the next check
    }
    Ok(())
}

/// Downloads and verifies what the last check found. `quiet`: failures go back to Available, except a bad signature.
pub async fn download<R: Runtime>(app: &AppHandle<R>, quiet: bool) -> Result<(), String> {
    let u = app.state::<Updates>();
    let up = {
        let s = u.state.lock().unwrap();
        if matches!(*s, State::Checking | State::Downloading { .. } | State::Downloaded { .. }) {
            return Ok(());
        }
        u.found.lock().unwrap().clone().ok_or("Check for updates first.")?
    };
    let (version, notes) = info(&up);
    let downloading = move |pct| State::Downloading { version: version.clone(), notes: notes.clone(), pct };
    show(app, downloading(0));
    let (a, task_up) = (app.clone(), up.clone());
    // Its own task, so Cancel can drop it mid-download (the updater has no cancel of its own).
    let task = tauri::async_runtime::spawn(async move {
        let (mut got, mut last) = (0u64, 0u64);
        let chunk = |n: usize, total: Option<u64>| {
            got += n as u64;
            let pct = total.filter(|t| *t > 0).map_or(0, |t| (got * 100 / t).min(100));
            if pct != last {
                last = pct;
                show(&a, downloading(pct));
            }
        };
        task_up.download(chunk, || {}).await
    });
    let abort = task.inner().abort_handle();
    *u.cancel.lock().unwrap() = Some(Box::new(move || abort.abort()));
    let r = task.await;
    u.cancel.lock().unwrap().take();
    match r {
        Err(_) => {
            show(app, available(&up)); // cancelled
            Ok(())
        }
        Ok(Ok(b)) => {
            *u.bytes.lock().unwrap() = Some(b);
            let (version, notes) = info(&up);
            show(app, State::Downloaded { version, notes });
            Ok(())
        }
        Ok(Err(e)) if quiet && !bad_signature(&e) => {
            show(app, available(&up));
            Ok(())
        }
        Ok(Err(e)) => Err(fail(app, &e, "download")),
    }
}

/// Cancel in the pop-up: the download in progress stops and the update is offered again.
pub fn cancel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(stop) = app.state::<Updates>().cancel.lock().unwrap().take() {
        stop();
    }
}

/// Puts the downloaded update in place; the caller relaunches. `busy`: dictating or recording.
pub fn install<R: Runtime>(app: &AppHandle<R>, busy: bool) -> Result<(), String> {
    let u = app.state::<Updates>();
    can_install(busy, &u.state.lock().unwrap())?;
    let (Some(up), Some(bytes)) = (u.found.lock().unwrap().clone(), u.bytes.lock().unwrap().take()) else {
        return Err("Download the update first.".into());
    };
    up.install(bytes).map_err(|e| fail(app, &e, "install"))
}

/// A new Channel means a new feed, so what the old one said no longer applies. A download in hand stays.
pub fn forget<R: Runtime>(app: &AppHandle<R>) {
    let u = app.state::<Updates>();
    if matches!(*u.state.lock().unwrap(), State::Available { .. } | State::Uptodate | State::Error { .. }) {
        *u.found.lock().unwrap() = None;
        show(app, State::Idle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};

    const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDFDNkEwOEMwRDg1Q0EyMUEKUldRYW9sell3QWhxSExCZ2g1YllUWUpCTEV0bWVMWXlUVlFPNWtyVU9VSm9qZHlGSWxNdVJKRnMK";

    /// A fake feed: `stable` and `beta` are each feed's version, None = 404. Any other path serves a few bytes.
    fn feed(stable: Option<&str>, beta: Option<&str>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let (stable, beta, b) = (stable.map(String::from), beta.map(String::from), base.clone());
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap_or(0);
                let path = String::from_utf8_lossy(&req[..n]).split(' ').nth(1).unwrap_or("").to_string();
                let v = match path.as_str() {
                    "/stable.json" => stable.as_ref(),
                    "/beta.json" => beta.as_ref(),
                    _ => Some(&b), // the update bundle; its contents don't matter
                };
                if path.contains("stall") {
                    // A download that never finishes, for Cancel.
                    std::thread::spawn(move || {
                        let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\npartial");
                        std::thread::sleep(Duration::from_secs(30));
                    });
                    continue;
                }
                let body = match v {
                    None => { let _ = write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); continue; }
                    Some(_) if !path.ends_with(".json") => "not a real bundle".to_string(),
                    Some(v) => {
                        let p = serde_json::json!({ "signature": "bm90IGEgc2lnbmF0dXJl", "url": format!("{b}/{v}.tar.gz") });
                        serde_json::json!({ "version": v, "notes": "## What’s new", "platforms": { "darwin-aarch64": p, "darwin-x86_64": p } }).to_string()
                    }
                };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        base
    }

    fn app(version: &str) -> tauri::App<MockRuntime> {
        let mut ctx = mock_context(noop_assets());
        ctx.package_info_mut().version = version.parse().unwrap();
        ctx.config_mut().plugins.0.insert("updater".into(), serde_json::json!({ "pubkey": PUBKEY }));
        mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).manage(Updates::default()).build(ctx).unwrap()
    }

    fn run(app: &tauri::App<MockRuntime>, base: &str, mode: &str, channel: &str, manual: bool) -> (Result<(), String>, State) {
        let p = Prefs { mode: mode.into(), channel: channel.into() };
        let r = tauri::async_runtime::block_on(check(app.handle(), base, &p, manual));
        (r, app.state::<Updates>().state.lock().unwrap().clone())
    }

    fn avail(v: &str) -> State {
        State::Available { version: v.into(), notes: "## What’s new".into() }
    }

    #[test]
    fn a_newer_beta_is_found_on_beta() {
        let base = feed(Some("0.0.1"), Some("0.0.2-beta.2"));
        assert_eq!(run(&app("0.0.2-beta.1"), &base, "check", "beta", true), (Ok(()), avail("0.0.2-beta.2")));
    }

    #[test]
    fn a_beta_is_never_offered_on_stable() {
        let base = feed(Some("0.0.1"), Some("0.0.3-beta.1"));
        assert_eq!(run(&app("0.0.1"), &base, "check", "stable", true), (Ok(()), State::Uptodate));
    }

    #[test]
    fn the_matching_stable_is_offered_to_its_beta() {
        let base = feed(Some("0.0.2"), Some("0.0.2"));
        assert_eq!(run(&app("0.0.2-beta.1"), &base, "check", "beta", true), (Ok(()), avail("0.0.2")));
    }

    #[test]
    fn switching_beta_to_stable_never_downgrades() {
        let base = feed(Some("0.0.1"), Some("0.0.2-beta.1"));
        assert_eq!(run(&app("0.0.2-beta.1"), &base, "check", "stable", true), (Ok(()), State::Uptodate));
    }

    #[test]
    fn no_stable_feed_yet_is_up_to_date_not_an_error() {
        let base = feed(None, Some("0.0.2-beta.1"));
        assert_eq!(run(&app("0.0.2-beta.1"), &base, "check", "stable", true), (Ok(()), State::Uptodate));
        assert_eq!(run(&app("0.0.2-beta.1"), &base, "auto", "stable", false), (Ok(()), State::Idle));
    }

    #[test]
    fn off_never_checks_on_its_own_and_the_others_check_daily() {
        let now = SystemTime::now();
        let hour = Duration::from_secs(3600);
        assert!(!due("off", None, now) && !due("off", Some(now - 2 * DAY), now));
        for mode in ["auto", "check"] {
            assert!(due(mode, None, now), "at launch");
            assert!(!due(mode, Some(now - 23 * hour), now));
            assert!(due(mode, Some(now - DAY), now));
            assert!(due(mode, Some(now + hour), now), "a clock set back still checks");
        }
    }

    #[test]
    fn automatic_errors_stay_quiet_and_a_manual_check_reports_them() {
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap(); // closed again at once
        let base = format!("http://{dead}");
        let a = app("0.0.2-beta.1");
        assert_eq!(run(&a, &base, "auto", "beta", false), (Ok(()), State::Idle));
        let (r, s) = run(&a, &base, "auto", "beta", true);
        assert_eq!(r, Err("Couldn’t reach GitHub. Check your connection.".into()));
        assert!(matches!(s, State::Error { ref kind, .. } if kind == "check"), "{s:?}");
    }

    #[test]
    fn check_only_stops_at_available_and_a_bad_signature_is_always_shown() {
        let base = feed(None, Some("0.0.2-beta.2"));
        assert_eq!(run(&app("0.0.2-beta.1"), &base, "check", "beta", false), (Ok(()), avail("0.0.2-beta.2")));
        let (r, s) = run(&app("0.0.2-beta.1"), &base, "auto", "beta", false);
        assert_eq!(r, Err("This update failed its security check and wasn’t installed.".into()));
        assert!(matches!(s, State::Error { ref kind, .. } if kind == "signature"), "{s:?}");
    }

    #[test]
    fn install_is_refused_while_recording() {
        let done = State::Downloaded { version: "0.0.2".into(), notes: "".into() };
        assert_eq!(can_install(true, &done), Err(BUSY.into()));
        assert!(can_install(false, &avail("0.0.2")).is_err(), "nothing downloaded yet");
        assert_eq!(can_install(false, &done), Ok(()));
        let a = app("0.0.2-beta.1");
        show(a.handle(), done.clone());
        assert_eq!(install(a.handle(), true), Err(BUSY.into()));
        assert_eq!(*a.state::<Updates>().state.lock().unwrap(), done, "still ready for Restart now");
    }

    #[test]
    fn cancel_stops_the_download_and_offers_the_update_again() {
        let base = feed(None, Some("0.0.3-stall.1"));
        let a = app("0.0.2-beta.1");
        assert_eq!(run(&a, &base, "check", "beta", true), (Ok(()), avail("0.0.3-stall.1")));
        let h = a.handle().clone();
        let dl = std::thread::spawn(move || tauri::async_runtime::block_on(download(&h, false)));
        for _ in 0..500 {
            if a.state::<Updates>().cancel.lock().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(*a.state::<Updates>().state.lock().unwrap(), State::Downloading { .. }));
        cancel(a.handle());
        assert_eq!(dl.join().unwrap(), Ok(()));
        assert_eq!(*a.state::<Updates>().state.lock().unwrap(), avail("0.0.3-stall.1"));
    }

    #[test]
    fn settings_default_to_the_builds_channel_and_save_only_valid_choices() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let p = |m: &str, c: &str| Prefs { mode: m.into(), channel: c.into() };
        assert_eq!(get(&conn, true), Ok(p("auto", "beta")));
        assert_eq!(get(&conn, false), Ok(p("auto", "beta")), "a beta install that updates to stable stays on beta");
        let fresh = crate::storage::open(&tmp.path().join("other")).unwrap();
        assert_eq!(get(&fresh, false), Ok(p("auto", "stable")));
        set(&conn, &p("off", "stable")).unwrap();
        assert_eq!(get(&conn, true), Ok(p("off", "stable")));
        assert!(set(&conn, &p("sometimes", "beta")).is_err());
        assert!(set(&conn, &p("check", "nightly")).is_err());
        assert_eq!(get(&conn, true), Ok(p("off", "stable")));
    }
}
