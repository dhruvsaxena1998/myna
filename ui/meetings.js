// Meetings window: list of recordings and a transcript with the time in the margin.
// Uses invoke, app, icon, esc and ICONS from the inline script in index.html.
// Wrapped so its helpers stay out of the page's global scope; only `meetings` is exposed.
const meetings = (() => {
const clock = (s) => {
  s = Math.max(0, Math.floor(s));
  const p = (n) => String(n).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}` : `${p(Math.floor(s / 60))}:${p(s % 60)}`;
};
const mins = (ms) => (ms < 60000 ? "<1 min" : `${Math.round(ms / 60000)} min`);
const hhmm = (d) => d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
const dayLabel = (d) => {
  const days = Math.round((new Date().setHours(0, 0, 0, 0) - new Date(d).setHours(0, 0, 0, 0)) / 864e5);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  return d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
};
const elapsed = (m) => clock(Date.now() / 1000 - m.started_at);
const pct = (m) => Math.round(m.progress * 100) + "%";

function statusHtml(m) {
  if (m.status === "recording") return `<span class="pill alert"><span class="dot live"></span> Recording <span data-live="elapsed">${elapsed(m)}</span></span>`;
  if (m.status === "transcribing") return `transcribing <span data-live="pct">${pct(m)}</span>`;
  if (m.status === "failed") return '<span class="pill alert">Transcription failed</span>';
  return m.segments ? `${m.segments} segment${m.segments === 1 ? "" : "s"}` : "no speech";
}

async function meetings() {
  Object.assign(ICONS, {
    back: '<path d="M10 3 5 8l5 5"/>',
    list: '<path d="M2.5 4h11M2.5 8h11M2.5 12h7"/>',
    rec: '<circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="2" fill="currentColor"/>',
    stop: '<rect x="4" y="4" width="8" height="8" rx="1.5"/>',
  });
  document.body.className = "set meet";
  app.innerHTML = `<nav class="sidebar" aria-label="Meetings"><button class="nav" aria-current="page" id="nav-list">${icon("list")}Meetings<span class="k" id="count"></span></button></nav><div class="content" id="main"></div>`;
  const main = document.getElementById("main");
  let open = null; // id of the meeting shown in detail, or null for the list
  let shown = "";
  let timer;
  let error = "";

  document.getElementById("nav-list").onclick = () => { open = null; refresh(); };

  async function refresh() {
    clearTimeout(timer);
    if (!main.isConnected) return;
    const list = await invoke("list_meetings");
    document.getElementById("count").textContent = list.length || "";
    const detail = open === null ? null : await invoke("get_meeting", { id: open }).catch(() => null);
    if (open !== null && !detail) open = null;
    const sig = JSON.stringify([open, error, ...(detail ? [detail.meeting.status, detail.meeting.error, detail.segments.length] : list.map((m) => [m.id, m.status, m.segments, m.app, m.has_them, m.error]))]);
    if (sig !== shown) {
      shown = sig;
      detail ? renderDetail(detail) : renderList(list);
    }
    // Numbers that tick (elapsed, percent) update in place so the page does not rebuild every second.
    const live = detail ? [detail.meeting] : list;
    for (const m of live) {
      main.querySelectorAll(`[data-live][data-id="${m.id}"], .item[data-id="${m.id}"] [data-live]`).forEach((el) => {
        el.textContent = el.dataset.live === "elapsed" ? elapsed(m) : pct(m);
      });
      const bar = main.querySelector(`[data-bar="${m.id}"]`);
      if (bar) bar.style.width = pct(m);
    }
    const busy = live.some((m) => m.status === "recording" || m.status === "transcribing");
    timer = setTimeout(refresh, busy ? 1000 : 3000);
  }

  async function toggle() {
    error = "";
    try { await invoke("meeting_toggle"); } catch (e) { error = String(e); }
    shown = "";
    refresh();
  }

  function renderList(list) {
    const recording = list.some((m) => m.status === "recording");
    main.innerHTML = `<div class="toolbar"><h1>Meetings</h1><span class="sp"></span><button class="btn primary" id="rec">${icon(recording ? "stop" : "rec", "sm")} ${recording ? "Stop and transcribe" : "Record"}</button></div>${error ? `<div class="banner" role="alert">${esc(error)}</div>` : ""}<div class="list" id="list"></div>`;
    document.getElementById("rec").onclick = toggle;
    const box = document.getElementById("list");
    if (!list.length) {
      box.innerHTML = '<div class="empty"><h2>No meetings yet</h2><p>Join your call, then press Record or choose “Start meeting recording” in the menubar.</p></div>';
      box.style.display = "flex";
      return;
    }
    let last = "";
    for (const m of list) {
      const d = new Date(m.started_at * 1000);
      const label = dayLabel(d);
      if (label !== last) { last = label; box.insertAdjacentHTML("beforeend", `<h2 class="day">${esc(label)}</h2>`); }
      const item = document.createElement("button");
      item.className = "item";
      item.dataset.id = m.id;
      item.innerHTML = `<div class="t">${esc(m.title)}</div><div class="when">${hhmm(d)}${m.duration_ms ? " · " + mins(m.duration_ms) : ""}</div><div class="meta"><span class="av" aria-hidden="true"><i>M</i>${m.has_them ? "<i>T</i>" : ""}</span><span>${esc(m.app || "Mic only")}</span>·<span>${statusHtml(m)}</span></div>`;
      item.onclick = () => { open = m.id; error = ""; shown = ""; refresh(); };
      box.append(item);
    }
  }

  function renderDetail({ meeting: m, segments }) {
    const d = new Date(m.started_at * 1000);
    const when = `${d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} · ${hhmm(d)}${m.duration_ms ? " · " + mins(m.duration_ms) : ""}`;
    main.innerHTML = `<div class="toolbar"><button class="btn quiet icon" id="back" aria-label="Back to Meetings">${icon("back")}</button><h1>Meetings</h1><span class="sp"></span><span class="s" id="saved" role="status"></span><div class="menu-wrap"><button class="btn" id="export" aria-haspopup="menu" aria-expanded="false"${m.status === "done" ? "" : " disabled"}>Export</button><div class="menu" role="menu" id="menu" hidden><button class="mi" role="menuitem" data-fmt="md">Markdown (.md)</button><button class="mi" role="menuitem" data-fmt="txt">Plain text (.txt)</button></div></div></div><div class="detail"><div class="doc"><div class="tx"><h2>${esc(m.title)}</h2><div class="sub"><span>${esc(when)}</span><span>${esc(m.app || "Mic only")}</span><span class="pill neutral">Parakeet v3</span></div>${m.note ? `<p class="note">${esc(m.note)}</p>` : ""}<div id="body"></div></div></div><div class="inspector" id="insp"></div></div>`;
    ai.summary(document.getElementById("insp"), m, m.status === "done" && segments.length > 0);
    document.getElementById("back").onclick = () => { open = null; shown = ""; refresh(); };
    const menu = document.getElementById("menu");
    const btn = document.getElementById("export");
    const close = () => { menu.hidden = true; btn.setAttribute("aria-expanded", "false"); };
    btn.onclick = (e) => {
      e.stopPropagation();
      menu.hidden = !menu.hidden;
      btn.setAttribute("aria-expanded", String(!menu.hidden));
      if (!menu.hidden) menu.querySelector(".mi").focus();
    };
    menu.onkeydown = (e) => { if (e.key === "Escape") { close(); btn.focus(); } };
    document.addEventListener("click", close, { once: true });
    menu.querySelectorAll(".mi").forEach((b) => {
      b.onclick = async () => {
        close();
        const saved = document.getElementById("saved");
        try { await invoke("export_meeting", { id: m.id, format: b.dataset.fmt }); saved.textContent = "Saved to Downloads"; }
        catch (e) { saved.textContent = "Export failed: " + e; }
      };
    });

    const body = document.getElementById("body");
    if (m.status === "recording") {
      body.innerHTML = `<div class="state"><p><span class="pill alert"><span class="dot live"></span> Recording <span data-live="elapsed" data-id="${m.id}">${elapsed(m)}</span></span></p><p>The transcript appears after you stop.</p></div>`;
    } else if (m.status === "transcribing") {
      body.innerHTML = `<div class="state"><p>Transcribing <span data-live="pct" data-id="${m.id}">${pct(m)}</span></p><div class="bar" role="progressbar" aria-label="Transcribing" aria-valuemin="0" aria-valuemax="100"><i data-bar="${m.id}" style="width:${pct(m)}"></i></div><p class="s">You can close this window; it keeps going.</p></div>`;
    } else if (m.status === "failed") {
      body.innerHTML = `<div class="state"><p class="err">${esc(m.error || "Transcription failed.")}</p><button class="btn" id="retry">Retry</button></div>`;
      document.getElementById("retry").onclick = async () => { await invoke("retry_transcription", { id: m.id }); shown = ""; refresh(); };
    } else if (!segments.length) {
      body.innerHTML = '<p class="state">No speech was picked up.</p>';
    } else {
      for (const s of segments) {
        const row = document.createElement("div");
        row.className = "seg-row";
        const me = s.speaker === "me";
        row.innerHTML = `<div class="who${me ? " me" : ""}">${me ? "Me" : "Them"}<span class="t">${clock(s.start_ms / 1000)}</span></div><p class="said">${esc(s.text)}</p>`;
        body.append(row);
      }
    }
  }

  refresh();
}

return meetings;
})();
