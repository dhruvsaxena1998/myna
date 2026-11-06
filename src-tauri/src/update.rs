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
/// How often automatic checks run: at launch and daily, daily, weekly.
pub const EVERY: [&str; 3] = ["launch", "day", "week"];
/// `<FEEDS>/stable.json` and `<FEEDS>/beta.json`, rewritten by the release workflow.
pub const FEEDS: &str = "https://github.com/dhruvsaxena1998/myna/releases/download/updater";
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const BUSY: &str = "Finish recording first";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Prefs {
    pub mode: String,
    pub channel: String,
    pub every: String,
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
    /// The last check, manual or not. Saved, but only read back at launch when Check is daily or weekly.
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
        every: read(conn, "update_every")?.filter(|e| EVERY.contains(&e.as_str())).unwrap_or(EVERY[0].into()),
    })
}

pub fn set(conn: &Connection, p: &Prefs) -> Result<(), String> {
    if !MODES.contains(&p.mode.as_str()) || !CHANNELS.contains(&p.channel.as_str()) || !EVERY.contains(&p.every.as_str()) {
        return Err(format!("{} {} {} isn't one of the choices.", p.mode, p.channel, p.every));
    }
    for (k, v) in [("update_mode", &p.mode), ("update_channel", &p.channel), ("update_every", &p.every)] {
        conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [k, v])
            .map_err(|e| format!("Couldn't save the update settings: {e}"))?;
    }
    Ok(())
}

/// Whether an automatic check is due: never when Off, else when none has run yet and then every 24 h
/// (7 days when weekly) of clock time. At launch `last` is None unless `restore` read it back.
pub fn due(p: &Prefs, last: Option<SystemTime>, now: SystemTime) -> bool {
    let every = if p.every == "week" { WEEK } else { DAY };
    p.mode != "off" && last.is_none_or(|t| !now.duration_since(t).is_ok_and(|d| d < every))
}

fn write(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [key, value])
        .map(drop)
        .map_err(|e| format!("Couldn't save the update settings: {e}"))
}

/// Saves when the last check ran and what it found, so a launch that skips the check still shows it.
pub fn remember<R: Runtime>(app: &AppHandle<R>, conn: &Connection) -> Result<(), String> {
    let u = app.state::<Updates>();
    if let Some(t) = *u.last.lock().unwrap() {
        write(conn, "update_last", &t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs().to_string())?;
    }
    match &*u.state.lock().unwrap() {
        State::Available { version, notes } | State::Downloading { version, notes, .. } | State::Downloaded { version, notes } => {
            write(conn, "update_found", &serde_json::json!({ "version": version, "notes": notes }).to_string())
        }
        State::Idle | State::Uptodate => write(conn, "update_found", ""),
        State::Checking | State::Error { .. } => Ok(()), // nothing new learned
    }
}

/// At launch, when Check is daily or weekly: the last check's time, and its update if still newer than
/// this build. Returns whether an update was restored (still to download: the bytes aren't saved).
pub fn restore<R: Runtime>(app: &AppHandle<R>, conn: &Connection, p: &Prefs) -> Result<bool, String> {
    if p.every == "launch" {
        return Ok(false);
    }
    let u = app.state::<Updates>();
    let last = read(conn, "update_last")?.and_then(|s| s.parse().ok());
    *u.last.lock().unwrap() = last.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s));
    let found = read(conn, "update_found")?.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let newer = found
        .as_ref()
        .and_then(|f| Some((f["version"].as_str()?, f["notes"].as_str().unwrap_or_default())))
        .filter(|(v, _)| v.parse::<semver::Version>().is_ok_and(|v| v > app.package_info().version));
    let Some((version, notes)) = newer else { return Ok(false) };
    show(app, State::Available { version: version.into(), notes: notes.into() });
    Ok(true)
}

/// The notification for where a check left things, as (version, title, body): only at the state the mode
/// stops at, and once per version. `seen`: the versions already told about, comma-separated.
pub fn notice(mode: &str, s: &State, seen: &str) -> Option<(String, String, String)> {
    let (version, what, body) = match s {
        State::Available { version, .. } if mode == "check" => (version, "is available", "Click to see what’s new."),
        State::Downloaded { version, .. } if mode == "auto" => (version, "is ready", "Restart to finish updating."),
        _ => return None,
    };
    (!seen.split(',').any(|v| v == version)).then(|| (version.clone(), format!("Myna {version} {what}"), body.into()))
}

/// After a check: the notification to send as (title, body), if any, remembered across restarts.
/// A manual check sends none, since its pop-up opened, but what it found counts as told.
pub fn tell(conn: &Connection, mode: &str, s: &State, manual: bool) -> Result<Option<(String, String)>, String> {
    let seen = read(conn, "update_seen")?.unwrap_or_default();
    // A manual check always stops at Available, which Check only's notice matches.
    let Some((version, title, body)) = notice(if manual { "check" } else { mode }, s, &seen) else { return Ok(None) };
    conn.execute("INSERT INTO setting(key, value) VALUES ('update_seen', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1", [format!("{seen},{version}")])
        .map_err(|e| format!("Couldn't save the update settings: {e}"))?;
    Ok((!manual).then_some((title, body)))
}

/// The menubar item's label for `s`; None hides it.
pub fn menu_label(s: &State) -> Option<&'static str> {
    match s {
        State::Available { .. } | State::Downloading { .. } => Some("Update available…"),
        State::Downloaded { .. } => Some("Restart to update"),
        _ => None,
    }
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
    // GitHub's asset CDN has 4 IPs and some networks drop one; without this a connect to it hangs ~75 s
    // (the OS default) before the next is tried. hyper splits this across the IPs: 1 s each.
    // Measured on such a network: check and download 75 s each without it, ~1.3 s with it. Covers the download too.
    let builder = app.updater_builder().configure_client(|c| c.connect_timeout(Duration::from_secs(4)));
    match builder.endpoints(vec![url])?.build()?.check().await {
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
                return download(app, base, &p.channel, true).await;
            }
        }
        Err(e) if manual => return Err(fail(app, &e, "check")),
        Err(_) => show(app, was), // retried at the next check
    }
    Ok(())
}

/// Downloads and verifies what the last check found. `quiet`: failures go back to Available, except a bad signature.
/// An update restored at launch is looked up again first, at `base` on `channel`.
pub async fn download<R: Runtime>(app: &AppHandle<R>, base: &str, channel: &str, quiet: bool) -> Result<(), String> {
    let u = app.state::<Updates>();
    let (found, was) = {
        let s = u.state.lock().unwrap();
        if matches!(*s, State::Checking | State::Downloading { .. } | State::Downloaded { .. }) {
            return Ok(());
        }
        (u.found.lock().unwrap().clone(), s.clone())
    };
    let up = match found {
        Some(up) => up,
        None if !matches!(was, State::Available { .. }) => return Err("Check for updates first.".into()),
        None => match find(app, base, channel).await {
            Ok(Some(up)) => {
                show(app, available(&up)); // the feed may have moved on since it was saved
                *u.found.lock().unwrap() = Some(up.clone());
                up
            }
            Ok(None) => {
                show(app, State::Uptodate);
                return Ok(());
            }
            Err(_) if quiet => return Ok(()),
            Err(e) => return Err(fail(app, &e, "download")),
        },
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
        let p = Prefs { mode: mode.into(), channel: channel.into(), every: "launch".into() };
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

    fn prefs(mode: &str, every: &str) -> Prefs {
        Prefs { mode: mode.into(), channel: "beta".into(), every: every.into() }
    }

    #[test]
    fn off_never_checks_on_its_own_and_the_others_check_daily_or_weekly() {
        let now = SystemTime::now();
        let hour = Duration::from_secs(3600);
        assert!(!due(&prefs("off", "day"), None, now) && !due(&prefs("off", "day"), Some(now - 2 * DAY), now));
        for mode in ["auto", "check"] {
            for every in ["launch", "day"] {
                let p = prefs(mode, every);
                assert!(due(&p, None, now), "at launch");
                assert!(!due(&p, Some(now - 23 * hour), now));
                assert!(due(&p, Some(now - DAY), now));
                assert!(due(&p, Some(now + hour), now), "a clock set back still checks");
            }
            assert!(!due(&prefs(mode, "week"), Some(now - 6 * DAY), now));
            assert!(due(&prefs(mode, "week"), Some(now - WEEK), now));
        }
    }

    #[test]
    fn a_launch_restores_the_last_check_unless_it_checks_at_every_launch() {
        let tmp = tempfile::tempdir().unwrap();
        let (c, a) = (crate::storage::open(tmp.path()).unwrap(), app("0.0.2-beta.1"));
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        *a.state::<Updates>().last.lock().unwrap() = Some(t);
        show(a.handle(), avail("0.0.2-beta.2"));
        remember(a.handle(), &c).unwrap();

        let b = app("0.0.2-beta.1");
        assert_eq!(restore(b.handle(), &c, &prefs("check", "launch")), Ok(false));
        assert_eq!(*b.state::<Updates>().last.lock().unwrap(), None, "checks at launch");
        assert_eq!(restore(b.handle(), &c, &prefs("check", "day")), Ok(true));
        assert_eq!(*b.state::<Updates>().last.lock().unwrap(), Some(t));
        assert_eq!(*b.state::<Updates>().state.lock().unwrap(), avail("0.0.2-beta.2"));

        let updated = app("0.0.2-beta.2"); // the saved update is now this build
        assert_eq!(restore(updated.handle(), &c, &prefs("check", "week")), Ok(false));
        assert_eq!(*updated.state::<Updates>().state.lock().unwrap(), State::Idle);

        show(a.handle(), State::Uptodate);
        remember(a.handle(), &c).unwrap();
        assert_eq!(restore(app("0.0.2-beta.1").handle(), &c, &prefs("check", "day")), Ok(false), "nothing found last time");
    }

    #[test]
    fn a_restored_update_is_looked_up_again_before_downloading() {
        let base = feed(None, Some("0.0.2-beta.3"));
        let a = app("0.0.2-beta.1");
        show(a.handle(), avail("0.0.2-beta.2")); // as restored: no Update in hand
        let r = tauri::async_runtime::block_on(download(a.handle(), &base, "beta", false));
        assert_eq!(r, Err("This update failed its security check and wasn’t installed.".into()), "it fetched beta.3's bundle");
        assert_eq!(a.state::<Updates>().found.lock().unwrap().as_ref().map(|u| u.version.clone()), Some("0.0.2-beta.3".into()));
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
        let (h, base) = (a.handle().clone(), base.clone());
        let dl = std::thread::spawn(move || tauri::async_runtime::block_on(download(&h, &base, "beta", false)));
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
    fn a_notification_comes_once_per_version_only_from_automatic_checks() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let ready = |v: &str| State::Downloaded { version: v.into(), notes: "".into() };
        let ready_msg = |v: &str| Some((format!("Myna {v} is ready"), "Restart to finish updating.".into()));
        assert_eq!(tell(&conn, "off", &ready("0.0.3"), false), Ok(None));
        assert_eq!(tell(&conn, "off", &avail("0.0.3"), false), Ok(None));
        assert_eq!(tell(&conn, "auto", &avail("0.0.3"), false), Ok(None), "Check and download waits for the download");
        assert_eq!(tell(&conn, "auto", &ready("0.0.3"), false), Ok(ready_msg("0.0.3")));
        assert_eq!(tell(&conn, "auto", &ready("0.0.3"), false), Ok(None), "once per version");
        drop(conn);
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert_eq!(tell(&conn, "auto", &ready("0.0.3"), false), Ok(None), "not again after a restart");
        assert_eq!(tell(&conn, "check", &avail("0.0.4"), false), Ok(Some(("Myna 0.0.4 is available".into(), "Click to see what’s new.".into()))));
        assert_eq!(tell(&conn, "check", &avail("0.0.5"), true), Ok(None), "a manual check opened the pop-up");
        assert_eq!(tell(&conn, "check", &avail("0.0.5"), false), Ok(None), "and counts as told");
        assert_eq!(tell(&conn, "off", &avail("0.0.6"), true), Ok(None));
        assert_eq!(tell(&conn, "auto", &ready("0.0.7"), false), Ok(ready_msg("0.0.7")));
    }

    #[test]
    fn the_menubar_item_shows_while_an_update_is_on_its_way() {
        let downloading = State::Downloading { version: "0.0.3".into(), notes: "".into(), pct: 40 };
        assert_eq!(menu_label(&avail("0.0.3")), Some("Update available…"));
        assert_eq!(menu_label(&downloading), Some("Update available…"));
        assert_eq!(menu_label(&State::Downloaded { version: "0.0.3".into(), notes: "".into() }), Some("Restart to update"));
        for s in [State::Idle, State::Checking, State::Uptodate, State::Error { kind: "check".into(), message: "".into() }] {
            assert_eq!(menu_label(&s), None, "{s:?}");
        }
    }

    #[test]
    fn settings_default_to_the_builds_channel_and_save_only_valid_choices() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let p = |m: &str, c: &str| Prefs { mode: m.into(), channel: c.into(), every: "launch".into() };
        assert_eq!(get(&conn, true), Ok(p("auto", "beta")));
        assert_eq!(get(&conn, false), Ok(p("auto", "beta")), "a beta install that updates to stable stays on beta");
        let fresh = crate::storage::open(&tmp.path().join("other")).unwrap();
        assert_eq!(get(&fresh, false), Ok(p("auto", "stable")));
        set(&conn, &p("off", "stable")).unwrap();
        assert_eq!(get(&conn, true), Ok(p("off", "stable")));
        assert!(set(&conn, &p("sometimes", "beta")).is_err());
        assert!(set(&conn, &p("check", "nightly")).is_err());
        assert!(set(&conn, &Prefs { every: "hourly".into(), ..p("off", "stable") }).is_err());
        assert_eq!(get(&conn, true), Ok(p("off", "stable")));
        set(&conn, &Prefs { every: "week".into(), ..p("off", "stable") }).unwrap();
        assert_eq!(get(&conn, true).unwrap().every, "week");
    }
}
