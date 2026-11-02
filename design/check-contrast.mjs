// Checks tokens.css: WCAG contrast for every text/surface pair we promise, and that the
// dark and auto-dark blocks hold identical values. Run: node design/check-contrast.mjs
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('./tokens.css', import.meta.url), 'utf8');
const vars = block => Object.fromEntries([...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map(m => [m[1], m[2].trim()]));
const section = (start, end) => css.slice(css.indexOf(start), css.indexOf(end, css.indexOf(start)));

const light = vars(section('/* ---------- 2.', '/* ---------- 3.'));
const darkBlock = section(':root[data-theme="dark"] {', '}');
const autoBlock = section(':root[data-theme="auto"] {', '}');
const dark = { ...light, ...vars(darkBlock) };
const accents = Object.fromEntries([...css.matchAll(/data-accent="(\w+)"\]\s*\{([^}]+)\}/g)].map(m => [m[1], vars(m[2])]));

const parse = c => {
  let m = c.match(/^#([0-9a-f]{6})$/i);
  if (m) return [0, 2, 4].map(i => parseInt(m[1].slice(i, i + 2), 16)).concat(1);
  m = c.match(/^rgba\(([^)]+)\)$/);
  if (m) return m[1].split(',').map(Number);
  throw new Error(`cannot parse colour ${c}`);
};
const over = (fg, bg) => fg.slice(0, 3).map((v, i) => v * fg[3] + bg[i] * (1 - fg[3])).concat(1);
const lum = rgb => rgb.slice(0, 3).map(v => (v /= 255) <= .03928 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4)
  .reduce((s, v, i) => s + v * [.2126, .7152, .0722][i], 0);
const ratio = (fg, bg) => { const [a, b] = [lum(over(fg, bg)), lum(bg)].sort((x, y) => y - x); return (a + .05) / (b + .05); };

let fails = 0;
const check = (theme, t, fgName, bgName, min, fgVal) => {
  const bg = parse(t[bgName]);
  const r = ratio(parse(fgVal ?? t[fgName]), bg);
  if (r < min) { fails++; console.log(`FAIL ${theme}: ${fgName} on ${bgName} = ${r.toFixed(2)} (needs ${min})`); }
};
const surfaces = ['bg-content', 'bg-inset', 'bg-sidebar', 'bg-raised'];

for (const [theme, t] of [['light', light], ['dark', dark]]) {
  for (const s of surfaces) {
    for (const fg of ['text', 'text-2', 'text-3', 'danger-text', 'caution-text', 'ok-text']) check(theme, t, fg, s, 4.5);
    for (const fg of ['rec', 'ok', 'caution', 'line-strong']) check(theme, t, fg, s, fg === 'line-strong' ? 1.3 : 2.5); // non-text marks
  }
  for (const [name, a] of Object.entries(accents)) {
    const at = theme === 'light' ? a['accent-text-l'] : a['accent-text-d'];
    for (const s of surfaces) check(theme, t, `accent-text(${name})`, s, 4.5, at);
    if (ratio(parse(a['on-accent']), parse(a.accent)) < 4.5) { fails++; console.log(`FAIL on-accent on accent(${name})`); }
  }
  check(theme, { ...t, 'hud-solid': t['hud-bg-solid'] }, 'hud-fg', 'hud-solid', 4.5);
  check(theme, { ...t, 'hud-solid': t['hud-bg-solid'] }, 'hud-2', 'hud-solid', 4.5);
}
for (const [theme, t] of [['light', light], ['dark', dark]])
  if (ratio(parse(t['on-solid']), parse(t.danger)) < 4.5) { fails++; console.log(`FAIL ${theme}: on-solid on danger`); }

if (JSON.stringify(vars(darkBlock)) !== JSON.stringify(vars(autoBlock))) { fails++; console.log('FAIL dark and auto blocks differ'); }

console.log(fails ? `${fails} failure(s)` : 'all contrast checks pass');
process.exit(fails ? 1 : 0);
