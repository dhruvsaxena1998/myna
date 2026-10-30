// Meetings window: list of recordings and a transcript with the time in the margin, a screenshot filmstrip above it.
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
    play: '<path d="M5 3.5v9l7-4.5z" fill="currentColor" stroke="none"/>',
    chev: '<path d="M6 3.5 10.5 8 6 12.5"/>',
    more: '<circle cx="3.5" cy="8" r="1" fill="currentColor"/><circle cx="8" cy="8" r="1" fill="currentColor"/><circle cx="12.5" cy="8" r="1" fill="currentColor"/>',
    pause: '<path d="M5.5 3.5v9M10.5 3.5v9"/>',
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
  let confirmingMedia = null; // id of the meeting whose media delete is waiting on a confirm
  let hasAudio = true; // the open meeting's tracks are still on disk, so segments can be played
  let tracks = []; // the <audio> elements of the open meeting (mic, and system audio when tapped), played together
  let clip; // the segment clip that is playing
  const pauseAudio = () => [...tracks, clip].forEach((a) => a?.pause());
  const stopAudio = () => { pauseAudio(); tracks = []; clip = null; };
  const opened = new Set(); // ids of segments in collapsed groups the user opened, so a group stays open after an Include
  let people = []; // the open meeting's speakers on the system track
  let segs = []; // and its segments
  let notice = ""; // a line next to Export, e.g. that a voice could not be remembered
  let pop = null; // the rename popover, while it is open
  const side = document.getElementById("side");
  const folder = () => folders.find((f) => String(f.id) === view);
  const inView = (m) => view === "all" || (view === "inbox" ? m.folder_id === null : String(m.folder_id) === view);

  // Runs a folder command and redraws; a refusal (duplicate name, folder gone) shows in the banner.
  async function act(cmd, args) {
    error = "";
    let result;
    try { result = await invoke(cmd, args); } catch (e) { error = String(e); }
    adding = renaming = confirming = false;
    confirmingMedia = null;
    shown = "";
    refresh();
    return result;
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
    const sig = JSON.stringify([open, error, view, folders, renaming, confirming, confirmingMedia, ...(detail ? [detail.meeting.status, detail.meeting.error, detail.segments.length, detail.meeting.folder_id, detail.shots.length, detail.shots_taken, detail.video, detail.media_bytes] : list.map((m) => [m.id, m.status, m.segments, m.app, m.has_them, m.error, m.folder_id]))]);
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
    stopAudio();
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

  const size = (b) => (b >= 1e9 ? (b / 1e9).toFixed(1) + " GB" : Math.max(1, Math.round(b / 1e6)) + " MB");

  function renderDetail({ meeting: m, segments, speakers, shots, shots_taken, audio, video, media_bytes }) {
    stopAudio();
    closeRename();
    people = speakers;
    segs = segments;
    hasAudio = audio.length > 0;
    // Media can go once the transcript no longer needs the audio.
    const canDelete = media_bytes > 0 && m.status !== "recording" && m.status !== "transcribing";
    const d = new Date(m.started_at * 1000);
    const when = `${d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} · ${hhmm(d)}${m.duration_ms ? " · " + mins(m.duration_ms) : ""}`;
    main.innerHTML = `<div class="toolbar"><button class="btn quiet icon" id="back" aria-label="Back to Meetings">${icon("back")}</button><h1>Meetings</h1><span class="sp"></span><span class="s" id="saved" role="status">${esc(notice)}</span>${audio.length && m.status === "done" ? `<button class="btn" id="play" aria-label="Play recording">${icon("play", "sm")}<span id="pos"></span></button>` : ""}<div class="menu-wrap"><button class="btn" id="export" aria-haspopup="menu" aria-expanded="false"${m.status === "done" ? "" : " disabled"}>Export</button><div class="menu" role="menu" id="menu" hidden><button class="mi" role="menuitem" data-fmt="md">Markdown (.md)</button><button class="mi" role="menuitem" data-fmt="txt">Plain text (.txt)</button></div></div><div class="menu-wrap"><button class="btn quiet icon" id="more" aria-label="More" aria-haspopup="menu" aria-expanded="false">${icon("more")}</button><div class="menu" role="menu" id="move-menu" aria-label="More" hidden><div class="mhead" aria-hidden="true">Move to</div>${[{ id: null, name: "Inbox" }, ...folders].map((f) => `<button class="mi" role="menuitemradio" aria-checked="${f.id === m.folder_id}" data-folder="${f.id}">${icon("check")}${esc(f.name)}</button>`).join("")}${canDelete ? `<div class="msep" role="separator"></div><button class="mi danger" role="menuitem" id="del-media">${icon("check")}Delete audio, video and screenshots…</button>` : ""}</div></div></div>${error ? `<div class="banner" role="alert">${esc(error)}</div>` : ""}${confirmingMedia === m.id && canDelete ? `<div class="banner ask" role="alert"><span>Delete this meeting’s audio, video and screenshots (${size(media_bytes)})? The transcript and summary stay. This can’t be undone.</span><button class="btn" id="media-cancel">Cancel</button><button class="btn danger" id="media-confirm">Delete</button></div>` : ""}<div class="detail"><div class="doc">${filmstrip(shots, shots_taken, m.status === "done" ? video : null)}<div class="tx"><h2>${esc(m.title)}</h2><div class="sub"><span>${esc(when)}</span><span>${esc(m.app || "Mic only")}</span><span class="pill neutral">Parakeet v3</span>${m.status === "done" && !media_bytes ? '<span class="pill neutral">Audio, video and screenshots deleted</span>' : ""}</div>${m.note ? `<p class="note">${esc(m.note)}</p>` : ""}<div id="body"></div></div></div><div class="inspector" id="insp"></div></div>`;
    ai.inspector(document.getElementById("insp"), m, m.status === "done" && segments.length > 0);
    if (m.status === "done" && segments.length) ai.suggestion(document.getElementById("insp"), m, (f) => act("move_meeting", { id: m.id, folder: f.id }));
    document.getElementById("back").onclick = () => { open = null; confirmingMedia = null; notice = ""; shown = ""; refresh(); };
    const menu = document.getElementById("menu");
    const close = dropdown(document.getElementById("export"), menu);
    const moveMenu = document.getElementById("move-menu");
    const closeMove = dropdown(document.getElementById("more"), moveMenu);
    moveMenu.querySelectorAll(".mi[data-folder]").forEach((b) => {
      b.onclick = () => { closeMove(); act("move_meeting", { id: m.id, folder: b.dataset.folder === "null" ? null : Number(b.dataset.folder) }); };
    });
    const on = (id, fn) => { const el = document.getElementById(id); if (el) el.onclick = fn; };
    on("del-media", () => { closeMove(); confirmingMedia = m.id; shown = ""; refresh(); });
    on("media-cancel", () => { confirmingMedia = null; shown = ""; refresh(); });
    on("media-confirm", () => act("delete_media", { id: m.id }));
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
      // Lines that were possibly muted collapse into a group where they happened, until the user includes them.
      let run = [];
      const flush = () => { if (run.length) body.append(mutedGroup(run)); run = []; };
      for (const s of segments) {
        if (s.mute === "unmuted" || s.included) { flush(); body.append(segRow(s)); } else run.push(s);
      }
      flush();
      body.onclick = async (e) => {
        const b = e.target.closest("[data-play], [data-include], [data-undo], [data-spk]");
        if (!b) return;
        if (b.dataset.spk) return openRename(b, Number(b.dataset.spk), m);
        if (b.dataset.play) return play(b.dataset.play);
        await act("include_segment", { id: Number(b.dataset.include || b.dataset.undo), included: !!b.dataset.include });
      };
    }
    wirePlayer(m, audio);
    const film = main.querySelector(".film");
    if (film) film.onclick = (e) => {
      const f = e.target.closest(".f");
      if (!f) return;
      film.querySelector('[aria-current="true"]')?.removeAttribute("aria-current");
      f.setAttribute("aria-current", "true");
      seek(Number(f.dataset.ms));
    };
  }

  // The window video, if one was recorded, and the frames kept while recording, on the timeline; a click on a frame
  // jumps the transcript, the audio and the video to that moment.
  function filmstrip(shots, taken, video) {
    if (!shots.length && !video) return "";
    const src = window.__TAURI__.core.convertFileSrc;
    const frames = shots.map((s) => `<button class="f" data-ms="${s.at_ms}" data-t="${clock(s.at_ms / 1000)}" aria-label="Screenshot at ${clock(s.at_ms / 1000)}"><img src="${esc(src(s.path))}" alt="" loading="lazy" decoding="async"></button>`);
    const vid = video ? `<video class="vid" src="${esc(src(video))}" controls muted playsinline preload="metadata" aria-label="Window video"></video>` : "";
    const film = shots.length ? `<div class="film" aria-label="Screenshots">${frames.join("")}</div><div class="ruler"><span>${shots.length} kept of ${taken} taken</span></div>` : "";
    return `<div class="filmbar">${vid}${film}</div>`;
  }

  function seek(ms) {
    tracks.forEach((a) => { a.currentTime = ms / 1000; });
    const rows = [...main.querySelectorAll(".seg-row")];
    const row = rows.filter((r) => Number(r.dataset.ms) <= ms).at(-1) || rows[0];
    main.querySelector(".seg-row.cur")?.classList.remove("cur");
    if (row) { row.classList.add("cur"); row.scrollIntoView({ block: "center", behavior: "smooth" }); }
  }

  // One Play button for both tracks. shortcut: two <audio> elements drift apart a little over a long meeting;
  // mix the tracks into one file if that is ever audible.
  function wirePlayer(m, files) {
    const btn = document.getElementById("play");
    if (!btn) return;
    const src = window.__TAURI__.core.convertFileSrc;
    // Metadata only: a long meeting's WAV is tens of MB, fetched as it plays.
    tracks = files.map((f) => Object.assign(new Audio(), { src: src(f), preload: "metadata" }));
    const [lead] = tracks;
    // The video has no sound of its own and follows the tracks; its own controls drive them too.
    const video = main.querySelector(".vid");
    if (video) {
      tracks.push(video);
      video.onplay = () => tracks.forEach((a) => a.paused && a.play().catch(() => {}));
      video.onpause = () => video.ended || tracks.forEach((a) => a.pause()); // a video that ends first must not stop the audio
      video.onseeked = () => tracks.forEach((a) => { if (Math.abs(a.currentTime - video.currentTime) > 0.5) a.currentTime = video.currentTime; });
    }
    const pos = document.getElementById("pos");
    const total = clock(m.duration_ms / 1000);
    const show = () => {
      pos.textContent = `${clock(lead.currentTime)} / ${total}`;
      btn.firstElementChild.outerHTML = icon(lead.paused ? "play" : "pause", "sm");
      btn.setAttribute("aria-label", lead.paused ? "Play recording" : "Pause recording");
    };
    show();
    btn.onclick = () => {
      clip?.pause();
      const go = lead.paused; // read once: play() flips it at once, which would pause the tracks after the first
      tracks.forEach((a) => (go ? a.play().catch(() => {}) : a.pause()));
    };
    for (const ev of ["play", "pause", "timeupdate", "seeked"]) lead.addEventListener(ev, show);
    lead.addEventListener("ended", () => tracks.forEach((a) => a.pause()));
  }

  const speakerName = (c) => people.find((p) => p.cluster === c)?.name || `Speaker ${c}`;

  // Me; "Them" when the speakers weren't separated; otherwise the speaker's name, a button that opens the rename popover.
  function whoHtml(s) {
    if (s.speaker === "me") return "Me";
    if (s.cluster == null) return "Them";
    const p = people.find((x) => x.cluster === s.cluster);
    const chip = p?.suggested && !p.name ? `<span class="sugg">suggested: ${esc(p.suggested)}</span>` : "";
    return `<button class="spk" data-spk="${s.cluster}" aria-haspopup="dialog">${esc(speakerName(s.cluster))}</button>${chip}`;
  }

  function segRow(s) {
    const row = document.createElement("div");
    row.className = "seg-row";
    row.dataset.ms = s.start_ms;
    const me = s.speaker === "me";
    const mark = s.included ? ` <span class="pill neutral">${s.auto ? "Auto-included" : "Included"}</span><button class="btn quiet" data-undo="${s.id}">Undo</button>` : "";
    row.innerHTML = `<div class="who${me ? " me" : ""}">${whoHtml(s)}<span class="t">${clock(s.start_ms / 1000)}</span></div><p class="said">${esc(s.text)}${mark}</p><div class="inc">${hasAudio ? `<button class="btn quiet icon" data-play="${s.id}" aria-label="Play">${icon("play", "sm")}</button>` : ""}</div>`;
    return row;
  }

  function closeRename() {
    if (!pop) return;
    const row = pop.parentElement;
    pop.remove();
    pop = null;
    document.removeEventListener("click", outside);
    row?.querySelector(".spk")?.focus();
  }
  function outside(e) {
    // The path, not `contains`: a click that redraws the popover detaches its own target before this runs.
    if (pop && !e.composedPath().includes(pop) && !e.target.closest("[data-spk]")) closeRename();
  }

  // "Who is this?": name the speaker (every line of theirs follows), hear their longest clips, or merge them into another.
  async function openRename(btn, cluster, m) {
    closeRename();
    const p = people.find((x) => x.cluster === cluster);
    const count = (c) => segs.filter((s) => s.speaker === "them" && s.cluster === c).length;
    const n = count(cluster);
    const sugg = p.suggested && !p.name;
    const clips = hasAudio ? await invoke("speaker_clips", { meeting: m.id, cluster }).catch(() => []) : [];
    const row = btn.closest(".seg-row");
    pop = document.createElement("div");
    pop.className = "pop";
    pop.setAttribute("role", "dialog");
    pop.setAttribute("aria-label", "Who is this?");
    const others = people.filter((x) => x.cluster !== cluster);
    const main = () => `<div class="arrow"></div><h3>Who is this?</h3><input class="field" id="spk-name" aria-label="Speaker name" maxlength="80" placeholder="Speaker ${cluster}" value="${esc(p.name || p.suggested || "")}">`
      + (sugg ? `<div class="s">${p.from_captions ? "Named by the call’s captions." : "Sounds like a voice you remembered."} <button class="btn quiet" data-dismiss>Dismiss</button></div>` : "")
      + `<label class="row pick"><input type="checkbox" id="spk-remember"${sugg && !p.from_captions ? " checked" : ""}><span class="radio"></span>Remember this voice for future meetings</label>`
      + (clips.length ? `<div class="xs head">Longest clips from this speaker</div>${clips.map((c) => `<div class="clip"><button class="btn quiet icon" data-clip="${c.id}" data-secs="${c.secs}" aria-label="Play the clip from ${clock(c.start_ms / 1000)}">${icon("play", "sm")}</button><span class="wave" aria-hidden="true"></span><span class="mono">${clock(c.start_ms / 1000)} · ${c.secs} s</span></div>`).join("")}` : "")
      + `<div class="foot"><button class="btn quiet" data-merge${others.length ? "" : " disabled"}>Merge into…</button><button class="btn primary" data-save>Rename ${n} segment${n === 1 ? "" : "s"}</button></div>`;
    const mergeView = () => `<div class="arrow"></div><h3>Merge ${esc(speakerName(cluster))} into…</h3>${others.map((o) => `<button class="mi" data-into="${o.cluster}">${esc(speakerName(o.cluster))}<span class="xs">${count(o.cluster)} segment${count(o.cluster) === 1 ? "" : "s"}</span></button>`).join("")}<div class="foot"><button class="btn quiet" data-back>Back</button></div>`;
    const draw = (html) => { pop.innerHTML = html; (pop.querySelector("#spk-name") || pop.querySelector(".mi")).focus(); };
    const save = async () => {
      const remember = pop.querySelector("#spk-remember").checked;
      const name = pop.querySelector("#spk-name").value;
      notice = "";
      const stored = await act("speaker_rename", { meeting: m.id, cluster, name, remember });
      if (remember && stored === false) {
        notice = "Renamed. There was too little of their speech to remember the voice.";
        const saved = document.getElementById("saved"); // the redraw may already have happened
        if (saved) saved.textContent = notice;
      }
    };
    pop.onclick = (e) => {
      const b = e.target.closest("button");
      if (!b) return;
      if (b.dataset.clip) return play(b.dataset.clip, Number(b.dataset.secs) * 1000);
      if (b.dataset.save !== undefined) return save();
      if (b.dataset.dismiss !== undefined) return act("speaker_dismiss", { meeting: m.id, cluster });
      if (b.dataset.merge !== undefined) return draw(mergeView());
      if (b.dataset.back !== undefined) return draw(main());
      if (b.dataset.into) return act("speaker_merge", { meeting: m.id, from: cluster, into: Number(b.dataset.into) });
    };
    pop.onkeydown = (e) => {
      if (e.key === "Escape") closeRename();
      if (e.key === "Enter" && e.target.id === "spk-name") save();
    };
    row.append(pop);
    const who = row.querySelector(".who");
    pop.style.top = who.offsetTop + who.offsetHeight + 8 + "px"; // just under the name, its chip and the time
    draw(main());
    pop.scrollIntoView({ block: "nearest" });
    document.addEventListener("click", outside);
  }

  function mutedGroup(run) {
    const g = document.createElement("details");
    g.className = "muted-group";
    g.open = run.some((s) => opened.has(s.id));
    g.ontoggle = () => run.forEach((s) => (g.open ? opened.add(s.id) : opened.delete(s.id)));
    const n = run.length;
    const span = n > 1 ? ` – ${clock(run.at(-1).end_ms / 1000)}` : "";
    g.innerHTML = `<summary><span class="dot live"></span> ${n} segment${n === 1 ? "" : "s"} while possibly muted · ${clock(run[0].start_ms / 1000)}${span} <span class="s">not in transcript, not sent anywhere</span>${icon("chev", "sm chev")}</summary>`
      + run.map((s) => `<div class="seg-row"><div class="who me">Me<span class="t">${clock(s.start_ms / 1000)}</span></div><p class="said">${esc(s.text)}</p><div class="inc">${hasAudio ? `<button class="btn quiet icon" data-play="${s.id}" aria-label="Play">${icon("play", "sm")}</button>` : ""}<button class="btn" data-include="${s.id}">Include</button></div></div>`).join("");
    return g;
  }

  async function play(id, maxMs = null) {
    pauseAudio();
    try {
      const url = URL.createObjectURL(new Blob([await invoke("segment_clip", { id: Number(id), maxMs })], { type: "audio/wav" }));
      clip = new Audio(url);
      clip.onended = () => URL.revokeObjectURL(url);
      await clip.play();
    } catch (e) { error = "Couldn't play that: " + e; shown = ""; refresh(); }
  }

  refresh();
}

return meetings;
})();
