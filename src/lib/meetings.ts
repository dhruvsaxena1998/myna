/** A meeting as `meetings_list` returns it. `state`: recorded and transcribing are still being written up. */
export type Meeting = {
  id: number;
  title: string;
  started: number;
  duration_ms: number | null;
  source_app: string | null;
  state: "recorded" | "transcribing" | "done" | "failed";
  failure: string | null;
  /** Worth knowing about a finished meeting, like why voices weren't told apart. */
  note: string | null;
  /** Failed with an Endpoint's chunks undone: Transcribe locally is offered beside Try again. */
  incomplete: boolean;
};
/** `hidden`: said while muted, or `echo` (the call heard back through the speakers), and left out; its `text` is empty. `included`: kept by the user, shown with a marker. */
export type Line = { id: number; speaker: string; /* "Me", a caption name, "Speaker 2"…, or "Them" when voices weren't told apart */ start_ms: number; end_ms: number; text: string; /* as transcript cleanup wrote it, when it did */ clean: string | null; muted: boolean; included: boolean; echo?: boolean; hidden: boolean };
/** `cleanup`: the last transcript cleanup run, none when it never ran. `stt`: the Endpoint that transcribed it (no cost while it runs), none for the Local model. */
export type Page = Meeting & { segments: Line[]; cleanup: { provider: string; model: string; cost: Cost } | null; stt: { provider: string; model: string; off: boolean; cost?: Cost } | null };
/** The recording in progress, or none. */
/** `mic`: "paused" when paused, "live" or "muted" while the browser add-on mirrors the mute button, else null. `words`: how the mic reads on the card. */
export type Running = { id: number; started: number; source: string | null; mic: "live" | "muted" | "paused" | null; words: string } | null;

/** A pause this long or more between one speaker's lines starts a new paragraph; so does a paragraph this long. */
const PAUSE = 2000, LONGEST = 60000;

/**
 * The transcript's rows: runs of lines that are left out for one reason (muted, or echo) show as one row, and one speaker's lines with only short
 * pauses between them read as one paragraph. An included line ("Said while muted") never joins ordinary ones.
 */
export function groups(lines: Line[]): ({ lines: Line[] } | { left: Line[] })[] {
  const out: ({ lines: Line[] } | { left: Line[] })[] = [];
  for (const line of lines) {
    const last = out.at(-1);
    if (line.hidden) {
      if (last && "left" in last && !!last.left[0].echo === !!line.echo) last.left.push(line);
      else out.push({ left: [line] });
      continue;
    }
    const prev = last && "lines" in last ? last.lines.at(-1)! : null;
    const joins = prev && last && "lines" in last && prev.speaker === line.speaker && prev.included === line.included &&
      line.start_ms - prev.end_ms < PAUSE && line.start_ms - last.lines[0].start_ms < LONGEST;
    if (joins) last.lines.push(line);
    else out.push({ lines: [line] });
  }
  return out;
}

/** A speaker's `n` longest paragraphs, longest first: the clips to hear while naming them. Left-out lines are never offered. */
export function clips(lines: Line[], speaker: string, n = 3): { start_ms: number; end_ms: number }[] {
  return groups(lines)
    .flatMap((g) => ("lines" in g && g.lines[0].speaker === speaker ? [{ start_ms: g.lines[0].start_ms, end_ms: g.lines.at(-1)!.end_ms }] : []))
    .sort((a, b) => b.end_ms - b.start_ms - (a.end_ms - a.start_ms) || a.start_ms - b.start_ms)
    .slice(0, n);
}

/**
 * The cleaned line in runs of words: the ones that differ from what Myna heard carry `heard`, the words they took the
 * place of ("" for a word cleanup added). Words cleanup only took out (filler) have nothing to mark and don't show.
 */
export function changes(heard: string, clean: string): { text: string; heard?: string }[] {
  const a = heard.split(/\s+/).filter(Boolean), b = clean.split(/\s+/).filter(Boolean);
  // lcs[i][j]: how many words a[i..] and b[j..] have in common, in order.
  const lcs = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--) for (let j = b.length - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
  const out: { text: string; heard?: string }[] = [];
  let [i, j, same, added, taken] = [0, 0, [] as string[], [] as string[], [] as string[]];
  const flush = () => {
    if (added.length) {
      if (same.length) out.push({ text: same.join(" ") });
      out.push({ text: added.join(" "), heard: taken.join(" ") });
      same = [];
    }
    (added = []), (taken = []);
  };
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) flush(), same.push(b[j]), i++, j++;
    else if (j < b.length && (i === a.length || lcs[i][j + 1] >= lcs[i + 1][j])) added.push(b[j++]);
    else taken.push(a[i++]);
  }
  flush();
  if (same.length) out.push({ text: same.join(" ") });
  return out;
}

/** `meeting-notes`: how writing up a meeting is going. */
export type Progress = { id: number; state: "transcribing" | "done" | "failed"; progress: number; error: string | null };

export const writing = (m: Meeting) => m.state === "recorded" || m.state === "transcribing";

const WEEKDAY = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** The day heading and the date beside it, like the design: Today / Yesterday / weekday, else just the date. */
export function dayHead(ms: number, now = new Date()): [string, string] {
  const d = new Date(ms);
  const days = Math.round((day(now) - day(d)) / 864e5);
  const date = d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
  if (days === 0 || days === 1) return [days ? "Yesterday" : "Today", `${d.toLocaleDateString("en-GB", { weekday: "short" })} ${date}`];
  return days < 7 ? [WEEKDAY[d.getDay()], date] : [`${date} ${d.getFullYear()}`, ""];
}

export const hm = (ms: number) => new Date(ms).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

/** "mm:ss". */
export function clock(ms: number) {
  const s = Math.floor(Math.max(0, ms) / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

export function length(ms: number) {
  const m = Math.max(1, Math.round(ms / 60000));
  return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h${m % 60 ? ` ${m % 60} min` : ""}`;
}

/** "11:00 · 42 min · Google Meet", leaving out what isn't known. */
export const meta = (m: Meeting) => [hm(m.started), m.duration_ms === null ? "" : length(m.duration_ms), m.source_app ?? ""].filter(Boolean).join(" · ");

export type Cost = { kind: "usd"; usd: number } | { kind: "free" } | { kind: "unknown" };
/** A kept Summary version (`summary_get`). */
export type Version = { id: number; template: string; prompt: string; provider: string; model: string; created: number; parts: number; cost: Cost; text: string; edited: boolean };
/** `meeting-summary`: how a Summary run is going. `writing`: the Summary so far, as the model writes it. */
export type SummaryRun = { id: number } & ({ state: "reading"; n: number; of: number } | { state: "merging" | "skipped" } | { state: "writing"; text: string } | { state: "done"; version: Version } | { state: "failed"; error: string });
/** `meeting-cleanup`: how transcript cleanup is going. */
export type CleanupRun = { id: number } & ({ state: "reading"; n: number; of: number } | { state: "done"; cost: Cost } | { state: "failed"; error: string });
