//! The local channel the companion browser extension (`extension/`) reports the call's mute button on.
//! A tiny HTTP server on 127.0.0.1 only. Every request must carry the pairing token Myna shows in Settings, come
//! from this host and from an extension (never a web page), and be a known message, so another process or page
//! on this Mac can't spoof mute state. Reports feed the same timeline as the manual toggle (`mute::Source::Extension`).
use super::{captions, mute::{Event, Source}, Recorder};
use crate::{settings, Db};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

pub const PORT: u16 = 47615;
const TOKEN_KEY: &str = "extension_token";
/// No report for this long and the extension counts as gone.
const STALE_MS: u64 = 15_000;
const MAX_HEAD: usize = 8 * 1024;
const MAX_BODY: usize = 1024;

#[derive(Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    Meet,
    Teams,
}

impl Site {
    fn label(self) -> &'static str {
        match self {
            Self::Meet => "mirrored from Meet",
            Self::Teams => "mirrored from Teams",
        }
    }
}

/// What the extension may send. A new kind of message is a new variant; anything else is rejected.
#[derive(Deserialize, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Message {
    /// `muted` null means the extension can't read the button: "mute unknown".
    Mute { site: Site, muted: Option<bool> },
    /// Who the call's captions show as speaking now; null when captions are off or unreadable. Myna stamps the time.
    Caption { site: Site, name: Option<String> },
}

#[derive(PartialEq, Debug)]
pub enum Reject {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    TooLarge,
}

impl Reject {
    fn status(&self) -> (u16, &'static str) {
        match self {
            Self::BadRequest => (400, "Bad Request"),
            Self::Unauthorized => (401, "Unauthorized"),
            Self::Forbidden => (403, "Forbidden"),
            Self::NotFound => (404, "Not Found"),
            Self::TooLarge => (413, "Payload Too Large"),
        }
    }
}

struct Head<'a> {
    method: &'a str,
    path: &'a str,
    host: Option<&'a str>,
    origin: Option<&'a str>,
    auth: Option<&'a str>,
    len: usize,
}

fn parse_head(head: &str) -> Result<Head<'_>, Reject> {
    let mut lines = head.split("\r\n");
    let mut start = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(path), Some(v), None) = (start.next(), start.next(), start.next(), start.next()) else {
        return Err(Reject::BadRequest);
    };
    if !v.starts_with("HTTP/1.") {
        return Err(Reject::BadRequest);
    }
    let mut h = Head { method, path, host: None, origin: None, auth: None, len: 0 };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "host" => h.host = h.host.or(Some(value)),
            "origin" => h.origin = h.origin.or(Some(value)),
            "authorization" => h.auth = h.auth.or(Some(value)),
            "content-length" => h.len = value.parse().map_err(|_| Reject::BadRequest)?,
            _ => {}
        }
    }
    Ok(h)
}

/// Compares without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

/// Host must be the loopback name the extension dials (a rebound DNS name fails here), a page's Origin is refused,
/// and the bearer token must match.
fn authorize(h: &Head, token: &str) -> Result<(), Reject> {
    let loopback = [format!("127.0.0.1:{PORT}"), format!("localhost:{PORT}")];
    if !h.host.is_some_and(|host| loopback.iter().any(|l| l == host)) || h.origin.is_some_and(|o| !o.starts_with("chrome-extension://")) {
        return Err(Reject::Forbidden);
    }
    match h.auth.and_then(|a| a.strip_prefix("Bearer ")) {
        Some(t) if !token.is_empty() && same(t, token) => Ok(()),
        _ => Err(Reject::Unauthorized),
    }
}

fn route(h: &Head, token: &str) -> Result<(), Reject> {
    authorize(h, token)?;
    if h.path != "/message" {
        return Err(Reject::NotFound);
    }
    if h.method != "POST" {
        return Err(Reject::BadRequest);
    }
    Ok(())
}

pub fn parse_message(body: &[u8]) -> Result<Message, Reject> {
    match serde_json::from_slice(body).map_err(|_| Reject::BadRequest)? {
        // A name is page text: kept only if it is a plain, short one, trimmed.
        Message::Caption { site, name: Some(n) } => Ok(Message::Caption { site, name: Some(captions::valid(&n).ok_or(Reject::BadRequest)?.to_string()) }),
        m => Ok(m),
    }
}

/// Whole-request check on what came off the wire: the head, then the body the head promised.
fn accept(raw: &[u8], token: &str) -> Result<Message, Reject> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or(Reject::BadRequest)?;
    let head = parse_head(std::str::from_utf8(&raw[..end]).map_err(|_| Reject::BadRequest)?)?;
    route(&head, token)?;
    if head.len > MAX_BODY {
        return Err(Reject::TooLarge);
    }
    parse_message(raw.get(end + 4..end + 4 + head.len).ok_or(Reject::BadRequest)?)
}

/// The extension's last report, per recording. Pure bookkeeping: what to write to the timeline, if anything.
#[derive(Default)]
pub struct Link(Option<Seen>);

#[derive(Clone, Copy)]
struct Seen {
    meeting: i64,
    muted: Option<bool>,
    at_ms: u64,
}

/// "Mute unknown" or a vanished extension hands the mic back to Myna's own toggle, kept out of the transcript
/// until the user unmutes there: ticket 08's behaviour.
fn fallback(at_ms: u64) -> Event {
    Event { at_ms, muted: true, source: Source::Manual }
}

impl Link {
    /// A report at `at_ms` on the recording's clock. Only a change is written, so Myna's own toggle holds
    /// until the call's mute button moves again.
    pub fn report(&mut self, meeting: i64, at_ms: u64, muted: Option<bool>) -> Option<Event> {
        let prior = self.0.filter(|s| s.meeting == meeting).and_then(|s| s.muted);
        self.0 = Some(Seen { meeting, muted, at_ms });
        match (prior, muted) {
            (p, Some(m)) if p != Some(m) => Some(Event { at_ms, muted: m, source: Source::Extension }),
            (Some(_), None) => Some(fallback(at_ms)),
            _ => None,
        }
    }

    /// The extension went quiet: fall back from where it was last heard.
    pub fn expire(&mut self, meeting: i64, now_ms: u64) -> Option<Event> {
        let s = self.0.as_mut().filter(|s| s.meeting == meeting && s.muted.is_some() && now_ms.saturating_sub(s.at_ms) >= STALE_MS)?;
        s.muted = None;
        Some(fallback(s.at_ms))
    }
}

struct State {
    link: Link,
    /// When anything last got through, and from where; drives the Settings pill and the "mirrored from" text.
    seen: Option<(Instant, Site)>,
}

static STATE: Mutex<State> = Mutex::new(State { link: Link(None), seen: None });

fn connected() -> Option<Site> {
    STATE.lock().unwrap().seen.filter(|(t, _)| t.elapsed().as_millis() < STALE_MS as u128).map(|(_, s)| s)
}

/// "mirrored from Meet" while the extension is the source, plain "mirrored" if it's gone quiet since.
pub fn mirrored() -> &'static str {
    connected().map_or("mirrored", Site::label)
}

fn apply(app: &AppHandle, site: Site, muted: Option<bool>) {
    let wrote = {
        let rec = app.state::<Recorder>();
        let slot = rec.0.lock().unwrap();
        let mut st = STATE.lock().unwrap();
        st.seen = Some((Instant::now(), site));
        slot.as_ref().is_some_and(|a| {
            let at_ms = a.started.elapsed().as_millis() as u64;
            st.link.report(a.id, at_ms, muted).is_some_and(|e| super::store::add_mute_event(&app.state::<Db>().0.lock().unwrap(), a.id, &e).is_ok())
        })
    };
    if wrote {
        crate::settle_pill(app);
    }
}

/// A caption report at the recording's clock, kept for naming speakers once the meeting is transcribed.
fn apply_caption(app: &AppHandle, site: Site, name: Option<&str>) {
    let rec = app.state::<Recorder>();
    let slot = rec.0.lock().unwrap();
    STATE.lock().unwrap().seen = Some((Instant::now(), site));
    if let Some(a) = slot.as_ref() {
        let _ = captions::add(&app.state::<Db>().0.lock().unwrap(), a.id, a.started.elapsed().as_millis() as u64, name);
    }
}

/// Once a second, from the recorder's clock thread: falls back when the extension has gone quiet.
pub fn tick(app: &AppHandle) {
    let wrote = {
        let rec = app.state::<Recorder>();
        let slot = rec.0.lock().unwrap();
        slot.as_ref().is_some_and(|a| {
            let e = STATE.lock().unwrap().link.expire(a.id, a.started.elapsed().as_millis() as u64);
            e.is_some_and(|e| super::store::add_mute_event(&app.state::<Db>().0.lock().unwrap(), a.id, &e).is_ok())
        })
    };
    if wrote {
        crate::settle_pill(app);
    }
}

/// Random 128 bits as hex, from the OS.
fn new_token() -> String {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).expect("the OS random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn token(db: &Connection) -> String {
    settings::get(db, TOKEN_KEY).filter(|t| !t.is_empty()).unwrap_or_else(|| {
        let t = new_token();
        let _ = settings::set(db, TOKEN_KEY, &t);
        t
    })
}

fn read(s: &mut TcpStream) -> Vec<u8> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 1024];
    // Read until the head ends, then until the body the head promised has arrived; the caps keep a stuck or hostile client small.
    while raw.len() < MAX_HEAD + MAX_BODY {
        let Ok(n @ 1..) = s.read(&mut buf) else { break };
        raw.extend_from_slice(&buf[..n]);
        let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
        let len = std::str::from_utf8(&raw[..end]).ok().and_then(|h| parse_head(h).ok()).map_or(0, |h| h.len);
        if raw.len() >= end + 4 + len {
            break;
        }
    }
    raw
}

fn serve(app: &AppHandle, mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
    let raw = read(&mut s);
    let token = token(&app.state::<Db>().0.lock().unwrap());
    let (code, text) = match accept(&raw, &token) {
        Ok(Message::Mute { site, muted }) => {
            apply(app, site, muted);
            (204, "No Content")
        }
        Ok(Message::Caption { site, name }) => {
            apply_caption(app, site, name.as_deref());
            (204, "No Content")
        }
        Err(r) => r.status(),
    };
    let _ = write!(s, "HTTP/1.1 {code} {text}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
}

// shortcut: one connection at a time, with a 2 s read timeout; the only client is the extension, a few small posts a second. Spawn per connection if that changes.
/// Starts the listener on loopback. Called once, at launch.
pub fn start(app: &AppHandle) {
    let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => l,
        Err(e) => return eprintln!("Myna: the extension relay couldn't listen on 127.0.0.1:{PORT}: {e}"),
    };
    let app = app.clone();
    std::thread::spawn(move || listener.incoming().flatten().for_each(|s| serve(&app, s)));
}

#[derive(Serialize)]
pub struct Pairing {
    token: String,
    port: u16,
    connected: bool,
    site: Option<&'static str>,
}

#[tauri::command]
pub fn extension_state(db: tauri::State<Db>) -> Pairing {
    let site = connected();
    Pairing { token: token(&db.0.lock().unwrap()), port: PORT, connected: site.is_some(), site: site.map(|s| if s == Site::Meet { "Google Meet" } else { "Teams" }) }
}

/// A new token un-pairs every browser until the new one is pasted into the extension.
#[tauri::command]
pub fn extension_reset_token(db: tauri::State<Db>) -> Result<(), String> {
    STATE.lock().unwrap().seen = None;
    settings::set(&db.0.lock().unwrap(), TOKEN_KEY, &new_token()).map_err(|e| e.to_string())
}

/// Opens the bundled extension folder in Finder, for "Load unpacked".
#[tauri::command]
pub fn reveal_extension(app: AppHandle) -> Result<(), String> {
    let dir = app.path().resource_dir().map_err(|e| e.to_string())?.join("extension");
    std::process::Command::new("open").arg(dir).spawn().map(drop).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "0123456789abcdef";

    fn req(method: &str, path: &str, extra: &[(&str, &str)], body: &str) -> Vec<u8> {
        let mut r = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nOrigin: chrome-extension://abcdef\r\nAuthorization: Bearer {T}\r\nContent-Length: {}\r\n", body.len());
        // A header given in `extra` replaces the default one of the same name.
        for (k, v) in extra {
            r = r.lines().filter(|l| !l.to_lowercase().starts_with(&format!("{}:", k.to_lowercase()))).map(|l| format!("{l}\r\n")).collect();
            if !v.is_empty() {
                r.push_str(&format!("{k}: {v}\r\n"));
            }
        }
        format!("{r}\r\n{body}").into_bytes()
    }

    const MUTED: &str = r#"{"type":"mute","site":"meet","muted":true}"#;

    #[test]
    fn a_paired_extension_message_gets_through() {
        let want = Message::Mute { site: Site::Meet, muted: Some(true) };
        assert_eq!(accept(&req("POST", "/message", &[], MUTED), T), Ok(want));
        let unknown = r#"{"type":"mute","site":"teams","muted":null}"#;
        assert_eq!(accept(&req("POST", "/message", &[], unknown), T), Ok(Message::Mute { site: Site::Teams, muted: None }));
        // No Origin at all (not a browser) is fine with the token; a missing "muted" reads as unknown.
        assert!(accept(&req("POST", "/message", &[("Origin", "")], r#"{"type":"mute","site":"meet"}"#), T).is_ok());
    }

    #[test]
    fn a_caption_message_gets_through_trimmed_and_the_same_rules_apply() {
        let caption = |body: &str| accept(&req("POST", "/message", &[], body), T);
        assert_eq!(caption(r#"{"type":"caption","site":"meet","name":" Lena Ström "}"#), Ok(Message::Caption { site: Site::Meet, name: Some("Lena Ström".into()) }));
        assert_eq!(caption(r#"{"type":"caption","site":"teams","name":null}"#), Ok(Message::Caption { site: Site::Teams, name: None }));
        assert_eq!(caption(r#"{"type":"caption","site":"teams"}"#), Ok(Message::Caption { site: Site::Teams, name: None }), "a missing name reads as unreadable");
        let max = format!(r#"{{"type":"caption","site":"meet","name":"{}"}}"#, "é".repeat(captions::MAX_NAME));
        assert!(max.len() < MAX_BODY && caption(&max).is_ok(), "the longest name fits the body cap");
        let bad = accept(&req("POST", "/message", &[("Authorization", "Bearer nope")], r#"{"type":"caption","site":"meet","name":"Lena"}"#), T);
        assert_eq!(bad, Err(Reject::Unauthorized));
        let page = accept(&req("POST", "/message", &[("Origin", "https://evil.example")], r#"{"type":"caption","site":"meet","name":"Lena"}"#), T);
        assert_eq!(page, Err(Reject::Forbidden));
        let huge = format!(r#"{{"type":"caption","site":"meet","name":"{}"}}"#, "x".repeat(2000));
        assert_eq!(caption(&huge), Err(Reject::TooLarge));
    }

    #[test]
    fn anything_unpaired_or_not_from_an_extension_is_refused() {
        let bad = |extra: &[(&str, &str)]| accept(&req("POST", "/message", extra, MUTED), T);
        let table = [
            ("wrong token", bad(&[("Authorization", "Bearer nope")]), Reject::Unauthorized),
            ("same length wrong token", bad(&[("Authorization", "Bearer 0123456789abcdee")]), Reject::Unauthorized),
            ("no token", bad(&[("Authorization", "")]), Reject::Unauthorized),
            ("not a bearer", bad(&[("Authorization", T)]), Reject::Unauthorized),
            ("a web page's origin", bad(&[("Origin", "https://evil.example")]), Reject::Forbidden),
            ("a rebound DNS name", bad(&[("Host", "evil.example:47615")]), Reject::Forbidden),
            ("the wrong port", bad(&[("Host", "127.0.0.1:80")]), Reject::Forbidden),
            ("no host", bad(&[("Host", "")]), Reject::Forbidden),
        ];
        for (name, got, want) in table {
            assert_eq!(got, Err(want), "{name}");
        }
        assert_eq!(accept(&req("POST", "/message", &[], MUTED), ""), Err(Reject::Unauthorized), "an empty configured token never matches");
        assert_eq!(accept(&req("POST", "/other", &[], MUTED), T), Err(Reject::NotFound));
        assert_eq!(accept(&req("GET", "/message", &[], ""), T), Err(Reject::BadRequest));
        assert_eq!(accept(&req("OPTIONS", "/message", &[], ""), T), Err(Reject::BadRequest), "a preflight is never answered yes");
    }

    #[test]
    fn malformed_requests_and_messages_are_rejected_without_a_panic() {
        let msg = |body: &str| accept(&req("POST", "/message", &[], body), T);
        let table = [
            ("not json", msg("nope")),
            ("empty", msg("")),
            ("unknown type", msg(r#"{"type":"reboot"}"#)),
            ("unknown site", msg(r#"{"type":"mute","site":"zoom","muted":true}"#)),
            ("muted not a bool", msg(r#"{"type":"mute","site":"meet","muted":"yes"}"#)),
            ("extra field", msg(r#"{"type":"mute","site":"meet","muted":true,"x":1}"#)),
            ("no site", msg(r#"{"type":"mute","muted":true}"#)),
            ("caption name not a string", msg(r#"{"type":"caption","site":"meet","name":7}"#)),
            ("caption name empty", msg(r#"{"type":"caption","site":"meet","name":"  "}"#)),
            ("caption name too long", msg(&format!(r#"{{"type":"caption","site":"meet","name":"{}"}}"#, "x".repeat(captions::MAX_NAME + 1)))),
            ("caption name with a control character", msg(r#"{"type":"caption","site":"teams","name":"a\nb"}"#)),
            ("caption extra field", msg(r#"{"type":"caption","site":"meet","name":"Lena","text":"hi"}"#)),
            ("caption unknown site", msg(r#"{"type":"caption","site":"zoom","name":"Lena"}"#)),
        ];
        for (name, got) in table {
            assert_eq!(got, Err(Reject::BadRequest), "{name}");
        }
        assert_eq!(accept(b"garbage", T), Err(Reject::BadRequest));
        assert_eq!(accept(b"POST /message HTTP/1.1\r\n\r\n", T), Err(Reject::Forbidden), "no host");
        let short = req("POST", "/message", &[("Content-Length", "500")], MUTED);
        assert_eq!(accept(&short, T), Err(Reject::BadRequest), "the body the head promised never arrived");
        assert_eq!(accept(&req("POST", "/message", &[("Content-Length", "9999")], MUTED), T), Err(Reject::TooLarge));
        assert_eq!(accept(&req("POST", "/message", &[("Content-Length", "x")], MUTED), T), Err(Reject::BadRequest));
    }

    const EXT: Source = Source::Extension;

    fn ev(at_ms: u64, muted: bool, source: Source) -> Option<Event> {
        Some(Event { at_ms, muted, source })
    }

    #[test]
    fn reports_become_timeline_events_only_when_they_change_something() {
        let mut l = Link::default();
        assert_eq!(l.report(1, 100, Some(false)), ev(100, false, EXT), "first report");
        assert_eq!(l.report(1, 2100, Some(false)), None, "a heartbeat");
        assert_eq!(l.report(1, 4100, Some(true)), ev(4100, true, EXT), "muted in the call");
        assert_eq!(l.report(1, 6100, Some(false)), ev(6100, false, EXT), "unmuted again");
        assert_eq!(l.report(1, 8100, None), ev(8100, true, Source::Manual), "the selector broke: back to manual, kept out");
        assert_eq!(l.report(1, 10_100, None), None, "still unknown, nothing more to write");
        assert_eq!(l.report(1, 12_100, Some(false)), ev(12_100, false, EXT), "it recovers");
    }

    #[test]
    fn a_report_before_any_known_state_writes_nothing_and_a_new_meeting_starts_over() {
        let mut l = Link::default();
        assert_eq!(l.report(1, 0, None), None, "unknown from the start stays unknown");
        assert_eq!(l.report(1, 1000, Some(false)), ev(1000, false, EXT));
        assert_eq!(l.report(2, 500, Some(false)), ev(500, false, EXT), "the next recording needs its own first event");
    }

    #[test]
    fn a_silent_extension_hands_the_mic_back_to_myna_once() {
        let mut l = Link::default();
        l.report(1, 1000, Some(false));
        assert_eq!(l.expire(1, 1000 + STALE_MS - 1), None, "still fresh");
        assert_eq!(l.expire(2, 1000 + STALE_MS), None, "another meeting's link is not this one's");
        assert_eq!(l.expire(1, 1000 + STALE_MS), ev(1000, true, Source::Manual), "from where it was last heard");
        assert_eq!(l.expire(1, 1000 + 2 * STALE_MS), None, "only once");
        assert_eq!(l.report(1, 40_000, Some(false)), ev(40_000, false, EXT), "back, and it counts as a change");
        assert_eq!(Link::default().expire(1, 99_999), None);
    }

    #[test]
    fn mirrored_and_fallback_events_read_right_in_the_indicator() {
        use super::super::mute::{mic_now, state, Mic, State};
        let mut l = Link::default();
        let mut events = vec![];
        events.extend(l.report(1, 0, Some(false)));
        events.extend(l.report(1, 10_000, Some(true)));
        events.extend(l.report(1, 20_000, Some(false)));
        // Held-space push to talk is the same stream: only the parts unmuted in the call are transcript.
        assert_eq!(state(&events, 1_000, 5_000, false), State::Unmuted);
        assert_eq!(state(&events, 11_000, 15_000, false), State::Muted);
        assert_eq!(state(&events, 19_000, 22_000, false), State::Muted);
        assert_eq!(mic_now(&events, false), Mic { live: true, how: "mirrored" });
        events.extend(l.expire(1, 40_000));
        assert_eq!(mic_now(&events, false), Mic { live: false, how: "manual" });
        assert_eq!(Site::Meet.label(), "mirrored from Meet");
    }

    #[test]
    fn tokens_are_random_hex_and_kept_until_reset() {
        let (a, b) = (new_token(), new_token());
        assert!(a.len() == 32 && a.bytes().all(|c| c.is_ascii_hexdigit()) && a != b);
        let db = settings::open(std::path::Path::new(":memory:")).unwrap();
        let t = token(&db);
        assert_eq!(token(&db), t);
        assert!(same(&t, &t) && !same(&t, &a));
    }
}
