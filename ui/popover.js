// Menubar popover: its own window, opened by a tray click. The backend decides what to show
// (`popover_view`); this renders it. Uses invoke, app, icon, esc, fmtKey and ICONS from the inline script in index.html.
// Wrapped so its helpers stay out of the page's global scope; only `popover` is exposed.
const popover = (() => {
const clock = (s) => {
  s = Math.max(0, Math.floor(s));
  const p = (n) => String(n).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}` : `${p(Math.floor(s / 60))}:${p(s % 60)}`;
};
const elapsed = (started) => clock(Date.now() / 1000 - started);
const pct = (progress) => Math.round(progress * 100);
const when = (m) => {
  if (m.status === "transcribing") return "Transcribing";
  if (m.status === "failed") return "Failed";
  const d = new Date(m.started_at * 1000);
  return d.toDateString() === new Date().toDateString()
    ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false })
    : d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
};

// tabindex: WebKit leaves buttons out of the Tab order unless Full Keyboard Access is on; an explicit one keeps them in.
const row = (act, ic, label, k = "", attrs = "") =>
  `<button class="mi" tabindex="0" data-act="${act}"${attrs}>${ic ? icon(ic) : ""}<span class="t">${label}</span>${k ? `<span class="k">${k}</span>` : ""}</button>`;
const section = (...parts) => (parts.some(Boolean) ? `<div class="sec">${parts.filter(Boolean).join("")}</div>` : "");

// Always text plus a dot, never colour alone. When the mic is off, the way back and what that means sit under it.
function micHtml(m) {
  const mute = (cls, label) => `<button class="btn ${cls}" tabindex="0" data-act="mute">${label}</button>`;
  if (!m) return "";
  const state = `<span class="grow">Mic ${m.live ? "live" : "off"} · <b>${esc(m.how)}</b></span>`;
  if (m.live) return `<div class="line"><span class="dot live"></span>${state}${mute("quiet", "Mute")}</div>`;
  return `<div class="line"><span class="dot"></span>${state}</div><div class="line">${mute("", icon("mic", "sm") + " Unmute in Myna")}<span class="xs grow">Saved locally, kept out of the transcript.</span></div>`;
}

function stateHtml(s, mic) {
  if (s.kind === "recording") {
    return `<div class="sec"><div class="state"><div><div class="lbl">Recording</div><div class="big" data-live="elapsed">${elapsed(s.started_at)}</div></div></div>
${micHtml(mic)}
<div class="line"><span class="dot"></span><span class="grow">${esc(s.app ? "System audio · " + s.app : s.note || "No system audio")}</span></div>
${s.shots ? `<div class="line"><span class="dot"></span><span class="grow">${esc((s.shots.video ? "Window video · " : "Screenshots · ") + s.shots.text)}</span></div>` : ""}</div>`;
  }
  if (s.kind === "transcribing") {
    const n = pct(s.progress);
    return `<div class="sec"><div class="state"><div><div class="lbl">Transcribing</div><div class="big" data-live="pct">${n}%</div></div><div class="bar" role="progressbar" aria-label="Transcribing" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${n}"><i style="width:${n}%"></i></div><div class="xs">${esc(s.title)}</div></div></div>`;
  }
  return "";
}

// Which window the screenshots or video follow: the app using the mic, unless picked here. `windows` is the open list, or null.
function pickerHtml(shots, windows) {
  const open = windows !== null;
  const items = [{ id: null, label: "Automatic · the app using the mic" }, ...(windows || []).map((w) => ({ id: w.id, label: w.title ? `${w.app} · ${w.title}` : w.app }))];
  const list = items.map((w) => `<button class="mi" tabindex="0" role="menuitemradio" aria-checked="${w.id === shots.chosen}" data-act="pick"${w.id === null ? "" : ` data-id="${w.id}"`}><span class="t">${esc(w.label)}</span>${w.id === shots.chosen ? icon("check") : ""}</button>`);
  return row("windows", "cam", shots.video ? "Video window" : "Screenshot window", shots.chosen === null ? "automatic" : "picked", ` aria-expanded="${open}"`) + (open ? `<div class="picklist" role="menu" aria-label="${shots.video ? "Video window" : "Screenshot window"}">${list.join("")}</div>` : "");
}

function viewHtml(v, error, windows) {
  const s = v.state;
  // With Meetings off there is nothing to start, so offer to switch it on here rather than hide recording entirely.
  const toggle = s.kind === "recording" ? row("toggle", "stop", "Stop and transcribe") : v.can_start ? row("toggle", "rec", "Start meeting recording") : !v.meetings ? row("meetings-on", "rec", "Turn on Meetings") : "";
  const picker = s.kind === "recording" && s.shots ? pickerHtml(s.shots, windows) : "";
  const recent = v.recent.length
    ? `<div class="sec" role="group" aria-label="Recent meetings"><div class="hd" aria-hidden="true">Recent</div>${v.recent
        .map((m) => `<button class="mi" tabindex="0" data-act="meeting" data-id="${m.id}"><span class="t">${esc(m.title)}</span><span class="k">${esc(when(m))}</span></button>`)
        .join("")}</div>`
    : "";
  const d = v.dictation;
  const dictation = d && `<div class="mi${d.error ? " err" : ""}">${icon("mic")}<span class="t">Dictation</span><span class="k">${d.error ? "hotkey unavailable" : (d.toggle ? "press " : "hold ") + esc(fmtKey(d.hotkey))}</span></div>`;
  return `<div class="popover" role="group" aria-label="Myna">${error ? `<div class="banner" role="alert">${esc(error)}</div>` : ""}${stateHtml(s, v.mic)}${section(toggle, picker)}${recent}${section(dictation, v.meetings && row("meetings", "list", "Meetings…"), row("settings", "gear", "Settings…"))}${section(row("quit", "", "Quit Myna"))}</div>`;
}

async function popover() {
  Object.assign(ICONS, {
    list: '<path d="M2.5 4h11M2.5 8h11M2.5 12h7"/>',
    rec: '<circle cx="8" cy="8" r="6"/><circle cx="8" cy="8" r="2.5" fill="currentColor" stroke="none"/>',
    stop: '<rect x="3.5" y="3.5" width="9" height="9" rx="1.5"/>',
    cam: '<rect x="1.5" y="4.5" width="9" height="7" rx="1.5"/><path d="M10.5 7l4-2v6l-4-2"/>',
  });
  document.body.className = "set pop";
  app.innerHTML = '<div class="shell"></div>';
  const shell = app.firstChild;
  let shown = "";
  let error = "";
  let busy = false;
  let windows = null; // the screenshot window list while the picker is open

  async function refresh() {
    const v = await invoke("popover_view");
    // Ticking numbers update in place, so the page (and keyboard focus in it) is not rebuilt every second.
    const sig = JSON.stringify([v, error, windows], (k, x) => (k === "progress" ? 0 : x));
    if (sig !== shown) {
      shown = sig;
      const focused = document.activeElement?.closest("[data-act]");
      const key = focused && `[data-act="${focused.dataset.act}"]${focused.dataset.id ? `[data-id="${focused.dataset.id}"]` : ""}`;
      shell.innerHTML = viewHtml(v, error, windows);
      if (key) shell.querySelector(key)?.focus();
      // The first fit after a tray click shows the window; later ones follow the content's height.
      invoke("popover_fit", { height: shell.offsetHeight });
    }
    const s = v.state;
    const live = shell.querySelector("[data-live]");
    if (s.kind === "recording" && live) live.textContent = elapsed(s.started_at);
    if (s.kind === "transcribing" && live) {
      live.textContent = pct(s.progress) + "%";
      shell.querySelector(".bar i").style.width = pct(s.progress) + "%";
    }
  }

  app.onclick = async (e) => {
    const b = e.target.closest("[data-act]");
    if (!b || busy) return;
    const act = b.dataset.act;
    if (act === "toggle") {
      busy = true;
      error = "";
      windows = null;
      // Starting and stopping wait on audio hardware, so the control stays put until it is done.
      try { await invoke("meeting_toggle"); } catch (err) { error = String(err); }
      busy = false;
      refresh();
    } else if (act === "windows") {
      error = "";
      // Closing is just forgetting the list; opening asks ScreenCaptureKit, which can say no.
      try { windows = windows ? null : await invoke("screenshot_windows"); } catch (err) { error = String(err); }
      refresh();
    } else if (act === "pick") {
      windows = null;
      await invoke("screenshot_pick", { id: b.dataset.id ? Number(b.dataset.id) : null });
      refresh();
    } else if (act === "mute") {
      try { await invoke("meeting_mute"); } catch (err) { error = String(err); }
      refresh();
    } else if (act === "meetings-on") {
      await invoke("set_setting", { key: "meetings_enabled", value: "true" });
      refresh();
    } else if (act === "meeting") {
      invoke("popover_go", { to: "meetings", id: Number(b.dataset.id) });
    } else {
      invoke("popover_go", { to: act });
    }
  };
  // Esc closes; the arrow keys move through the items, as in a native menu.
  document.onkeydown = (e) => {
    if (e.key === "Escape") invoke("popover_hide");
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = [...shell.querySelectorAll("button")];
    const i = items.indexOf(document.activeElement);
    const down = e.key === "ArrowDown";
    items[i < 0 ? (down ? 0 : items.length - 1) : (i + (down ? 1 : items.length - 1)) % items.length]?.focus();
  };
  window.__TAURI__.event.listen("popover-open", () => {
    error = "";
    windows = null;
    shown = "";
    refresh();
  });
  setInterval(() => { if (document.hasFocus()) refresh(); }, 1000);
}

return popover;
})();
