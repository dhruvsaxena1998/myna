// Same matching as the mockup (docs/index.html `fuzzy`): every typed word is in the text, or nearly matches one of its words.
// shortcut: per-word substring, subsequence or one-typo match; fine for short lists of names, not for long text.
const lev = (a: string, b: string) => {
  const d = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    let p = d[0];
    d[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const t = d[j];
      d[j] = Math.min(d[j] + 1, d[j - 1] + 1, p + (a[i - 1] === b[j - 1] ? 0 : 1));
      p = t;
    }
  }
  return d[b.length];
};
const subseq = (t: string, w: string) => { let i = 0; for (const c of w) if (c === t[i]) i++; return i === t.length; };
const near = (t: string, w: string) => t.length >= 3 && (lev(t, w.slice(0, t.length)) <= 1 || (subseq(t, w) && w.length - t.length <= 2));

export function fuzzy(q: string, text: string) {
  const hay = text.toLowerCase(), words = hay.match(/[\p{L}\p{N}:]+/gu) || [];
  return q.toLowerCase().split(/\s+/).filter(Boolean).every((t) => hay.includes(t) || words.some((w) => near(t, w)));
}
