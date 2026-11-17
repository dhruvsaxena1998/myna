//! Calls without the extension (#214): which call apps hold the mic, read from Core Audio every 500 ms. A call app
//! that holds it long enough is offered as a meeting; during a recording, the call app leaving starts a 30 s
//! countdown to a stop the user can cancel. The timing lives in `Watcher`, which tests drive with a fake clock.

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, c_char, c_void};

/// One app using the mic or playing sound, as Core Audio sees it (helpers folded into their app).
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct App {
    pub bundle: String,
    pub name: String,
    pub input: bool,
    pub output: bool,
}

unsafe extern "C" {
    fn myna_audio_apps() -> *mut c_char;
    fn free(p: *mut c_void);
}

/// The apps using the mic or playing sound now; none when Core Audio couldn't be read.
pub fn read() -> Option<Vec<App>> {
    let p = unsafe { myna_audio_apps() };
    if p.is_null() {
        return None;
    }
    let json = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { free(p.cast()) };
    serde_json::from_str(&json).ok()
}

const SLACK: &str = "com.tinyspeck.slackmacgap";
/// The call apps: bundle, how the offer names it, how long it must hold the mic (ms). Slack and Teams wait 1 s, so the
/// offer shows as the call starts (a clip held longer is offered too). Browsers wait 20 s because
/// voice notes and dictation fields open the mic for a few seconds; sitting in a Gather space holds it too.
const CALL_APPS: [(&str, &str, i64); 11] = [
    (SLACK, "Slack", 1_000),
    ("com.microsoft.teams2", "Microsoft Teams", 1_000),
    ("com.microsoft.teams", "Microsoft Teams", 1_000),
    ("com.gather.GatherV2", "Gather", 20_000),
    ("com.gather.Gather", "Gather", 20_000),
    ("com.google.Chrome", "Call in Chrome", 20_000),
    ("company.thebrowser.Browser", "Call in Arc", 20_000),
    ("com.brave.Browser", "Call in Brave", 20_000),
    ("com.microsoft.edgemac", "Call in Edge", 20_000),
    ("com.apple.Safari", "Call in Safari", 20_000),
    ("org.mozilla.firefox", "Call in Firefox", 20_000),
];

fn call_app(bundle: &str) -> Option<(&'static str, i64)> {
    // Safari's audio can show up as its WebKit GPU process.
    let bundle = if bundle == "com.apple.WebKit.GPU" { "com.apple.Safari" } else { bundle };
    CALL_APPS.iter().find(|a| a.0 == bundle).map(|a| (a.1, a.2))
}

/// A drop this short in holding the mic doesn't break a run.
const DROP: i64 = 2_000;
/// A gap this long between polls (sleep) restarts every clock.
const GAP: i64 = 10_000;
/// A dismissed app is asked about again only after it's been off the mic this long.
const SNOOZE: i64 = 60_000;
/// The call app has been gone this long: the countdown starts.
const ABSENT: i64 = 3_000;
/// How long the countdown gives the user, or the app, to come back.
const GRACE: i64 = 30_000;

/// The offer: "Meeting detected" (or "Huddle detected"), what it is, and the app's name to tag the meeting with.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Offer {
    pub title: &'static str,
    pub label: String,
    pub name: String,
}

/// What the watcher wants shown or done.
#[derive(Clone, Debug, PartialEq)]
pub enum Do {
    Offer(Offer),
    /// The offer goes away: the call app left, or a recording started.
    Withdraw,
    /// "Call ended. Stopping in 30 s": the stop comes at this time (unix ms).
    Countdown(i64),
    /// The countdown is off: the app came back, the user kept recording, or the recording stopped.
    Cancel,
    Stop,
}

/// Since when, and until when last, an app has held the mic.
struct Run {
    since: i64,
    last: i64,
}

/// The call being recorded: its app, when it was last in the call, and the countdown's end if one is on.
struct End {
    bundle: String,
    present: i64,
    deadline: Option<i64>,
}

#[derive(Default)]
pub struct Watcher {
    runs: HashMap<String, Run>,
    names: HashMap<String, String>,
    snoozed: HashSet<String>,
    offered: Option<String>,
    end: Option<End>,
    /// Keep recording: no end watch for the rest of this recording.
    kept: bool,
    recording: bool,
    last: Option<i64>,
}

impl Watcher {
    /// One poll at `now` (unix ms). `apps` is none when the read failed: that counts as "still in the call", so an
    /// error never stops a recording. `offer` and `stop` are the two settings; `offer` is also off while dictating.
    pub fn poll(&mut self, now: i64, apps: Option<&[App]>, recording: bool, offer: bool, stop: bool) -> Vec<Do> {
        let mut out = vec![];
        if self.last.is_some_and(|l| now - l > GAP) {
            self.runs.clear();
            if let Some(e) = &mut self.end {
                e.present = now;
            }
        }
        self.last = Some(now);
        if let Some(apps) = apps {
            for a in apps.iter().filter(|a| a.input && call_app(&a.bundle).is_some()) {
                let r = self.runs.entry(a.bundle.clone()).or_insert(Run { since: now, last: now });
                if now - r.last > DROP {
                    r.since = now;
                }
                r.last = now;
                self.names.insert(a.bundle.clone(), a.name.clone());
            }
            self.runs.retain(|_, r| now - r.last < SNOOZE);
            let runs = &self.runs;
            self.snoozed.retain(|b| runs.contains_key(b));
        }
        if recording != self.recording {
            self.recording = recording;
            if recording {
                self.watch(now);
            } else if self.end.take().is_some_and(|e| e.deadline.is_some()) {
                out.push(Do::Cancel);
            }
        }
        self.offer(now, offer && !recording, &mut out);
        if recording {
            self.end(now, apps, stop, &mut out);
        }
        out
    }

    /// A recording started: watch the offered call app, else the call app holding the mic; none, no end watch.
    /// That app won't be offered again until it has been off the mic a while.
    fn watch(&mut self, now: i64) {
        self.kept = false;
        let held = self.offered.clone().or_else(|| {
            let mut held: Vec<_> = self.runs.iter().filter(|(_, r)| r.last == now).map(|(b, _)| b.clone()).collect();
            held.sort();
            held.into_iter().next()
        });
        self.end = held.map(|bundle| {
            self.snoozed.insert(bundle.clone());
            End { bundle, present: now, deadline: None }
        });
    }

    fn offer(&mut self, now: i64, on: bool, out: &mut Vec<Do>) {
        if let Some(b) = &self.offered {
            let gone = self.runs.get(b).is_none_or(|r| now - r.last > DROP);
            if !on || gone {
                self.offered = None;
                out.push(Do::Withdraw);
            }
        }
        if !on || self.offered.is_some() {
            return;
        }
        let mut ready: Vec<_> = self
            .runs
            .iter()
            .filter(|(b, r)| r.last == now && !self.snoozed.contains(*b) && call_app(b).is_some_and(|(_, hold)| now - r.since >= hold))
            .map(|(b, _)| b.clone())
            .collect();
        ready.sort();
        if let Some(b) = ready.into_iter().next() {
            let (label, _) = call_app(&b).unwrap();
            let name = self.names.get(&b).cloned().unwrap_or_default();
            let title = if b == SLACK { "Huddle detected" } else { "Meeting detected" };
            out.push(Do::Offer(Offer { title, label: label.into(), name }));
            self.offered = Some(b);
        }
    }

    fn end(&mut self, now: i64, apps: Option<&[App]>, stop: bool, out: &mut Vec<Do>) {
        let Some(e) = &mut self.end else { return };
        if !stop || self.kept {
            if e.deadline.take().is_some() {
                out.push(Do::Cancel);
            }
            return;
        }
        // In the call while it holds the mic or plays sound: apps that let go of the mic when muted still play.
        let present = apps.is_none_or(|apps| apps.iter().any(|a| a.bundle == e.bundle && (a.input || a.output)));
        if present {
            e.present = now;
            if e.deadline.take().is_some() {
                out.push(Do::Cancel);
            }
        } else if e.deadline.is_none() && now - e.present >= ABSENT {
            e.deadline = Some(now + GRACE);
            out.push(Do::Countdown(now + GRACE));
        }
        if e.deadline.is_some_and(|d| now >= d) {
            self.end = None;
            out.push(Do::Stop);
        }
    }

    /// Whether the offer or a countdown is on screen.
    pub fn showing(&self) -> bool {
        self.offered.is_some() || self.end.as_ref().is_some_and(|e| e.deadline.is_some())
    }

    /// The offer was dismissed: that app isn't asked about again until it's been off the mic for a minute.
    pub fn dismiss(&mut self) {
        if let Some(b) = self.offered.take() {
            self.snoozed.insert(b);
        }
    }

    /// The offered call's app name, to tag the meeting it starts with.
    pub fn offered_name(&self) -> Option<String> {
        self.offered.as_ref().and_then(|b| self.names.get(b).cloned())
    }

    /// Keep recording: the countdown is off, and no end watch for the rest of this recording.
    pub fn keep(&mut self) {
        self.kept = true;
        if let Some(e) = &mut self.end {
            e.deadline = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME: &str = "com.google.Chrome";
    const TEAMS: &str = "com.microsoft.teams2";

    fn app(bundle: &str, input: bool, output: bool) -> App {
        App { bundle: bundle.into(), name: bundle.rsplit('.').next().unwrap().into(), input, output }
    }
    fn mic(bundle: &str) -> Vec<App> {
        vec![app(bundle, true, true)]
    }

    /// Polls every 500 ms from `from` to `to` (inclusive) with the same apps, and gathers what came out.
    fn run(w: &mut Watcher, from: i64, to: i64, apps: Option<&[App]>, recording: bool) -> Vec<(i64, Do)> {
        (from..=to).step_by(500).flat_map(|t| w.poll(t, apps, recording, true, true).into_iter().map(move |d| (t, d))).collect()
    }

    fn offered(d: &[(i64, Do)]) -> Vec<(i64, String, &'static str)> {
        d.iter().filter_map(|(t, d)| if let Do::Offer(o) = d { Some((*t, o.label.clone(), o.title)) } else { None }).collect()
    }

    #[test]
    fn slack_and_teams_are_offered_after_1_s_and_a_browser_after_20_s() {
        let mut w = Watcher::default();
        assert_eq!(offered(&run(&mut w, 0, 5_000, Some(&mic(SLACK)), false)), [(1_000, "Slack".into(), "Huddle detected")]);
        let mut w = Watcher::default();
        assert_eq!(offered(&run(&mut w, 0, 5_000, Some(&mic(TEAMS)), false)), [(1_000, "Microsoft Teams".into(), "Meeting detected")]);
        let mut w = Watcher::default();
        assert_eq!(offered(&run(&mut w, 0, 25_000, Some(&mic(CHROME)), false)), [(20_000, "Call in Chrome".into(), "Meeting detected")]);
    }

    #[test]
    fn other_apps_and_short_browser_use_are_never_offered() {
        let mut w = Watcher::default();
        assert!(offered(&run(&mut w, 0, 60_000, Some(&mic("com.apple.VoiceMemos")), false)).is_empty());
        // A browser voice note: 6 s on the mic, off for 5 s, again.
        let mut w = Watcher::default();
        let mut d = run(&mut w, 0, 6_000, Some(&mic(CHROME)), false);
        d.extend(run(&mut w, 6_500, 11_000, Some(&[]), false));
        d.extend(run(&mut w, 11_500, 17_000, Some(&mic(CHROME)), false));
        assert!(offered(&d).is_empty());
    }

    #[test]
    fn a_drop_of_up_to_2_s_keeps_the_run_and_a_longer_one_restarts_it() {
        let mut w = Watcher::default();
        let mut d = run(&mut w, 0, 10_000, Some(&mic(CHROME)), false);
        d.extend(run(&mut w, 10_500, 12_000, Some(&[]), false)); // last seen 10 s, back at 12.5 s: 2.5 s apart is > 2 s
        d.extend(run(&mut w, 12_500, 40_000, Some(&mic(CHROME)), false));
        assert_eq!(offered(&d)[0].0, 32_500);
        let mut w = Watcher::default();
        let mut d = run(&mut w, 0, 10_000, Some(&mic(CHROME)), false);
        d.extend(run(&mut w, 10_500, 11_500, Some(&[]), false)); // back at 12 s: 2 s apart
        d.extend(run(&mut w, 12_000, 40_000, Some(&mic(CHROME)), false));
        assert_eq!(offered(&d)[0].0, 20_000);
    }

    #[test]
    fn a_gap_over_10_s_between_polls_restarts_the_clock() {
        let mut w = Watcher::default();
        let mut d = run(&mut w, 0, 15_000, Some(&mic(CHROME)), false);
        d.extend(run(&mut w, 30_000, 60_000, Some(&mic(CHROME)), false)); // the Mac slept 15 s
        assert_eq!(offered(&d)[0].0, 50_000);
    }

    #[test]
    fn the_offer_goes_when_the_app_leaves_and_comes_once_per_run() {
        let mut w = Watcher::default();
        let mut d = run(&mut w, 0, 5_000, Some(&mic(SLACK)), false);
        assert_eq!(offered(&d).len(), 1);
        d = run(&mut w, 5_500, 9_000, Some(&[]), false);
        assert_eq!(d.iter().filter(|(_, d)| *d == Do::Withdraw).count(), 1);
    }

    #[test]
    fn dismissing_snoozes_the_app_until_it_has_been_off_the_mic_for_60_s() {
        let mut w = Watcher::default();
        run(&mut w, 0, 2_000, Some(&mic(SLACK)), false);
        w.dismiss();
        assert!(offered(&run(&mut w, 2_500, 30_000, Some(&mic(SLACK)), false)).is_empty());
        run(&mut w, 30_500, 60_000, Some(&[]), false); // off 30 s: still snoozed
        assert!(offered(&run(&mut w, 60_500, 70_000, Some(&mic(SLACK)), false)).is_empty());
        run(&mut w, 70_500, 131_000, Some(&[]), false); // off over 60 s
        assert_eq!(offered(&run(&mut w, 131_500, 140_000, Some(&mic(SLACK)), false)).len(), 1);
    }

    #[test]
    fn nothing_is_offered_while_recording_or_with_the_setting_off_and_a_start_withdraws_it() {
        let mut w = Watcher::default();
        assert!(offered(&run(&mut w, 0, 5_000, Some(&mic(SLACK)), true)).is_empty());
        let mut w = Watcher::default();
        let d: Vec<_> = (0..=5_000).step_by(500).flat_map(|t| w.poll(t, Some(&mic(SLACK)), false, false, true)).collect();
        assert!(d.is_empty());
        let mut w = Watcher::default();
        run(&mut w, 0, 2_000, Some(&mic(SLACK)), false);
        assert_eq!(w.poll(2_500, Some(&mic(SLACK)), true, true, true), [Do::Withdraw]);
    }

    /// Starts recording a Teams call at 0 (it's held the mic since -5 s).
    fn in_a_call() -> Watcher {
        let mut w = Watcher::default();
        run(&mut w, -5_000, -500, Some(&mic(TEAMS)), false);
        run(&mut w, 0, 0, Some(&mic(TEAMS)), true);
        w
    }

    #[test]
    fn the_call_app_leaving_for_3_s_starts_the_countdown_and_30_s_later_it_stops() {
        let mut w = in_a_call();
        let d = run(&mut w, 500, 40_000, Some(&[]), true);
        assert_eq!(d, [(3_000, Do::Countdown(33_000)), (33_000, Do::Stop)]);
    }

    #[test]
    fn the_app_coming_back_within_30_s_cancels_and_the_meeting_carries_on() {
        let mut w = in_a_call();
        let mut d = run(&mut w, 500, 10_000, Some(&[]), true);
        d.extend(run(&mut w, 10_500, 60_000, Some(&mic(TEAMS)), true));
        assert_eq!(d, [(3_000, Do::Countdown(33_000)), (10_500, Do::Cancel)]);
    }

    #[test]
    fn playing_sound_without_the_mic_is_still_in_the_call() {
        let mut w = in_a_call();
        assert!(run(&mut w, 500, 60_000, Some(&[app(TEAMS, false, true)]), true).is_empty()); // muted in Meet
    }

    #[test]
    fn a_failed_read_never_stops_a_recording() {
        let mut w = in_a_call();
        assert!(run(&mut w, 500, 120_000, None, true).is_empty());
        let mut w = in_a_call();
        let mut d = run(&mut w, 500, 5_000, Some(&[]), true);
        d.extend(run(&mut w, 5_500, 60_000, None, true));
        assert_eq!(d, [(3_000, Do::Countdown(33_000)), (5_500, Do::Cancel)]);
    }

    #[test]
    fn keep_recording_turns_the_end_watch_off_for_this_recording_only() {
        let mut w = in_a_call();
        run(&mut w, 500, 5_000, Some(&[]), true);
        w.keep();
        assert!(run(&mut w, 5_500, 120_000, Some(&[]), true).is_empty());
        run(&mut w, 120_500, 120_500, Some(&mic(TEAMS)), false); // stopped
        run(&mut w, 121_000, 200_000, Some(&mic(TEAMS)), false);
        run(&mut w, 200_500, 200_500, Some(&mic(TEAMS)), true); // the next recording watches again
        assert_eq!(run(&mut w, 201_000, 210_000, Some(&[]), true)[0], (203_500, Do::Countdown(233_500)));
    }

    #[test]
    fn a_manual_recording_with_no_call_app_on_the_mic_has_no_end_watch() {
        let mut w = Watcher::default();
        assert!(run(&mut w, 0, 120_000, Some(&[]), true).is_empty());
    }

    #[test]
    fn a_recording_watches_the_offered_app_and_stopping_it_cancels_the_countdown() {
        let mut w = Watcher::default();
        let both = [app(SLACK, true, false), app(TEAMS, true, false)];
        let d = run(&mut w, 0, 2_000, Some(&both), false);
        assert_eq!(offered(&d), [(1_000, "Microsoft Teams".into(), "Meeting detected")]); // one offer, never two
        assert_eq!(w.offered_name().as_deref(), Some("teams2"));
        w.poll(2_500, Some(&both), true, true, true);
        let d = run(&mut w, 3_000, 7_000, Some(&both[..1]), true); // Teams left, Slack stays
        assert_eq!(d, [(5_500, Do::Countdown(35_500))]);
        assert_eq!(w.poll(7_500, Some(&both[..1]), false, true, true)[0], Do::Cancel); // then Slack, still on the mic, is offered
    }

    #[test]
    fn turning_stop_when_the_call_ends_off_cancels_a_countdown() {
        let mut w = in_a_call();
        run(&mut w, 500, 5_000, Some(&[]), true);
        assert_eq!(w.poll(5_500, Some(&[]), true, true, false), [Do::Cancel]);
        assert!((6_000..=60_000).step_by(500).flat_map(|t| w.poll(t, Some(&[]), true, true, false)).next().is_none());
    }

    #[test]
    fn safari_is_known_by_its_webkit_audio_process() {
        assert_eq!(call_app("com.apple.WebKit.GPU"), Some(("Call in Safari", 20_000)));
    }
}
