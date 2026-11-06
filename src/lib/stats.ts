/** Stats for nerds' Overview: every number, computed from the stored traces (`traces_all`). Definitions: #105 and #73. */
import { KINDS, type Edit, type Trace } from "./trace";

export type Period = "today" | "7" | "30" | "all";
export const PERIODS: [Period, string][] = [["today", "Today"], ["7", "7 days"], ["30", "30 days"], ["all", "All"]];
export type Outcome = "pasted" | "nothing_heard" | "failed";
export const OUTCOMES: [Outcome, string, string][] = [["pasted", "Pasted", "--o-pasted"], ["nothing_heard", "Nothing heard", "--o-none"], ["failed", "Failed", "--o-failed"]];
export type Kind = keyof typeof KINDS;
export type Word = { spelling: string; heard: string | null };
export type Opts = { period: Period; model: string; app: string; now: number; words: Word[] };
/** A row in By model, By rule or By app, picked to filter the list. */
export type Pick = { kind: "model" | "rule" | "app"; v: string } | null;

/** Medians, p90s, rates and shares wait for this many dictations behind them; counts always show. */
export const NEEDS = 10;
/** A number and the count of dictations it's computed over. */
export type Cell<T = number> = { n: number; v: T };
export const shown = (c: { n: number }) => c.n >= NEEDS;
export const why = (n: number) => `${n} dictation${n === 1 ? "" : "s"}, needs ${NEEDS}`;

/** Nearest rank. */
export function quantile(xs: number[], q: number) {
  if (!xs.length) return 0;
  const a = [...xs].sort((x, y) => x - y);
  return a[Math.max(0, Math.ceil(q * a.length) - 1)];
}
const share = (part: number, n: number): Cell => ({ n, v: n ? part / n : 0 });

/** "Not pasted" counts as failed in the split; the list badge still says "Not pasted". */
export const outcome = (t: Trace): Outcome => (t.outcome === "pasted" || t.outcome === "nothing_heard" ? t.outcome : "failed");
const stt = (t: Trace) => t.steps.find((s) => s.kind === "speech-to-text");
const rules = (t: Trace) => t.steps.find((s) => s.kind === "rules");
/** Words speech to text heard, before cleanup. */
export const words = (t: Trace) => (stt(t)?.text ?? "").split(/\s+/).filter(Boolean).length;
/** An already-right word to remember isn't an edit. */
const isEdit = (e: Edit) => !(e.kind === "word" && e.unchanged);
export const edits = (t: Trace) => (rules(t)?.edits ?? []).filter(isEdit);
const cold = (t: Trace) => (t.load_ms ?? 0) >= 1;
/** Model load wait, speech to text, rules, and paste as what's left of let go → pasted (as the opened trace's strip). */
export function steps(t: Trace) {
  const parts = [t.load_ms ?? 0, stt(t)?.ms ?? 0, rules(t)?.ms ?? 0];
  const sum = parts.reduce((a, b) => a + b, 0);
  return [...parts, (t.paste_ms ?? sum) - sum];
}
export const STEPS: [string, string][] = [["Model load wait", "--t-load"], ["Speech to text", "--t-stt"], ["Rules", "--t-rules"], ["Paste", "--t-paste"]];

const midnight = (ms: number) => { const d = new Date(ms); d.setHours(0, 0, 0, 0); return d; };
/** Local calendar days back from today: 0 is today. */
const daysAgo = (ms: number, now: number) => Math.round((midnight(now).getTime() - midnight(ms).getTime()) / 864e5);
/** The first moment in the period: local midnight, today and the days before it. */
export function since(period: Period, now: number) {
  if (period === "all") return -Infinity;
  const d = midnight(now);
  d.setDate(d.getDate() - (period === "today" ? 0 : +period - 1));
  return d.getTime();
}

/** The traces in the period, model and app, newest first as given. */
export const select = (traces: Trace[], o: Omit<Opts, "words">) =>
  traces.filter((t) => t.started >= since(o.period, o.now) && (o.model === "all" || t.model === o.model) && (o.app === "all" || t.app === o.app));

export function picks(t: Trace, p: Pick) {
  if (!p) return true;
  if (p.kind === "model") return t.model === p.v;
  if (p.kind === "app") return t.app === p.v;
  return edits(t).some((e) => e.kind === p.v);
}

/** The numbers every breakdown row and the tiles share. */
function summary(ts: Trace[]) {
  const pasted = ts.filter((t) => outcome(t) === "pasted"), total = pasted.map((t) => t.paste_ms ?? 0);
  const split = Object.fromEntries(OUTCOMES.map(([k]) => [k, ts.filter((t) => outcome(t) === k).length])) as Record<Outcome, number>;
  return { n: ts.length, pasted, split, med: { n: pasted.length, v: quantile(total, 0.5) }, p90: { n: pasted.length, v: quantile(total, 0.9) } };
}

export function overview(traces: Trace[], o: Opts) {
  const ts = select(traces, o), s = summary(ts), heard = ts.reduce((a, t) => a + words(t), 0);
  const colds = ts.filter(cold), all = ts.flatMap(edits);

  // The chart: by hour today, else one column a day; All goes back to the first trace, at least 7 days.
  const hourly = o.period === "today";
  const days = o.period === "all" ? Math.max(7, ...ts.map((t) => daysAgo(t.started, o.now) + 1)) : +o.period;
  const keys = hourly ? Array.from({ length: 24 }, (_, h) => h) : Array.from({ length: days }, (_, i) => days - 1 - i);
  const label = (k: number) => {
    if (hourly) return `${String(k).padStart(2, "0")}:00`;
    if (k === 0) return "Today";
    const d = midnight(o.now); d.setDate(d.getDate() - k);
    return d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
  };
  const buckets = keys.map((k) => {
    const g = ts.filter((t) => (hourly ? new Date(t.started).getHours() : daysAgo(t.started, o.now)) === k);
    return { label: label(k), ...summary(g) };
  });
  const max = Math.max(0, ...buckets.map((b) => b.n));
  const top = max <= 4 ? 4 : Math.ceil(max / 4) * 4;

  // By model, most used first.
  const count = (key: (t: Trace) => string) => {
    const c = new Map<string, Trace[]>();
    for (const t of ts) c.set(key(t), [...(c.get(key(t)) ?? []), t]);
    return [...c].sort((a, b) => b[1].length - a[1].length);
  };
  const models = count((t) => t.model ?? "").map(([name, g]) => {
    const r = summary(g), audio = r.pasted.reduce((a, t) => a + t.audio_ms, 0), sttMs = r.pasted.reduce((a, t) => a + (stt(t)?.ms ?? 0), 0);
    return { name, ...r, speed: { n: r.pasted.length, v: sttMs ? audio / sttMs : 0 }, cold: share(g.filter(cold).length, g.length),
      nothing: share(r.split.nothing_heard, g.length), failed: share(r.split.failed, g.length), words: { n: g.length, v: quantile(g.map(words), 0.5) },
      steps: { n: r.pasted.length, v: STEPS.map((_, i) => quantile(r.pasted.map((t) => steps(t)[i]), 0.5)) } };
  });

  const rulesRows = (Object.keys(KINDS) as Kind[]).map((k) => {
    const e = all.filter((x) => x.kind === k).length;
    return { kind: k, edits: e, per100: { n: ts.length, v: heard ? (e / heard) * 100 : 0 }, touched: share(ts.filter((t) => picks(t, { kind: "rule", v: k })).length, ts.length) };
  });

  // Fillers by what was removed: "Um, " and ", uh" count as "um" and "uh".
  const fill = new Map<string, number>();
  for (const t of ts) {
    const raw = Array.from(stt(t)?.text ?? "");
    for (const e of edits(t)) if (e.kind === "filler") {
      const f = raw.slice(e.start, e.end).join("").toLowerCase().replace(/^[\s\p{P}]+|[\s\p{P}]+$/gu, "");
      if (f) fill.set(f, (fill.get(f) ?? 0) + 1);
    }
  }
  const fillers = [...fill].sort((a, b) => b[1] - a[1]).slice(0, 5);

  // Words to remember: only the ones on the list now.
  const wordEdits = ts.flatMap((t) => rules(t)?.edits ?? []).filter((e) => e.kind === "word");
  const wordRows = o.words.map((w) => {
    const hits = wordEdits.filter((e) => e.kind === "word" && e.spelling === w.spelling && (e.heard ?? null) === (w.heard ?? null));
    return { ...w, fixed: hits.filter(isEdit).length, right: hits.filter((e) => !isEdit(e)).length };
  });

  // By app: the top 8, then the rest as one row that can't be picked.
  const apps = count((t) => t.app ?? "");
  const appRow = (name: string | null, g: Trace[], others = 0) => {
    const r = summary(g);
    return { name, others, ...r, pastedShare: share(r.split.pasted, g.length), failedShare: share(r.split.failed, g.length) };
  };
  const appRows = apps.slice(0, 8).map(([a, g]) => appRow(a, g));
  if (apps.length > 8) appRows.push(appRow(null, apps.slice(8).flatMap(([, g]) => g), apps.length - 8));

  return {
    traces: ts, ...s, words: heard,
    cold: share(colds.length, ts.length), wait: { n: colds.length, v: quantile(colds.map((t) => t.load_ms ?? 0), 0.5) },
    per100: { n: ts.length, v: heard ? (all.length / heard) * 100 : 0 },
    chart: { hourly, buckets, top },
    models, rules: rulesRows, fillers,
    heardWords: wordRows.filter((w) => w.fixed + w.right), neverHeard: wordRows.filter((w) => !(w.fixed + w.right)),
    apps: appRows,
  };
}
export type Overview = ReturnType<typeof overview>;
