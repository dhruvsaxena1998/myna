//! The channel to Myna's browser add-on (07.1): a WebSocket on 127.0.0.1 while Meeting notes is on.
//! Only the add-on's own origin gets in, and its first message must carry the connection code.
//! The messages are in `Msg`; `extension/messages.js` is the other copy.

use rusqlite::{Connection, OptionalExtension};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::{Error, Message};

/// The only origin let in. The id comes from the `key` in `extension/manifest.json`.
pub const ORIGIN: &str = "chrome-extension://kojbonfiabdccghdeaifapdiagcdhnmk";
/// Myna takes the first free one; the add-on tries them in this order.
pub const PORTS: [u16; 4] = [47821, 47822, 47823, 47824];
/// Text messages and frames; audio (07.3) gets its own cap when it comes.
const MAX: usize = 16 * 1024;
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
// `Caption` is for 07.4.
#[allow(dead_code)]
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Msg {
    Hello { code: String },
    Welcome,
    Ping,
    Mute { state: MuteState, at: i64 },
    Caption { speaker: String, at: i64 },
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
}

struct Shared {
    db: Arc<Mutex<Connection>>,
    origin: &'static str,
    notify: Box<dyn Fn(bool) + Send + Sync>,
    /// The mute button's state and when the add-on saw it (unix ms), from the authenticated connection only.
    on_mute: Box<dyn Fn(MuteState, i64) + Send + Sync>,
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
        if let Some(c) = db.query_row("SELECT value FROM setting WHERE key = 'addon_code'", [], |r| r.get(0)).optional().map_err(err)? {
            return Ok(c);
        }
        let c = fresh_code();
        db.execute("INSERT INTO setting(key, value) VALUES ('addon_code', ?1)", [&c]).map_err(err)?;
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
        let _ = ws.send(Message::text(encode(&Msg::Welcome)));
        (self.notify)(true);

        loop {
            match kill.load(Ordering::SeqCst) {
                STAY => {}
                STOPPED => break close(&mut ws, 1001),
                WRONG_CODE => break close(&mut ws, 4001),
                _ => break close(&mut ws, 4002),
            }
            match ws.read() {
                // Later tickets route `caption` here; `ping` and unknown types need nothing.
                Ok(Message::Text(t)) => {
                    if let Some(Msg::Mute { state, at }) = decode(&t) {
                        (self.on_mute)(state, at);
                    }
                }
                Ok(Message::Binary(_)) => break close(&mut ws, 1003), // audio comes in 07.3, with its own cap
                Ok(Message::Close(_)) => break drop(ws.flush()),
                Ok(_) => {}
                Err(Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        let mut st = self.st.lock().unwrap();
        if st.conn.as_ref().is_some_and(|k| Arc::ptr_eq(k, &kill)) {
            st.conn = None;
            drop(st);
            (self.notify)(false);
        }
    }
}

impl Addon {
    pub fn new(db: Arc<Mutex<Connection>>, origin: &'static str, ports: &[u16], notify: impl Fn(bool) + Send + Sync + 'static, on_mute: impl Fn(MuteState, i64) + Send + Sync + 'static) -> Self {
        Addon { ports: ports.to_vec(), me: Arc::new(Shared { db, origin, notify: Box::new(notify), on_mute: Box::new(on_mute), st: Default::default() }) }
    }

    /// The connection code, made the first time it's asked for.
    pub fn code(&self) -> Result<String, String> {
        self.me.code()
    }

    pub fn connected(&self) -> bool {
        self.me.st.lock().unwrap().conn.is_some()
    }

    /// Cuts the add-on off: a new code, so the old one is useless, and the connection closed. Returns the new code.
    pub fn disconnect(&self) -> Result<String, String> {
        let c = fresh_code();
        self.me.db.lock().unwrap()
            .execute("INSERT INTO setting(key, value) VALUES ('addon_code', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1", [&c])
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
        let a = Addon::new(db, ORIGIN, &[0], |_| {}, |_, _| {});
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
    fn mute_messages_reach_myna_only_from_a_connection_that_said_hello() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        let got = Arc::new(Mutex::new(vec![]));
        let g = got.clone();
        let a = Addon::new(db, ORIGIN, &[0], |_| {}, move |s, at| g.lock().unwrap().push((s, at)));
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
        assert_eq!(decode(r#"{"v":2,"type":"hello","code":"X"}"#), None);
        assert_eq!(decode(r#"{"v":1,"type":"dance"}"#), None);
        assert_eq!(decode("nonsense"), None);
    }
}
