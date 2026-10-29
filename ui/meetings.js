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
    plus: '<path d="M8 3v10M3 8h10"/>',
    more: '<circle cx="3.5" cy="8" r="1" fill="currentColor"/><circle cx="8" cy="8" r="1" fill="currentColor"/><circle cx="12.5" cy="8" r="1" fill="currentColor"/>',
  });
  document.body.className = "set meet";
  app.innerHTML = `<nav class="sidebar" aria-label="Meetings" id="side"></nav><div class="content" id="main"></div>`;
  const main = document.getElementById("main");
  let open = Number(location.hash.split("/")[1]) || null; // id of the meeting shown in detail, or null for the list
  let shown = "";
  let timer;
  let error = "";
  let view = "all"; // "all", "inbox" or a folder id
  let folders = [];
  let adding = false; // the new-folder field is open in the sidebar
  let renaming = false;
  let confirming = false; // asked to delete the open folder
  const side = document.getElementById("side");
  const folder = () => folders.find((f) => String(f.id) === view);
  const inView = (m) => view === "all" || (view === "inbox" ? m.folder_id === null : String(m.folder_id) === view);

  // Runs a folder command and redraws; a refusal (duplicate name, folder gone) shows in the banner.
  async function act(cmd, args) {
    error = "";
    try { await invoke(cmd, args); } catch (e) { error = String(e); }
    adding = renaming = confirming = false;
    shown = "";
    refresh();
  }
  const show = (v) => { view = v; open = null; error = ""; adding = renaming = confirming = false; shown = ""; refresh(); };

  function renderSide(list) {
    const nav = (key, label, count, drop = "") =>
      `<button class="nav" data-view="${key}"${drop === "" ? "" : ` data-drop="${drop}"`}${view === String(key) ? ' aria-current="page"' : ""}>${label}<span class="k">${count || ""}</span></button>`;
    const n = (id) => list.filter((m) => m.folder_id === id).length;
    side.innerHTML = nav("all", `${icon("list")}<span class="lbl">Meetings</span>`, list.length)
      + "<h2>Folders</h2>"
      + nav("inbox", '<span class="lbl">Inbox</span>', n(null), "inbox")
      + folders.map((f) => nav(f.id, `<span class="lbl">${esc(f.name)}</span>`, n(f.id), f.id)).join("")
      + (adding ? '<input class="field new" id="new-folder" aria-label="New folder name" placeholder="Folder name" maxlength="80">' : `<button class="nav add" id="add">${icon("plus", "sm")}<span class="lbl">New folder</span></button>`);
    const input = document.getElementById("new-folder");
    if (input) {
      input.focus();
      input.onkeydown = (e) => {
        if (e.key === "Enter") act("create_folder", { name: input.value });
        if (e.key === "Escape") { adding = false; shown = ""; refresh(); }
      };
      input.onblur = () => { if (adding && !input.value.trim()) { adding = false; shown = ""; refresh(); } };
    }
  }

  side.onclick = (e) => {
    if (e.target.closest("#add")) { adding = true; renderSide(lastList); return; }
    const b = e.target.closest("[data-view]");
    if (b) show(b.dataset.view);
  };
  // Dragging a meeting onto Inbox or a folder files it there; the Move to menu does the same from the keyboard.
  let lastList = [];
  const target = (e) => e.target.closest("[data-drop]");
  side.ondragover = (e) => { const t = target(e); if (t) { e.preventDefault(); t.classList.add("drop"); } };
  side.ondragleave = (e) => target(e)?.classList.remove("drop");
  side.ondrop = (e) => {
    const t = target(e);
    const id = Number(e.dataTransfer.getData("text/plain"));
    if (!t || !id) return;
    e.preventDefault();
    act("move_meeting", { id, folder: t.dataset.drop === "inbox" ? null : Number(t.dataset.drop) });
  };

  // A button that opens a menu: Escape and arrows work, and a click elsewhere closes it.
  function dropdown(btn, menu) {
    const close = () => { menu.hidden = true; btn.setAttribute("aria-expanded", "false"); };
    btn.onclick = (e) => {
      e.stopPropagation();
      menu.hidden = !menu.hidden;
      btn.setAttribute("aria-expanded", String(!menu.hidden));
      if (!menu.hidden) (menu.querySelector('[aria-checked="true"]') || menu.querySelector(".mi")).focus();
    };
    menu.onkeydown = (e) => {
      if (e.key === "Escape") { close(); btn.focus(); }
      const d = { ArrowDown: 1, ArrowUp: -1 }[e.key];
      if (d) {
        e.preventDefault();
        const items = [...menu.querySelectorAll(".mi")];
        items[(items.indexOf(document.activeElement) + d + items.length) % items.length].focus();
      }
    };
    document.addEventListener("click", close, { once: true });
    return close;
  }

  // The menubar popover opens a meeting in a window that is already showing.
  window.__TAURI__.event.listen("open-meeting", ({ payload }) => { view = "all"; open = payload; error = ""; shown = ""; refresh(); });

  async function refresh() {
    clearTimeout(timer);
    if (!main.isConnected) return;
    const [list, fs] = await Promise.all([invoke("list_meetings"), invoke("list_folders")]);
    folders = fs;
    lastList = list;
    if (view !== "all" && view !== "inbox" && !folder()) view = "all"; // its folder was deleted
    const detail = open === null ? null : await invoke("get_meeting", { id: open }).catch(() => null);
    if (open !== null && !detail) open = null;
    const sig = JSON.stringify([open, error, view, folders, renaming, confirming, ...(detail ? [detail.meeting.status, detail.meeting.error, detail.segments.length, detail.meeting.folder_id] : list.map((m) => [m.id, m.status, m.segments, m.app, m.has_them, m.error, m.folder_id]))]);
    if (sig !== shown) {
      shown = sig;
      if (!adding) renderSide(list);
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

  function folderTools() {
    const f = folder();
    if (!f) return `<h1>${view === "inbox" ? "Inbox" : "Meetings"}</h1>`;
    if (renaming) return `<input class="field title" id="rename" aria-label="Folder name" maxlength="80" value="${esc(f.name)}">`;
    if (confirming) return `<h1>${esc(f.name)}</h1><span class="s" role="alert">Delete this folder? Its meetings return to Inbox.</span><button class="btn" id="cancel">Cancel</button><button class="btn danger" id="confirm">Delete</button>`;
    return `<h1>${esc(f.name)}</h1><button class="btn quiet" id="rename-btn">Rename</button><button class="btn quiet" id="delete-btn">Delete</button>`;
  }

  function wireFolderTools() {
    const f = folder();
    const on = (id, fn) => { const el = document.getElementById(id); if (el) el.onclick = fn; };
    const redraw = (r, c) => { renaming = r; confirming = c; shown = ""; refresh(); };
    on("rename-btn", () => redraw(true, false));
    on("delete-btn", () => redraw(false, true));
    on("cancel", () => redraw(false, false));
    on("confirm", () => act("delete_folder", { id: f.id }));
    const input = document.getElementById("rename");
    if (input) {
      input.focus();
      input.select();
      input.onkeydown = (e) => {
        if (e.key === "Enter") act("rename_folder", { id: f.id, name: input.value });
        if (e.key === "Escape") redraw(false, false);
      };
    }
  }

  function renderList(all) {
    const recording = all.some((m) => m.status === "recording");
    const list = all.filter(inView);
    main.innerHTML = `<div class="toolbar">${folderTools()}<span class="sp"></span><button class="btn primary" id="rec">${icon(recording ? "stop" : "rec", "sm")} ${recording ? "Stop and transcribe" : "Record"}</button></div>${error ? `<div class="banner" role="alert">${esc(error)}</div>` : ""}<div class="list" id="list"></div>`;
    document.getElementById("rec").onclick = toggle;
    wireFolderTools();
    const box = document.getElementById("list");
    if (!list.length) {
      box.innerHTML = view === "all"
        ? '<div class="empty"><h2>No meetings yet</h2><p>Join your call, then press Record or choose “Start meeting recording” in the menubar.</p></div>'
        : `<div class="empty"><h2>No meetings in ${view === "inbox" ? "Inbox" : esc(folder().name)}</h2><p>Drag a meeting here, or open it and use More → Move to.</p></div>`;
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
      item.draggable = true;
      item.ondragstart = (e) => e.dataTransfer.setData("text/plain", m.id);
      item.dataset.id = m.id;
      item.innerHTML = `<div class="t">${esc(m.title)}</div><div class="when">${hhmm(d)}${m.duration_ms ? " · " + mins(m.duration_ms) : ""}</div><div class="meta"><span class="av" aria-hidden="true"><i>M</i>${m.has_them ? "<i>T</i>" : ""}</span><span>${esc(m.app || "Mic only")}</span>·<span>${statusHtml(m)}</span></div>`;
      item.onclick = () => { open = m.id; error = ""; shown = ""; refresh(); };
      box.append(item);
    }
  }

  function renderDetail({ meeting: m, segments }) {
    const d = new Date(m.started_at * 1000);
    const when = `${d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} · ${hhmm(d)}${m.duration_ms ? " · " + mins(m.duration_ms) : ""}`;
    main.innerHTML = `<div class="toolbar"><button class="btn quiet icon" id="back" aria-label="Back to Meetings">${icon("back")}</button><h1>Meetings</h1><span class="sp"></span><span class="s" id="saved" role="status"></span><div class="menu-wrap"><button class="btn" id="export" aria-haspopup="menu" aria-expanded="false"${m.status === "done" ? "" : " disabled"}>Export</button><div class="menu" role="menu" id="menu" hidden><button class="mi" role="menuitem" data-fmt="md">Markdown (.md)</button><button class="mi" role="menuitem" data-fmt="txt">Plain text (.txt)</button></div></div><div class="menu-wrap"><button class="btn quiet icon" id="more" aria-label="More" aria-haspopup="menu" aria-expanded="false">${icon("more")}</button><div class="menu" role="menu" id="move-menu" aria-label="Move to" hidden><div class="mhead" aria-hidden="true">Move to</div>${[{ id: null, name: "Inbox" }, ...folders].map((f) => `<button class="mi" role="menuitemradio" aria-checked="${f.id === m.folder_id}" data-folder="${f.id}">${icon("check")}${esc(f.name)}</button>`).join("")}</div></div></div>${error ? `<div class="banner" role="alert">${esc(error)}</div>` : ""}<div class="doc"><div class="tx"><h2>${esc(m.title)}</h2><div class="sub"><span>${esc(when)}</span><span>${esc(m.app || "Mic only")}</span><span class="pill neutral">Parakeet v3</span></div>${m.note ? `<p class="note">${esc(m.note)}</p>` : ""}<div id="body"></div></div></div>`;
    document.getElementById("back").onclick = () => { open = null; shown = ""; refresh(); };
    const menu = document.getElementById("menu");
    const close = dropdown(document.getElementById("export"), menu);
    const moveMenu = document.getElementById("move-menu");
    const closeMove = dropdown(document.getElementById("more"), moveMenu);
    moveMenu.querySelectorAll(".mi").forEach((b) => {
      b.onclick = () => { closeMove(); act("move_meeting", { id: m.id, folder: b.dataset.folder === "null" ? null : Number(b.dataset.folder) }); };
    });
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
