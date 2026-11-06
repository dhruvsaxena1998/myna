/** What a stored dictation trace looks like to Stats for nerds (see src-tauri/src/trace.rs and rules.rs `Edit`). */
export type Edit =
  | { kind: "filler" | "repeat" | "cutoff" | "number"; start: number; end: number; to: string }
  | { kind: "word"; start: number; end: number; to: string; matched: "spelling" | "heard"; spelling: string; heard: string | null; unchanged: boolean };
/** The cleanup step's settings: the Fast model (none if it wasn't set), what the call cost, and why the rules text was pasted instead. */
export type Cleanup = { model: string | null; cost: { kind: "usd"; usd: number } | { kind: "free" | "unknown" } | null; fallback: string | null; detail: string | null };
export type Step = { kind: "speech-to-text" | "rules" | "cleanup"; text: string; ms: number; settings: { language: string; tidy: boolean } | Cleanup | null; edits: Edit[] | null };
export type Trace = {
  id: number; started: number; mode: "hold" | "tap"; app: string | null; model: string | null; audio_ms: number;
  outcome: string; reason: string | null; load_ms: number | null; paste_ms: number | null; steps: Step[];
};
/** `heard`: what speech-to-text wrote. `text`: what the last step left, which is what was pasted. */
export type Row = { id: number; started: number; app: string | null; outcome: string; heard: string | null; text: string | null };

export const KINDS = { filler: "Filler words", repeat: "Repeats", cutoff: "Cut-off words", number: "Numbers", word: "Words to remember" } as const;
export const KIND_ONE = { filler: "Filler word", repeat: "Repeat", cutoff: "Cut-off word", number: "Number", word: "Word to remember" } as const;

/** Edits nest ("twenty uh five": a filler inside a number). Only the outermost shows; of two identical spans, the earlier one. */
export function outermost(edits: Edit[]): Edit[] {
  const inside = (e: Edit, o: Edit) => o.start <= e.start && o.end >= e.end;
  return edits
    .filter((e, i) => !edits.some((o, j) => j !== i && inside(e, o) && (o.end - o.start > e.end - e.start || (j < i && o.start === e.start && o.end === e.end))))
    .sort((a, b) => a.start - b.start);
}

/** Spans count characters (Rust `char`s), not UTF-16 units. */
export function rebuild(raw: string, edits: Edit[]): string {
  const c = Array.from(raw);
  let out = "", at = 0;
  for (const e of outermost(edits)) {
    out += c.slice(at, e.start).join("") + e.to;
    at = e.end;
  }
  return out + c.slice(at).join("");
}

export const OUTCOME: Record<string, string> = { pasted: "Pasted", nothing_heard: "Didn’t hear anything", failed: "Failed", not_pasted: "Not pasted" };

/** The first words of what was heard: a trace's title. */
export function title(heard: string | null) {
  const w = (heard ?? "").trim().split(/\s+/).filter(Boolean);
  return w.length ? w.slice(0, 6).join(" ") + (w.length > 6 ? "…" : "") : "Nothing heard";
}

export function when(ms: number, now = new Date()) {
  const d = new Date(ms), day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const hm = d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  const days = Math.round((day(now) - day(d)) / 864e5);
  return `${days === 0 ? "Today" : days === 1 ? "Yesterday" : d.toLocaleDateString("en-GB", { day: "numeric", month: "short" })} ${hm}`;
}

export const dur = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(2)} s` : `${Math.round(n)} ms`);
