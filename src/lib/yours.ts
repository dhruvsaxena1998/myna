/** "Your dictation": personal numbers from the stored traces (`traces_all`), for the last 7 or 30 days. */
import { daysAgo, midnight, select } from "./stats";
import type { Trace } from "./trace";

/** Typing speed the time saved is measured against. */
export const TYPING_WPM = 40;
export const UNKNOWN_APP = "Unknown app";

/** Words in what was pasted: the last step's text. */
export const pastedWords = (t: Trace) => (t.steps.at(-1)?.text ?? "").split(/\s+/).filter(Boolean).length;

/** "12 min", "1 h 5 min", "under a minute". */
export function minutes(ms: number) {
  const m = Math.floor(ms / 6e4);
  if (m < 1) return "under a minute";
  return m < 60 ? `${m} min` : m % 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m / 60} h`;
}

export function yours(traces: Trace[], period: 7 | 30, now: number) {
  const pasted = (ts: Trace[]) => ts.filter((t) => t.outcome === "pasted");
  const ts = pasted(select(traces, { period: String(period) as "7" | "30", model: "all", app: "all", now }));
  const words = ts.reduce((a, t) => a + pastedWords(t), 0);
  // Typing time minus speaking time, summed, then floored at 0.
  const saved = Math.max(0, (words / TYPING_WPM) * 6e4 - ts.reduce((a, t) => a + t.audio_ms, 0));

  // Streak: back from today, or from yesterday while today has none. Not limited by the period.
  const days = new Set(pasted(traces).map((t) => daysAgo(t.started, now)));
  let streak = 0;
  for (let d = days.has(0) ? 0 : 1; days.has(d); d++) streak++;

  const buckets = Array.from({ length: period }, (_, i) => {
    const k = period - 1 - i, d = midnight(now);
    d.setDate(d.getDate() - k);
    return { label: d.toLocaleDateString("en-GB", { day: "numeric", month: "short" }), words: ts.filter((t) => daysAgo(t.started, now) === k).reduce((a, t) => a + pastedWords(t), 0) };
  });
  const max = Math.max(0, ...buckets.map((b) => b.words));
  const top = max <= 4 ? 4 : Math.ceil(max / 4) * 4;

  const byApp = new Map<string, number>();
  for (const t of ts) byApp.set(t.app ?? UNKNOWN_APP, (byApp.get(t.app ?? UNKNOWN_APP) ?? 0) + pastedWords(t));
  const apps = [...byApp].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([name, w]) => ({ name, words: w }));

  return { n: ts.length, words, saved, streak, buckets, top, apps };
}
export type Yours = ReturnType<typeof yours>;
