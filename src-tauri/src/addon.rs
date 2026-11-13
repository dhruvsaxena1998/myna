//! The channel to Myna's browser add-on (07.1): a WebSocket on 127.0.0.1 while Meeting notes is on.
//! Only the add-on's own origin gets in, and its first message must carry the connection code.
//! The messages are in `Msg` (`stop-tab` is the one hyphenated name); `extension/messages.js` is the other copy. The Teams tab's sound (07.3) comes as
//! binary frames: 4 bytes sequence (u32), 8 bytes capture time (i64, unix ms), then 16 kHz mono Int16 samples, all
//! little-endian; `extension/audio.js` is the other copy.

use crate::storage::{read_text, write_text};
use rusqlite::Connection;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::{Error, Message};

/// The only origin let in. The id comes from the `key` in `extension/manifest.json`.
pub const ORIGIN: &str = "chrome-extension://kojbonfiabdccghdeaifapdiagcdhnmk";
/// Myna takes the first free one; the add-on tries them in this order.
pub const PORTS: [u16; 4] = [47821, 47822, 47823, 47824];
/// Text messages, and everything before the connection code is right.
const MAX: usize = 16 * 1024;
/// A binary audio frame, header included: about two seconds of audio, where the add-on sends a tenth of one.
const AUDIO_MAX: usize = 64 * 1024;
const HEADER: usize = 12;
const AUDIO_RATE: f64 = 16_000.0;
const HELLO_WITHIN: Duration = Duration::from_secs(5);
/// How often a connection looks up from reading to see whether it was told to go.
const TICK: Duration = Duration::from_millis(250);
/// Why a connection is told to go, and the close code the add-on hears.
const STAY: u8 = 0;
const STOPPED: u8 = 1;
const WRONG_CODE: u8 = 2;
const REPLACED: u8 = 3;
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no I, O, 0, 1

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MuteState {
    Unmuted,
    Muted,
    Unknown,
}

impl MuteState {
    pub fn as_str(self) -> &'static str {
        match self {
            MuteState::Unmuted => "unmuted",
            MuteState::Muted => "muted",
            MuteState::Unknown => "unknown",
        }
    }
}

/// A message on the wire is `{"v":1,"type":...}` plus these fields. A type not listed fails to parse and is ignored.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Msg {
    Hello { code: String },
    Welcome,
    Ping,
    /// Which selector table the add-on reads Teams with (a date or number as text), sent on connect and when it changes.
    Selectors { version: String },
    Mute { state: MuteState, at: i64 },
    /// Teams' live captions showed `name` speaking at `at` (unix ms, the add-on's clock); `text` is optional.
    Caption { name: String, at: i64, #[serde(default)] text: Option<String> },
    /// The add-on expects captions but can't find them on the page.
    Captions { state: CaptionsState },
    /// The Teams tab's sound starts or stops coming as binary frames. `title` is the tab's, for the add-on's own use.
    Tab { state: TabState, #[serde(default)] title: String },
    /// Myna to the add-on: the meeting is over, stop capturing. Hyphenated on the wire, as the add-on listens for it.
    #[serde(rename = "stop-tab")]
    StopTab,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CaptionsState {
    Unavailable,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TabState {
    Start,
    Stop,
}

/// What the tab stream does, as the meeting hears it.
#[derive(Debug, PartialEq)]
pub enum Tab {
    Start,
    Stop,
    /// A frame: when it arrived, when it was captured (unix ms, the add-on's clock), and its samples.
    Audio { at: Instant, ts: i64, samples: Vec<f32> },
}

/// A frame's sequence number, capture time and samples; None if it isn't one.
pub fn decode_frame(b: &[u8]) -> Option<(u32, i64, Vec<f32>)> {
    let (head, pcm) = b.split_at_checked(HEADER)?;
    if pcm.is_empty() || pcm.len() % 2 != 0 {
        return None;
    }
    let seq = u32::from_le_bytes(head[..4].try_into().ok()?);
    let ts = i64::from_le_bytes(head[4..].try_into().ok()?);
    Some((seq, ts, pcm.as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes(*c) as f32 / 32768.0).collect()))
}

/// One connection's tab stream, from `tab start`.
struct Stream {
    started: Instant,
    last: Option<u32>,
    samples: u64,
}

impl Stream {
    /// The frame's audio, None if it repeats or trails one already taken; Err(close code) if it isn't a frame or the stream sends faster than it plays.
    fn take(&mut self, b: &[u8]) -> Result<Option<(i64, Vec<f32>)>, u16> {
        let (seq, ts, samples) = decode_frame(b).ok_or(1003u16)?;
        self.samples += samples.len() as u64;
        // Real time plus a 5 s head start and half again for catching up; a firehose is a bug or an attack.
        if self.samples as f64 > (self.started.elapsed().as_secs_f64() * 1.5 + 5.0) * AUDIO_RATE {
            return Err(1008);
        }
        if self.last.is_some_and(|l| seq <= l) {
            return Ok(None);
        }
        self.last = Some(seq);
        Ok(Some((ts, samples)))
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Wire<M> {
    v: u32,
    #[serde(flatten)]
    msg: M,
}

pub const VERSION: u32 = 1;

pub fn encode(msg: &Msg) -> String {
    serde_json::to_string(&Wire { v: VERSION, msg }).expect("a message always serializes")
}

pub fn decode(text: &str) -> Option<Msg> {
    serde_json::from_str::<Wire<Msg>>(text).ok().filter(|w| w.v == VERSION).map(|w| w.msg)
}

/// A live connection's "go away" reason (`STAY` while it should stay).
type Kill = Arc<AtomicU8>;

#[derive(Default)]
struct State {
    /// Set to stop the listener thread, and where it listens; None when not running.
    stop: Option<(Arc<AtomicBool>, SocketAddr)>,
    conn: Option<Kill>,
    /// The connected add-on's selector version, if it said.
    selectors: Option<String>,
}

struct Shared {
    db: Arc<Mutex<Connection>>,
    origin: &'static str,
    notify: Box<dyn Fn(bool) + Send + Sync>,
    /// The mute button's state and when the add-on saw it (unix ms), from the authenticated connection only.
    on_mute: Box<dyn Fn(MuteState, i64) + Send + Sync>,
    /// `Caption` and `Captions` messages, from the authenticated connection only.
    on_caption: Box<dyn Fn(Msg) + Send + Sync>,
    /// The tab stream, from the authenticated connection only.
    on_tab: Box<dyn Fn(Tab) + Send + Sync>,
    /// Set to tell the connection to send `stop-tab`.
    stop_tab: AtomicBool,
    st: Mutex<State>,
}

pub struct Addon {
    ports: Vec<u16>,
    me: Arc<Shared>,
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

fn fresh_code() -> String {
    let mut b = [0u8; 6];
    getrandom::fill(&mut b).expect("the OS has random numbers");
    let c = |i: usize| ALPHABET[(b[i] & 31) as usize] as char;
    format!("MYNA-{}{}{}{}-{}{}", c(0), c(1), c(2), c(3), c(4), c(5))
}

fn close(ws: &mut tungstenite::WebSocket<TcpStream>, code: u16) {
    let _ = ws.close(Some(CloseFrame { code: CloseCode::from(code), reason: "".into() }));
    let _ = ws.flush();
}

impl Shared {
    fn code(&self) -> Result<String, String> {
        let db = self.db.lock().unwrap();
        let err = |e: rusqlite::Error| format!("Couldn't read the add-on's connection code: {e}");
        if let Some(c) = read_text(&db, "addon_code").map_err(err)? {
            return Ok(c);
        }
        let c = fresh_code();
        write_text(&db, "addon_code", &c).map_err(err)?;
        Ok(c)
    }

    // The error type is tungstenite's callback contract.
    #[allow(clippy::result_large_err)]
    /// One connection, from the upgrade to the end. The first message has to be the right `hello`.
    fn serve(&self, stream: TcpStream) {
        let _ = stream.set_nonblocking(false); // accepted sockets inherit the listener's
        let _ = stream.set_read_timeout(Some(HELLO_WITHIN));
        let _ = stream.set_write_timeout(Some(HELLO_WITHIN));
        let origin = self.origin;
        // Before the upgrade, so a web page's attempt never becomes a WebSocket. A missing Origin is refused too.
        let check = |req: &tungstenite::handshake::server::Request, resp| {
            if req.headers().get("origin").is_some_and(|o| o.as_bytes() == origin.as_bytes()) {
                Ok(resp)
            } else {
                let mut no = tungstenite::handshake::server::ErrorResponse::new(None);
                *no.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
                Err(no)
            }
        };
        let cfg = WebSocketConfig::default().max_message_size(Some(MAX)).max_frame_size(Some(MAX));
        let Ok(mut ws) = tungstenite::accept_hdr_with_config(stream, check, Some(cfg)) else { return };

        let right = match ws.read() {
            Ok(Message::Text(t)) => match (decode(&t), self.code()) {
                (Some(Msg::Hello { code }), Ok(want)) => same(code.as_bytes(), want.as_bytes()),
                _ => false,
            },
            _ => return,
        };
        if !right {
            return close(&mut ws, 4001);
        }

        let kill: Kill = Arc::new(AtomicU8::new(STAY));
        if let Some(old) = self.st.lock().unwrap().conn.replace(kill.clone()) {
            old.store(REPLACED, Ordering::SeqCst); // the older browser is told, so the two don't keep taking turns
        }
        let _ = ws.get_ref().set_read_timeout(Some(TICK));
        // Audio frames only after the code was right.
        ws.set_config(|c| {
            c.max_message_size = Some(AUDIO_MAX);
            c.max_frame_size = Some(AUDIO_MAX);
        });
        let _ = ws.send(Message::text(encode(&Msg::Welcome)));
        (self.notify)(true);
        let mut stream: Option<Stream> = None;

        loop {
            match kill.load(Ordering::SeqCst) {
                STAY => {}
                STOPPED => break close(&mut ws, 1001),
                WRONG_CODE => break close(&mut ws, 4001),
                _ => break close(&mut ws, 4002),
            }
            if self.stop_tab.swap(false, Ordering::SeqCst) {
                let _ = ws.send(Message::text(encode(&Msg::StopTab)));
            }
            match ws.read() {
                Ok(Message::Text(t)) if t.len() > MAX => break close(&mut ws, 1009),
                // `ping` and unknown types need nothing.
                Ok(Message::Text(t)) => match decode(&t) {
                    Some(Msg::Mute { state, at }) => (self.on_mute)(state, at),
                    Some(Msg::Selectors { version }) => {
                        let v: String = version.chars().filter(|c| !c.is_control()).take(32).collect();
                        self.st.lock().unwrap().selectors = (!v.is_empty()).then_some(v);
                        (self.notify)(true); // Settings asks again for what it shows
                    }
                    Some(m @ (Msg::Caption { .. } | Msg::Captions { .. })) => (self.on_caption)(m),
                    Some(Msg::Tab { state: TabState::Start, .. }) => {
                        stream = Some(Stream { started: Instant::now(), last: None, samples: 0 });
                        (self.on_tab)(Tab::Start);
                    }
                    Some(Msg::Tab { state: TabState::Stop, .. }) if stream.take().is_some() => (self.on_tab)(Tab::Stop),
                    _ => {}
                },
                Ok(Message::Binary(b)) => {
                    let Some(s) = stream.as_mut() else { break close(&mut ws, 1003) }; // no `tab start`: not audio we asked for
                    match s.take(&b) {
                        Ok(Some((ts, samples))) => (self.on_tab)(Tab::Audio { at: Instant::now(), ts, samples }),
                        Ok(None) => {}
                        Err(code) => break close(&mut ws, code),
                    }
                }
                Ok(Message::Close(_)) => break drop(ws.flush()),
                Ok(_) => {}
                Err(Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        if stream.is_some() {
            (self.on_tab)(Tab::Stop); // the process tap takes over again
        }
        let mut st = self.st.lock().unwrap();
        if st.conn.as_ref().is_some_and(|k| Arc::ptr_eq(k, &kill)) {
            st.conn = None;
            st.selectors = None;
            drop(st);
            (self.notify)(false);
        }
    }
}

impl Addon {
    pub fn new(db: Arc<Mutex<Connection>>, origin: &'static str, ports: &[u16], notify: impl Fn(bool) + Send + Sync + 'static, on_mute: impl Fn(MuteState, i64) + Send + Sync + 'static, on_caption: impl Fn(Msg) + Send + Sync + 'static, on_tab: impl Fn(Tab) + Send + Sync + 'static) -> Self {
        Addon { ports: ports.to_vec(), me: Arc::new(Shared { db, origin, notify: Box::new(notify), on_mute: Box::new(on_mute), on_caption: Box::new(on_caption), on_tab: Box::new(on_tab), stop_tab: AtomicBool::new(false), st: Default::default() }) }
    }

    /// Tells the add-on the meeting is over, so it stops capturing the tab.
    pub fn stop_tab(&self) {
        if self.me.st.lock().unwrap().conn.is_some() {
            self.me.stop_tab.store(true, Ordering::SeqCst);
        }
    }

    /// The connection code, made the first time it's asked for.
    pub fn code(&self) -> Result<String, String> {
        self.me.code()
    }

    pub fn connected(&self) -> bool {
        self.me.st.lock().unwrap().conn.is_some()
    }

    /// The selector version the connected add-on says it uses.
    pub fn selectors(&self) -> Option<String> {
        self.me.st.lock().unwrap().selectors.clone()
    }

    /// Cuts the add-on off: a new code, so the old one is useless, and the connection closed. Returns the new code.
    pub fn disconnect(&self) -> Result<String, String> {
        let c = fresh_code();
        write_text(&self.me.db.lock().unwrap(), "addon_code", &c)
            .map_err(|e| format!("Couldn't make a new connection code: {e}"))?;
        if let Some(k) = &self.me.st.lock().unwrap().conn {
            k.store(WRONG_CODE, Ordering::SeqCst);
        }
        Ok(c)
    }

    /// Starts listening, on 127.0.0.1 only and never anywhere else. Does nothing if already listening.
    pub fn start(&self) -> Result<SocketAddr, String> {
        let mut st = self.me.st.lock().unwrap();
        if let Some((_, addr)) = &st.stop {
            return Ok(*addr);
        }
        let listener = self.ports.iter().find_map(|p| TcpListener::bind((Ipv4Addr::LOCALHOST, *p)).ok()).ok_or("Couldn't open a port for the browser add-on.")?;
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        // shortcut: the listener polls, so stopping takes up to 50 ms to land; fine until something needs a blocking accept.
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        st.stop = Some((stop.clone(), addr));
        let me = self.me.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((s, _)) => {
                        let me = me.clone();
                        std::thread::spawn(move || me.serve(s));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(50)),
                }
            }
        });
        Ok(addr)
    }

    /// Stops listening and drops the connection.
    pub fn stop(&self) {
        let mut st = self.me.st.lock().unwrap();
        if let Some((s, _)) = st.stop.take() {
            s.store(true, Ordering::SeqCst);
        }
        if let Some(k) = &st.conn {
            k.store(STOPPED, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungstenite::client::IntoClientRequest;

    type Client = tungstenite::WebSocket<TcpStream>;

    fn addon() -> (Addon, SocketAddr, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        let a = Addon::new(db, ORIGIN, &[0], |_| {}, |_, _| {}, |_| {}, |_| {});
        let addr = a.start().unwrap();
        (a, addr, tmp)
    }

    fn dial(addr: SocketAddr, origin: Option<&str>) -> Result<Client, tungstenite::Error> {
        let mut req = format!("ws://{addr}").into_client_request().unwrap();
        if let Some(o) = origin {
            req.headers_mut().insert("origin", o.parse().unwrap());
        }
        let s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        tungstenite::client(req, s).map(|(ws, _)| ws).map_err(|e| match e {
            tungstenite::HandshakeError::Failure(e) => e,
            tungstenite::HandshakeError::Interrupted(_) => unreachable!("blocking socket"),
        })
    }

    fn hello(ws: &mut Client, code: &str) {
        ws.send(Message::text(encode(&Msg::Hello { code: code.into() }))).unwrap();
    }

    /// True when the next thing is Myna's welcome.
    fn welcomed(ws: &mut Client) -> bool {
        matches!(ws.read(), Ok(Message::Text(t)) if decode(&t) == Some(Msg::Welcome))
    }

    /// The close code Myna sent (Some(None) if the socket just dropped), after any messages in between.
    fn closed(ws: &mut Client) -> Option<u16> {
        loop {
            match ws.read() {
                Ok(Message::Close(f)) => return Some(f.map_or(0, |f| f.code.into())),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return None,
                Err(_) => return Some(0),
            }
        }
    }

    fn wait(what: &str, mut f: impl FnMut() -> bool) {
        for _ in 0..60 {
            if f() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("never happened: {what}");
    }

    fn connect(a: &Addon, addr: SocketAddr) -> Client {
        let mut ws = dial(addr, Some(ORIGIN)).unwrap();
        hello(&mut ws, &a.code().unwrap());
        assert!(welcomed(&mut ws));
        ws
    }

    #[test]
    fn stop_tab_goes_out_as_the_name_the_addon_listens_for() {
        assert_eq!(encode(&Msg::StopTab), r#"{"v":1,"type":"stop-tab"}"#);
        assert_eq!(decode(r#"{"v":1,"type":"stop-tab"}"#), Some(Msg::StopTab));
        let names = [
            Msg::Hello { code: "X".into() },
            Msg::Welcome,
            Msg::Ping,
            Msg::Selectors { version: "1".into() },
            Msg::Mute { state: MuteState::Muted, at: 1 },
            Msg::Caption { name: "A".into(), at: 1, text: None },
            Msg::Captions { state: CaptionsState::Unavailable },
            Msg::Tab { state: TabState::Start, title: String::new() },
        ]
        .map(|m| serde_json::from_str::<serde_json::Value>(&encode(&m)).unwrap()["type"].as_str().unwrap().to_owned());
        assert_eq!(names, ["hello", "welcome", "ping", "selectors", "mute", "caption", "captions", "tab"]);
    }

    #[test]
    fn the_selector_version_is_kept_while_connected() {
        let (a, addr, _tmp) = addon();
        assert_eq!(a.selectors(), None);
        let mut ws = connect(&a, addr);
        ws.send(Message::text(encode(&Msg::Selectors { version: "2026-11-10".into() }))).unwrap();
        wait("the version", || a.selectors().as_deref() == Some("2026-11-10"));
        drop(ws);
        wait("forgotten on disconnect", || a.selectors().is_none());
    }

    #[test]
    fn mute_messages_reach_myna_only_from_a_connection_that_said_hello() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        let got = Arc::new(Mutex::new(vec![]));
        let g = got.clone();
        let a = Addon::new(db, ORIGIN, &[0], |_| {}, move |s, at| g.lock().unwrap().push((s, at)), |_| {}, |_| {});
        let addr = a.start().unwrap();
        let mut stranger = dial(addr, Some(ORIGIN)).unwrap();
        stranger.send(Message::text(encode(&Msg::Mute { state: MuteState::Muted, at: 1 }))).unwrap();
        assert!(closed(&mut stranger).is_some());
        let mut ws = connect(&a, addr);
        ws.send(Message::text(encode(&Msg::Mute { state: MuteState::Muted, at: 7 }))).unwrap();
        wait("heard", || !got.lock().unwrap().is_empty());
        assert_eq!(*got.lock().unwrap(), [(MuteState::Muted, 7)]);
    }

    #[test]
    fn caption_messages_reach_myna_only_from_a_connection_that_said_hello() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        let got = Arc::new(Mutex::new(vec![]));
        let g = got.clone();
        let a = Addon::new(db, ORIGIN, &[0], |_| {}, |_, _| {}, move |m| g.lock().unwrap().push(m), |_| {});
        let addr = a.start().unwrap();
        let mut stranger = dial(addr, Some(ORIGIN)).unwrap();
        stranger.send(Message::text(r#"{"v":1,"type":"caption","name":"Eve","at":1}"#)).unwrap();
        assert!(closed(&mut stranger).is_some());
        let mut ws = connect(&a, addr);
        ws.send(Message::text(r#"{"v":1,"type":"caption","name":"Ana","at":7,"text":"hi"}"#)).unwrap();
        ws.send(Message::text(r#"{"v":1,"type":"captions","state":"unavailable"}"#)).unwrap();
        wait("heard", || got.lock().unwrap().len() == 2);
        assert_eq!(*got.lock().unwrap(), [Msg::Caption { name: "Ana".into(), at: 7, text: Some("hi".into()) }, Msg::Captions { state: CaptionsState::Unavailable }]);
    }

    #[test]
    fn listens_on_loopback_only() {
        let (a, addr, _t) = addon();
        assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);
        assert_eq!(a.start().unwrap(), addr, "starting twice keeps the one listener");
    }

    #[test]
    fn a_wrong_or_missing_origin_never_upgrades() {
        let (_a, addr, _t) = addon();
        for o in [None, Some("https://teams.microsoft.com"), Some("chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), Some("chrome-extension://kojbonfiabdccghdeaifapdiagcdhnmk/")] {
            assert!(dial(addr, o).is_err(), "{o:?}");
        }
    }

    #[test]
    fn the_wrong_code_or_no_hello_closes_the_socket() {
        let (a, addr, _t) = addon();
        let mut ws = dial(addr, Some(ORIGIN)).unwrap();
        hello(&mut ws, "MYNA-AAAA-AA");
        assert_eq!(closed(&mut ws), Some(4001));
        let mut ws = dial(addr, Some(ORIGIN)).unwrap();
        ws.send(Message::text(r#"{"v":1,"type":"ping"}"#)).unwrap();
        assert!(closed(&mut ws).is_some());
        let mut ws = dial(addr, Some(ORIGIN)).unwrap();
        ws.send(Message::binary(vec![1u8])).unwrap();
        assert!(closed(&mut ws).is_some());
        assert!(!a.connected());
    }

    #[test]
    fn the_right_code_connects_and_status_follows() {
        let (a, addr, _t) = addon();
        let mut ws = connect(&a, addr);
        assert!(a.connected());
        // Unknown types and pings are ignored, not fatal.
        ws.send(Message::text(r#"{"v":1,"type":"from-the-future"}"#)).unwrap();
        ws.send(Message::text(encode(&Msg::Ping))).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(a.connected());
        drop(ws);
        wait("status flips back", || !a.connected());
    }

    #[test]
    fn oversized_text_and_binary_frames_are_refused() {
        let (a, addr, _t) = addon();
        let mut ws = connect(&a, addr);
        ws.send(Message::text("x".repeat(MAX + 1))).unwrap();
        assert!(closed(&mut ws).is_some());
        wait("dropped", || !a.connected());
        let mut ws = connect(&a, addr);
        ws.send(Message::binary(vec![0u8; 8])).unwrap();
        assert_eq!(closed(&mut ws), Some(1003));
        wait("dropped", || !a.connected());
    }

    fn frame(seq: u32, ts: i64, n: usize) -> Message {
        let mut b = seq.to_le_bytes().to_vec();
        b.extend(ts.to_le_bytes());
        b.extend(std::iter::repeat_n([0x00, 0x40], n).flatten()); // 0.5
        Message::binary(b)
    }

    fn tab(state: TabState) -> Message {
        Message::text(encode(&Msg::Tab { state, title: "Standup".into() }))
    }

    type Heard = Arc<Mutex<Vec<(&'static str, i64, usize)>>>;

    /// An add-on with a connection, and the tab events Myna heard from it as (kind, ts, samples).
    fn tabbed() -> (Addon, Client, Heard, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        let got = Arc::new(Mutex::new(vec![]));
        let g = got.clone();
        let a = Addon::new(db, ORIGIN, &[0], |_| {}, |_, _| {}, |_| {}, move |t| {
            g.lock().unwrap().push(match t {
                Tab::Start => ("start", 0, 0),
                Tab::Stop => ("stop", 0, 0),
                Tab::Audio { ts, samples, .. } => ("audio", ts, samples.len()),
            })
        });
        let addr = a.start().unwrap();
        let ws = connect(&a, addr);
        (a, ws, got, tmp)
    }

    #[test]
    fn tab_frames_in_order_reach_myna_and_repeats_and_stale_ones_are_dropped() {
        let (a, mut ws, got, _t) = tabbed();
        ws.send(tab(TabState::Start)).unwrap();
        for (seq, ts) in [(0, 1000), (1, 1100), (1, 1100), (0, 1000), (3, 1300)] {
            ws.send(frame(seq, ts, 1600)).unwrap(); // seq 2 was lost: the capture time shows the gap
        }
        ws.send(tab(TabState::Stop)).unwrap();
        wait("heard", || got.lock().unwrap().len() == 5);
        assert_eq!(*got.lock().unwrap(), [("start", 0, 0), ("audio", 1000, 1600), ("audio", 1100, 1600), ("audio", 1300, 1600), ("stop", 0, 0)]);
        assert!(a.connected());
    }

    #[test]
    fn audio_before_tab_start_or_after_tab_stop_closes_the_socket() {
        let (_a, mut ws, got, _t) = tabbed();
        ws.send(frame(0, 1, 160)).unwrap();
        assert_eq!(closed(&mut ws), Some(1003));
        assert!(got.lock().unwrap().is_empty());
        let (_a, mut ws, _got, _t) = tabbed();
        ws.send(tab(TabState::Start)).unwrap();
        ws.send(tab(TabState::Stop)).unwrap();
        ws.send(frame(0, 1, 160)).unwrap();
        assert_eq!(closed(&mut ws), Some(1003));
    }

    #[test]
    fn a_stranger_cant_send_audio() {
        let (_a, addr, _t) = addon();
        let mut ws = dial(addr, Some(ORIGIN)).unwrap();
        ws.send(tab(TabState::Start)).unwrap();
        ws.send(frame(0, 1, 160)).unwrap();
        assert!(closed(&mut ws).is_some());
    }

    #[test]
    fn an_oversized_frame_a_malformed_one_and_a_firehose_are_refused() {
        let (_a, mut ws, got, _t) = tabbed();
        ws.send(tab(TabState::Start)).unwrap();
        ws.send(Message::binary(vec![0u8; AUDIO_MAX + 1])).unwrap();
        assert!(closed(&mut ws).is_some());
        wait("stop", || got.lock().unwrap().last().is_some_and(|e| e.0 == "stop"));
        let (_a, mut ws, _got, _t) = tabbed();
        ws.send(tab(TabState::Start)).unwrap();
        ws.send(Message::binary(vec![0u8; HEADER + 3])).unwrap();
        assert_eq!(closed(&mut ws), Some(1003), "an odd byte is half a sample");
        let (_a, mut ws, got, _t) = tabbed();
        ws.send(tab(TabState::Start)).unwrap();
        // A full-size frame is two seconds of audio; five of them at once is ten seconds in no time.
        for seq in 0..5 {
            let _ = ws.send(frame(seq, seq as i64, (AUDIO_MAX - HEADER) / 2)); // the socket may already be shut
        }
        assert_eq!(closed(&mut ws), Some(1008));
        assert!(got.lock().unwrap().iter().filter(|e| e.0 == "audio").count() < 5);
    }

    #[test]
    fn a_dropped_connection_ends_the_tab_stream_and_stop_tab_reaches_the_addon() {
        let (a, mut ws, got, _t) = tabbed();
        ws.send(tab(TabState::Start)).unwrap();
        a.stop_tab();
        let told = (0..3).any(|_| matches!(ws.read(), Ok(Message::Text(t)) if t.as_str() == r#"{"v":1,"type":"stop-tab"}"#));
        assert!(told);
        drop(ws);
        wait("stop", || got.lock().unwrap().last().is_some_and(|e| e.0 == "stop"));
    }

    #[test]
    fn a_newer_connection_replaces_the_older() {
        let (a, addr, _t) = addon();
        let mut first = connect(&a, addr);
        let _second = connect(&a, addr);
        assert_eq!(closed(&mut first), Some(4002));
        assert!(a.connected());
    }

    #[test]
    fn disconnect_rotates_the_code_and_drops_the_connection() {
        let (a, addr, _t) = addon();
        let old = a.code().unwrap();
        let mut ws = connect(&a, addr);
        let new = a.disconnect().unwrap();
        assert_ne!(old, new);
        assert_eq!(a.code().unwrap(), new);
        assert_eq!(closed(&mut ws), Some(4001));
        wait("status flips", || !a.connected());
        let mut late = dial(addr, Some(ORIGIN)).unwrap();
        hello(&mut late, &old);
        assert_eq!(closed(&mut late), Some(4001), "the old code is dead");
    }

    #[test]
    fn stopping_drops_the_connection() {
        let (a, addr, _t) = addon();
        let mut ws = connect(&a, addr);
        a.stop();
        assert_eq!(closed(&mut ws), Some(1001));
        wait("status flips", || !a.connected());
    }

    #[test]
    fn codes_look_like_the_design_and_last() {
        let (a, _addr, _t) = addon();
        let c = a.code().unwrap();
        let ok = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| ALPHABET.contains(&b));
        let (head, rest) = c.split_once('-').unwrap();
        let (mid, tail) = rest.split_once('-').unwrap();
        assert!(head == "MYNA" && ok(mid, 4) && ok(tail, 2), "{c}");
        assert_eq!(a.code().unwrap(), c);
    }

    #[test]
    fn messages_are_versioned_and_unknown_ones_ignored() {
        assert_eq!(decode(r#"{"v":1,"type":"hello","code":"X"}"#), Some(Msg::Hello { code: "X".into() }));
        assert_eq!(decode(r#"{"v":1,"type":"mute","state":"muted","at":5}"#), Some(Msg::Mute { state: MuteState::Muted, at: 5 }));
        assert_eq!(decode(r#"{"v":1,"type":"selectors","version":"2026-11-10"}"#), Some(Msg::Selectors { version: "2026-11-10".into() }));
        assert_eq!(decode(r#"{"v":2,"type":"hello","code":"X"}"#), None);
        assert_eq!(decode(r#"{"v":1,"type":"dance"}"#), None);
        assert_eq!(decode("nonsense"), None);
    }
}
