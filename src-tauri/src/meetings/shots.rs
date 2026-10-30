//! Screenshots of the meeting window while recording: pick the window, grab it on a timer, drop the frames that
//! look like the last one kept. In window-video mode the same pick feeds video.rs instead. The hash, the dedupe
//! and the choice of window are pure and tested here; screen.rs does the capture.
use super::{capture, screen, video};
use crate::{permissions, Db};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

/// Frames are scaled to this many px wide, whatever the window's size.
const WIDTH: usize = 1280;
const COLS: usize = 9;
const ROWS: usize = 8;
/// Frames whose hash differs from the last kept one in this many of 64 bits or fewer count as the same slide.
// shortcut: a guess that keeps a static slide to one frame while a changed slide gets through. Measure it on a few
// recorded calls and tune; gallery view with moving video tiles may need it higher.
const NEAR: u32 = 6;
/// How often to look at the settings while capture is off, so switching it on mid-call starts it.
const IDLE: Duration = Duration::from_secs(5);

/// Box-averages a 32-bit BGRA frame (rows `stride` bytes apart) down to 9×8 brightness values.
fn grid(bgra: &[u8], w: usize, h: usize, stride: usize) -> [u8; COLS * ROWS] {
    let mut out = [0u8; COLS * ROWS];
    if w < COLS || h < ROWS || bgra.len() < (h - 1) * stride + w * 4 {
        return out;
    }
    for gy in 0..ROWS {
        let (y0, y1) = (gy * h / ROWS, (gy + 1) * h / ROWS);
        for gx in 0..COLS {
            let (x0, x1) = (gx * w / COLS, (gx + 1) * w / COLS);
            let mut sum = 0u64;
            for y in y0..y1 {
                for px in bgra[y * stride + x0 * 4..y * stride + x1 * 4].chunks_exact(4) {
                    sum += (29 * px[0] as u64 + 150 * px[1] as u64 + 77 * px[2] as u64) >> 8;
                }
            }
            out[gy * COLS + gx] = (sum / ((y1 - y0) * (x1 - x0)) as u64) as u8;
        }
    }
    out
}

/// Difference hash: each bit is whether a cell is darker than the one to its right.
fn dhash(g: &[u8; COLS * ROWS]) -> u64 {
    let mut bits = 0u64;
    for row in g.chunks_exact(COLS) {
        for pair in row.windows(2) {
            bits = bits << 1 | (pair[0] < pair[1]) as u64;
        }
    }
    bits
}

pub fn hash_bgra(bgra: &[u8], w: usize, h: usize, stride: usize) -> u64 {
    dhash(&grid(bgra, w, h, stride))
}

/// Remembers the last kept frame and says whether the next one is worth keeping.
#[derive(Default)]
struct Dedupe(Option<u64>);

impl Dedupe {
    fn keep(&mut self, hash: u64) -> bool {
        // Against the last kept frame, not the last seen one, so a slow drift still adds a frame now and then.
        let same = self.0.is_some_and(|last| (last ^ hash).count_ones() <= NEAR);
        if !same {
            self.0 = Some(hash);
        }
        !same
    }
}

/// The window to capture: the one picked by hand while it is still open, else the biggest window of an app
/// that has the mic open (the call app).
// shortcut: biggest wins, so a browser with several windows may pick the wrong one; the popover's override fixes it.
fn target<'a>(windows: &'a [screen::Window], mic_apps: &[String], chosen: Option<u32>) -> Option<&'a screen::Window> {
    let by_hand = chosen.and_then(|id| windows.iter().find(|w| w.id == id));
    by_hand.or_else(|| {
        let area = |w: &&screen::Window| (w.w as u64 * w.h as u64, std::cmp::Reverse(w.id));
        windows.iter().filter(|w| mic_apps.iter().any(|a| a.eq_ignore_ascii_case(&w.app))).max_by_key(area)
    })
}

/// Time between screenshots, or None when there are none: capture is off, or it is window video. On unless the
/// setting says "off" or "video".
fn interval(mode: Option<String>, secs: Option<String>) -> Option<Duration> {
    let secs = secs.and_then(|s| s.parse().ok()).filter(|s| (5..=600).contains(s)).unwrap_or(15);
    (!matches!(mode.as_deref(), Some("off" | "video"))).then(|| Duration::from_secs(secs))
}

// ---- storage: one row per frame taken; `file` is empty for the ones dropped as duplicates ----

pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS screenshots (
            meeting_id INTEGER NOT NULL,
            at_ms INTEGER NOT NULL,
            file TEXT
        );
        CREATE INDEX IF NOT EXISTS screenshots_meeting ON screenshots (meeting_id, at_ms);",
    )
}

fn record(db: &Connection, meeting: i64, at_ms: u64, file: Option<&str>) -> rusqlite::Result<()> {
    db.execute("INSERT INTO screenshots (meeting_id, at_ms, file) VALUES (?1, ?2, ?3)", params![meeting, at_ms, file])?;
    Ok(())
}

#[derive(Serialize, PartialEq, Debug)]
pub struct Shot {
    pub at_ms: u64,
    pub path: String,
}

/// The frames kept, in time order, and how many were taken in all. `dir` is the meeting's screenshot folder.
pub fn list(db: &Connection, meeting: i64, dir: &Path) -> rusqlite::Result<(Vec<Shot>, usize)> {
    let rows: Vec<(u64, Option<String>)> = db
        .prepare("SELECT at_ms, file FROM screenshots WHERE meeting_id = ?1 ORDER BY at_ms")?
        .query_map([meeting], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let taken = rows.len();
    let kept = rows.into_iter().filter_map(|(at_ms, f)| Some(Shot { at_ms, path: dir.join(f?).display().to_string() })).collect();
    Ok((kept, taken))
}

// ---- the live capture ----

/// What the popover shows while screenshots are on.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Status {
    /// The window being captured, or why nothing is.
    pub text: String,
    /// The window picked by hand, if one is.
    pub chosen: Option<u32>,
    /// Recording window video rather than taking screenshots.
    pub video: bool,
}

struct Live {
    chosen: Mutex<Option<u32>>,
    /// None while capture is off.
    text: Mutex<Option<String>>,
    /// Window video is the capture mode.
    video_mode: AtomicBool,
    /// The recorder, once there is a window to record. Dropping it closes the file.
    video: Mutex<Option<video::Video>>,
    stopped: AtomicBool,
    /// Wakes the loop early: a new pick, or the stop.
    wake: Sender<()>,
}

/// The recording in progress, if any. Commands and the popover read it without touching the recorder's lock.
static LIVE: Mutex<Option<Arc<Live>>> = Mutex::new(None);

pub fn status() -> Option<Status> {
    let live = LIVE.lock().unwrap().clone()?;
    let text = live.text.lock().unwrap().clone()?;
    let chosen = *live.chosen.lock().unwrap();
    Some(Status { text, chosen, video: live.video_mode.load(Ordering::SeqCst) })
}

/// Captures the picked window from now on; None goes back to the app that has the mic.
pub fn choose(id: Option<u32>) {
    if let Some(live) = &*LIVE.lock().unwrap() {
        *live.chosen.lock().unwrap() = id;
        let _ = live.wake.send(());
    }
}

/// Dropping it ends the capture; a screenshot already in flight finishes on its own, but the video file is
/// closed before the drop returns, so quitting right after a stop never leaves it half written.
pub struct Screenshots(Arc<Live>);

impl Screenshots {
    /// `dir` is where this meeting's frames go and `video` the file window video is written to; `started` is when
    /// its audio began, so both line up with the transcript.
    pub fn start(app: &AppHandle, meeting: i64, dir: PathBuf, video: PathBuf, started: Instant) -> Self {
        let (wake, rx) = channel();
        let live = Arc::new(Live {
            chosen: Mutex::new(None),
            text: Mutex::new(None),
            video_mode: AtomicBool::new(false),
            video: Mutex::new(None),
            stopped: AtomicBool::new(false),
            wake,
        });
        *LIVE.lock().unwrap() = Some(live.clone());
        let (app, thread_live) = (app.clone(), live.clone());
        std::thread::spawn(move || run(&app, &thread_live, rx, meeting, &dir, &video, started));
        Self(live)
    }
}

impl Drop for Screenshots {
    fn drop(&mut self) {
        self.0.stopped.store(true, Ordering::SeqCst);
        *LIVE.lock().unwrap() = None;
        let _ = self.0.wake.send(());
        drop(self.0.video.lock().unwrap().take());
    }
}

fn run(app: &AppHandle, live: &Live, rx: Receiver<()>, meeting: i64, dir: &Path, video: &Path, started: Instant) {
    let (mut dedupe, mut asked) = (Dedupe::default(), false);
    while !live.stopped.load(Ordering::SeqCst) {
        // Read each round, so a change in Settings applies mid-call.
        let mode = crate::setting(app, "meetings_capture");
        let every = interval(mode.clone(), crate::setting(app, "meetings_interval"));
        let is_video = mode.as_deref() == Some("video");
        live.video_mode.store(is_video, Ordering::SeqCst);
        let text = if is_video {
            Some(tick_video(live, video, started, &mut asked))
        } else {
            // Leaving video mode closes the file; what was recorded stays.
            drop(live.video.lock().unwrap().take());
            every.map(|_| tick(app, live, meeting, dir, started, &mut dedupe, &mut asked))
        };
        *live.text.lock().unwrap() = text;
        let _ = rx.recv_timeout(every.unwrap_or(IDLE));
    }
}

/// The window to capture now, or the status line saying why there is none.
fn pick(live: &Live, asked: &mut bool) -> Result<screen::Window, String> {
    if !permissions::granted("Screen Recording") {
        // Once per recording: the recording itself carries on, so this is a nudge, not a gate.
        if !std::mem::replace(asked, true) {
            permissions::request("Screen Recording");
        }
        return Err("needs Screen Recording (Settings → Meetings)".into());
    }
    let windows = screen::windows()?;
    let chosen = *live.chosen.lock().unwrap();
    target(&windows, &capture::mic_apps(), chosen).cloned().ok_or_else(|| "waiting for a call app to use the mic".into())
}

fn label(window: &screen::Window) -> String {
    if window.title.is_empty() { window.app.clone() } else { format!("{} · {}", window.app, window.title) }
}

/// One look at which window the video should follow: starts the recorder on the first, repoints it after.
fn tick_video(live: &Live, file: &Path, started: Instant, asked: &mut bool) -> String {
    let window = match pick(live, asked) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let label = label(&window);
    let mut slot = live.video.lock().unwrap();
    if live.stopped.load(Ordering::SeqCst) {
        return label; // the stop already closed the video; do not open another
    }
    let result = match slot.as_mut() {
        Some(v) => v.capture(window.id),
        // One file per meeting: starting again would write over what was recorded before the switch.
        None if file.exists() => Err("window video was switched off earlier in this call".into()),
        None => video::Video::start(file, window.id, started).map(|v| *slot = Some(v)),
    };
    match result {
        Ok(()) => label,
        Err(e) => format!("{label}: {e}"),
    }
}

/// One capture attempt. Returns the status line: the window it grabbed, or what stopped it.
fn tick(app: &AppHandle, live: &Live, meeting: i64, dir: &Path, started: Instant, dedupe: &mut Dedupe, asked: &mut bool) -> String {
    let window = match pick(live, asked) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let label = label(&window);
    let frame = match screen::grab(window.id, WIDTH) {
        Ok(f) => f,
        Err(e) => return format!("{label}: {e}"),
    };
    let at_ms = started.elapsed().as_millis() as u64;
    let file = dedupe.keep(frame.hash).then(|| format!("{at_ms}.jpg"));
    if let Some(name) = &file {
        if let Err(e) = fs::create_dir_all(dir).and_then(|_| fs::write(dir.join(name), &frame.jpeg)) {
            return format!("could not save a screenshot: {e}");
        }
    }
    let _ = record(&app.state::<Db>().0.lock().unwrap(), meeting, at_ms, file.as_deref());
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 32-bit BGRA image whose pixels are grey levels `f(x, y)`, with padding at the end of each row.
    fn image(w: usize, h: usize, f: impl Fn(usize, usize) -> u8) -> (Vec<u8>, usize) {
        let stride = w * 4 + 8;
        let mut px = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                px[y * stride + x * 4..][..4].copy_from_slice(&[f(x, y), f(x, y), f(x, y), 255]);
            }
        }
        (px, stride)
    }

    fn hash_of(w: usize, h: usize, f: impl Fn(usize, usize) -> u8) -> u64 {
        let (px, stride) = image(w, h, f);
        hash_bgra(&px, w, h, stride)
    }

    #[test]
    fn grid_averages_each_block_and_skips_row_padding() {
        let (px, stride) = image(18, 16, |x, _| (x / 2) as u8 * 10);
        let g = grid(&px, 18, 16, stride);
        for row in g.chunks_exact(COLS) {
            assert_eq!(row, [0, 10, 20, 30, 40, 50, 60, 70, 80]);
        }
        let (px, stride) = image(18, 16, |x, _| if x % 2 == 0 { 0 } else { 100 });
        assert!(grid(&px, 18, 16, stride).iter().all(|&v| v == 50));
    }

    #[test]
    fn grid_of_a_frame_too_small_or_truncated_is_flat() {
        assert_eq!(grid(&[0; 16], 2, 2, 8), [0; COLS * ROWS]);
        let (px, stride) = image(18, 16, |_, _| 200);
        assert_eq!(grid(&px[..px.len() / 2], 18, 16, stride), [0; COLS * ROWS]);
    }

    #[test]
    fn hash_bits_say_whether_a_cell_is_darker_than_its_right_neighbour() {
        let rising = hash_of(18, 16, |x, _| (x / 2) as u8 * 10);
        let falling = hash_of(18, 16, |x, _| 200 - (x / 2) as u8 * 10);
        assert_eq!((rising, falling), (u64::MAX, 0));
        assert_eq!(hash_of(18, 16, |_, _| 90), 0); // a flat window has no edges
    }

    #[test]
    fn hash_ignores_a_blinking_cursor_but_sees_a_new_slide() {
        let slide = |x: usize, y: usize| if (x / 4 + y / 4) % 3 == 0 { 40 } else { 220 };
        let before = hash_of(72, 64, slide);
        let blink = hash_of(72, 64, |x, y| if x == 5 && y == 5 { 0 } else { slide(x, y) });
        let next = hash_of(72, 64, |x, y| slide(y, x + 4));
        assert!((before ^ blink).count_ones() <= NEAR);
        assert!((before ^ next).count_ones() > NEAR);
    }

    #[test]
    fn a_static_slide_keeps_one_frame_out_of_forty() {
        let mut d = Dedupe::default();
        assert_eq!((0..40).filter(|_| d.keep(0xDEAD_BEEF_0123_4567)).count(), 1);
    }

    #[test]
    fn near_duplicates_are_dropped_and_changed_frames_kept() {
        let mut d = Dedupe::default();
        let a = 0x0123_4567_89AB_CDEF_u64;
        assert!(d.keep(a));
        assert!(!d.keep(a ^ ((1 << NEAR) - 1)), "exactly NEAR bits differ");
        assert!(d.keep(a ^ ((1 << (NEAR + 1)) - 1)), "one bit more is a different frame");
    }

    #[test]
    fn dedupe_compares_with_the_last_kept_frame_so_slow_drift_adds_frames() {
        let mut d = Dedupe::default();
        let a = 0u64;
        assert!(d.keep(a));
        let step1 = 0b1111; // 4 bits from a: dropped
        let step2 = 0b1111_1111; // 4 from step1 but 8 from a: kept
        assert!(!d.keep(step1));
        assert!(d.keep(step2));
        assert!(!d.keep(step2 ^ 1)); // now measured against step2
    }

    fn win(id: u32, app: &str, w: u32, h: u32) -> screen::Window {
        screen::Window { id, app: app.into(), title: String::new(), w, h }
    }

    fn open() -> Vec<screen::Window> {
        vec![win(1, "Google Chrome", 1400, 900), win(2, "Google Chrome", 800, 600), win(3, "zoom.us", 1000, 700), win(4, "Slack", 1600, 1000)]
    }

    fn mic(apps: &[&str]) -> Vec<String> {
        apps.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn auto_pick_is_the_biggest_window_of_the_app_with_the_mic() {
        let w = open();
        assert_eq!(target(&w, &mic(&["Google Chrome"]), None).map(|w| w.id), Some(1));
        assert_eq!(target(&w, &mic(&["ZOOM.US"]), None).map(|w| w.id), Some(3));
        // Several apps hold the mic: the biggest of all of them.
        assert_eq!(target(&w, &mic(&["zoom.us", "Slack"]), None).map(|w| w.id), Some(4));
    }

    #[test]
    fn auto_pick_finds_nothing_when_no_app_has_the_mic() {
        assert_eq!(target(&open(), &[], None), None);
        assert_eq!(target(&open(), &mic(&["Spotify"]), None), None);
        assert_eq!(target(&[], &mic(&["Google Chrome"]), None), None);
    }

    #[test]
    fn a_window_picked_by_hand_wins_until_it_closes() {
        let w = open();
        assert_eq!(target(&w, &mic(&["Google Chrome"]), Some(3)).map(|w| w.id), Some(3));
        // Even with nothing holding the mic.
        assert_eq!(target(&w, &[], Some(2)).map(|w| w.id), Some(2));
        // Closed: back to the automatic choice.
        assert_eq!(target(&w, &mic(&["Google Chrome"]), Some(99)).map(|w| w.id), Some(1));
    }

    #[test]
    fn equal_windows_resolve_the_same_way_every_time() {
        let w = vec![win(8, "zoom.us", 900, 600), win(5, "zoom.us", 900, 600)];
        assert_eq!(target(&w, &mic(&["zoom.us"]), None).map(|w| w.id), Some(5));
    }

    #[test]
    fn interval_defaults_to_15_seconds_and_off_turns_capture_off() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(interval(None, None), Some(Duration::from_secs(15)));
        assert_eq!(interval(s("screenshots"), s("30")), Some(Duration::from_secs(30)));
        assert_eq!(interval(s("off"), s("30")), None);
        // Window video takes no screenshots, whatever the interval says.
        assert_eq!(interval(s("video"), s("30")), None);
        // Out of range or not a number: the default, never a tight loop.
        assert_eq!(interval(None, s("0")), Some(Duration::from_secs(15)));
        assert_eq!(interval(None, s("86400")), Some(Duration::from_secs(15)));
        assert_eq!(interval(None, s("soon")), Some(Duration::from_secs(15)));
    }

    #[test]
    fn frames_list_in_time_order_with_how_many_were_taken() {
        let c = Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        init(&c).unwrap(); // idempotent
        record(&c, 1, 30_000, Some("30000.jpg")).unwrap();
        record(&c, 1, 0, Some("0.jpg")).unwrap();
        record(&c, 1, 15_000, None).unwrap(); // dropped as a duplicate
        record(&c, 2, 0, Some("other.jpg")).unwrap();
        let (kept, taken) = list(&c, 1, Path::new("/shots/1")).unwrap();
        assert_eq!(taken, 3);
        assert_eq!(kept, [Shot { at_ms: 0, path: "/shots/1/0.jpg".into() }, Shot { at_ms: 30_000, path: "/shots/1/30000.jpg".into() }]);
        assert_eq!(list(&c, 3, Path::new("/shots/3")).unwrap(), (vec![], 0));
    }
}
