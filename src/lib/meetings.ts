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
};
/** `hidden`: said while muted and left out; its `text` is empty. `included`: kept by the user, shown with a marker. */
export type Line = { id: number; speaker: string; /* "Me", a caption name, "Speaker 2"…, or "Them" when voices weren't told apart */ start_ms: number; text: string; muted: boolean; included: boolean; hidden: boolean };
export type Page = Meeting & { segments: Line[] };
/** The recording in progress, or none. */
/** `mic`: "live" or "muted" while the browser add-on mirrors the mute button, else null. `words`: how the mic reads on the card. */
export type Running = { id: number; started: number; source: string | null; mic: "live" | "muted" | null; words: string } | null;

/** Runs of lines that are left out show as one row; the rest show as they are. */
export function groups(lines: Line[]): ({ line: Line } | { left: Line[] })[] {
  const out: ({ line: Line } | { left: Line[] })[] = [];
  for (const line of lines) {
    const last = out.at(-1);
    if (!line.hidden) out.push({ line });
    else if (last && "left" in last) last.left.push(line);
    else out.push({ left: [line] });
  }
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
