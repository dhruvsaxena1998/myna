// AI provider: its Settings page and the Summary tab of a meeting.
// Uses invoke, get, set, icon, esc and ICONS from the inline script in index.html.
// Wrapped so its helpers stay out of the page's global scope; only `ai` is exposed.
const ai = (() => {
// Shown only while a request would leave the Mac, so a local server shows none.
const offMac = '<span class="pill alert offdev">sends text off this Mac</span>';

async function settings(page) {
  let st = await invoke("ai_state");
  const $ = (id) => page.querySelector("#" + id);
  const row = (id, label, control) => `<div class="prow single"><label class="l" for="${id}">${label}</label><div class="r">${control}</div></div>`;
  const text = (id, cls = "wide") => `<input class="field mono ${cls}" id="${id}" spellcheck="false" autocomplete="off">`;
  page.innerHTML = `<section class="group"><h2>Provider</h2><div class="box">
${row("ai-preset", "Preset", `<select class="field" id="ai-preset">${st.presets.map((p) => `<option value="${p.id}">${esc(p.name)}</option>`).join("")}</select>`)}
${row("ai-url", "Base URL", text("ai-url"))}
<div class="prow single"><label class="l" for="ai-key">API key</label><div class="r"><input class="field mono wide" id="ai-key" type="password" spellcheck="false" autocomplete="off"><span class="xs row">${icon("lock", "sm")} Keychain</span><button class="btn quiet" id="ai-key-rm">Remove</button></div><div class="d err" id="ai-err" role="alert" hidden></div></div>
${row("ai-model", "Model", text("ai-model"))}
</div><p class="note">Myna only talks to this provider when a feature below is on. Each one shows ${offMac} where it applies.</p></section>
<section class="group"><h2>Cost estimate (optional)</h2><div class="box">
${row("ai-in", "Input price per 1M tokens", text("ai-in", "num"))}
${row("ai-out", "Output price per 1M tokens", text("ai-out", "num"))}
${row("ai-ctx", "Context window (tokens)", text("ai-ctx", "num"))}
</div><p class="note">Saved for the model named above. Counts are approximate (characters ÷ 4). Leave prices blank to show tokens only.</p></section>
<section class="group"><h2>Uses this provider</h2><div class="box">
<div class="prow single"><div class="l row"><span id="ai-sum-l">Meeting summaries</span> ${offMac}</div><div class="r"><button class="switch" role="switch" id="ai-sum" aria-labelledby="ai-sum-l"></button></div></div>
<div class="prow single"><div class="l row"><span id="ai-chat-l">Chat with a meeting</span> ${offMac}</div><div class="r"><button class="switch" role="switch" id="ai-chat" aria-labelledby="ai-chat-l"></button></div></div>
</div></section>`;

  // The field being typed in keeps its text; everything else follows the saved state.
  const put = (el, v) => { if (el !== document.activeElement) el.value = v ?? ""; };
  function fill() {
    put($("ai-preset"), st.preset);
    put($("ai-url"), st.base_url);
    put($("ai-model"), st.model);
    $("ai-model").placeholder = st.presets.find((p) => p.id === st.preset).model;
    $("ai-key").placeholder = st.has_key ? "Saved in Keychain" : st.preset === "custom" ? "Optional for a local server" : "Paste your key";
    $("ai-key-rm").hidden = !st.has_key;
    for (const [id, v] of [["ai-in", st.input_price], ["ai-out", st.output_price], ["ai-ctx", st.context]]) {
      put($(id), v);
      $(id).disabled = !st.model;
    }
    $("ai-sum").setAttribute("aria-checked", st.summaries);
    $("ai-chat").setAttribute("aria-checked", st.chat);
    page.querySelectorAll(".offdev").forEach((p) => { p.hidden = !st.off_device; });
  }
  const sync = async () => { st = await invoke("ai_state"); fill(); };
  const save = async (key, value) => { await set(key, value); await sync(); };
  const setKey = async (key) => {
    const err = $("ai-err");
    err.hidden = true;
    try { await invoke("ai_set_key", { preset: st.preset, key }); } catch (e) { err.textContent = String(e); err.hidden = false; }
    await sync();
  };

  $("ai-preset").onchange = async (e) => {
    const p = st.presets.find((p) => p.id === e.target.value);
    await set("ai_preset", p.id);
    await save("ai_base_url", p.base_url);
  };
  $("ai-url").onchange = (e) => save("ai_base_url", e.target.value.trim());
  $("ai-model").onchange = (e) => save("ai_model", e.target.value.trim());
  $("ai-key").onchange = (e) => { const key = e.target.value; e.target.value = ""; setKey(key); };
  $("ai-key-rm").onclick = () => setKey("");
  // Price and context window are per model: the backend reads ai_in:<model>, ai_out:<model>, ai_ctx:<model>.
  for (const [id, kind] of [["ai-in", "in"], ["ai-out", "out"], ["ai-ctx", "ctx"]]) $(id).onchange = (e) => save(`ai_${kind}:${st.model}`, e.target.value.trim());
  $("ai-sum").onclick = () => save("ai_summaries", !st.summaries);
  $("ai-chat").onclick = () => save("ai_chat", !st.chat);
  fill();
}

const fmtTokens = (n) => (n >= 1000 ? (n / 1000).toFixed(1) + "k" : String(n));
const fmtCost = (c) => (c < 0.001 ? "<$0.001" : "$" + c.toFixed(c < 0.1 ? 3 : 2));
const inline = (t) => esc(t).replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>").replace(/`(.+?)`/g, "<code>$1</code>");
// Models answer in Markdown. Headings, bullets and paragraphs are enough here; everything is escaped first.
function md(text) {
  let html = "";
  let list = false;
  for (const line of text.split("\n").map((l) => l.trim())) {
    const li = /^(?:[-*•]|\d+[.)])\s+(.*)/.exec(line);
    if (list && !li) { html += "</ul>"; list = false; }
    if (li) { html += (list ? "" : "<ul>") + `<li>${inline(li[1])}</li>`; list = true; }
    else if (/^#{1,6}\s/.test(line)) html += `<h3>${inline(line.replace(/^#+\s*/, ""))}</h3>`;
    else if (line) html += `<p>${inline(line)}</p>`;
  }
  return html + (list ? "</ul>" : "");
}

const inflight = new Map(); // meeting id -> request, so reopening a meeting mid-request still shows it working

// The Summary tab of the meeting detail. `ready` is false until there is a transcript.
async function summary(pane, m, ready) {
  if (!ready) { pane.innerHTML = '<p class="s">A summary is available once the transcript is ready.</p>'; return; }
  try {
    const [st, templates0, stored] = await Promise.all([invoke("ai_state"), invoke("template_list"), invoke("summary_get", { id: m.id })]);
    if (!pane.isConnected) return;
    let templates = templates0;
    let current = stored;
    let prompt = stored ? stored.prompt : templates[0].prompt; // the last template used
    let editing = false;
    let error = "";
    const $ = (id) => pane.querySelector("#" + id);

    function draw() {
      const busy = inflight.has(m.id);
      const blocked = st.summaries ? st.problem : "Meeting summaries are off. Turn them on under AI provider in Settings.";
      const picked = templates.find((t) => t.prompt === prompt);
      const options = templates.map((t) => `<option value="${t.id}"${t === picked ? " selected" : ""}>${esc(t.name)} · ${t.builtin ? "built-in" : "mine"}</option>`);
      if (!picked) options.push('<option value="" selected>Custom prompt</option>');
      const when = current && new Date(current.generated_at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
      pane.innerHTML = `<div class="tpl"><select class="field grow" id="tpl" aria-label="Template">${options.join("")}</select><button class="btn" id="go"${busy || blocked ? " disabled" : ""}>${icon("spark", "sm")} ${current ? "Regenerate" : "Generate"}</button></div>
${blocked ? `<p class="s" id="why">${esc(blocked)}</p>` : ""}${error ? `<p class="err" role="alert">${esc(error)}</p>` : ""}${busy ? '<p class="s" role="status">Generating…</p>' : ""}
${editing ? `<div class="edit"><label class="xs" for="prompt">Prompt</label><textarea class="field" id="prompt" rows="6">${esc(prompt)}</textarea>
<div class="foot"><input class="field grow" id="tname" placeholder="Template name" aria-label="Template name" value="${picked && !picked.builtin ? esc(picked.name) : ""}"><button class="btn" id="save">Save as template</button></div>
<div class="foot"><button class="btn quiet" id="del"${picked && !picked.builtin ? "" : " hidden"}>Delete template</button><span class="sp"></span><button class="btn quiet" id="cancel">Cancel</button><button class="btn primary" id="run"${busy || blocked ? " disabled" : ""}>Generate</button></div></div>` : ""}
${current ? `<p class="xs meta">Generated ${when} with ${esc(current.model)} via ${esc(current.provider)} · ${fmtTokens(current.tokens_in + current.tokens_out)} tokens${current.cost == null ? "" : " · ≈ " + fmtCost(current.cost)}${current.off_device ? ` · <span class="pill alert">sent text off this Mac</span>` : ""}</p><div class="out">${md(current.text)}</div>` : blocked || busy ? "" : '<p class="s">No summary yet.</p>'}`;

      $("tpl").onchange = (e) => {
        const t = templates.find((t) => String(t.id) === e.target.value);
        if (t) { prompt = t.prompt; draw(); }
      };
      $("go").onclick = () => (current ? ((editing = true), draw()) : run());
      if (!editing) return;
      $("prompt").oninput = (e) => { prompt = e.target.value; };
      $("cancel").onclick = () => { editing = false; prompt = current ? current.prompt : templates[0].prompt; draw(); };
      $("run").onclick = run;
      $("save").onclick = async () => {
        try {
          prompt = (await invoke("template_save", { name: $("tname").value, prompt })).prompt;
          templates = await invoke("template_list");
          error = "";
        } catch (e) { error = String(e); }
        draw();
      };
      $("del").onclick = async () => {
        await invoke("template_delete", { id: picked.id });
        templates = await invoke("template_list");
        draw();
      };
    }

    function run() {
      error = "";
      editing = false;
      const request = invoke("summary_generate", { id: m.id, prompt })
        .then((s) => { current = s; }, (e) => { error = String(e); })
        .finally(() => { inflight.delete(m.id); if (pane.isConnected) draw(); });
      inflight.set(m.id, request);
      draw();
    }

    if (inflight.has(m.id)) {
      // Reopened mid-request: the answer is stored by the time it ends.
      inflight.get(m.id).then(async () => { current = await invoke("summary_get", { id: m.id }); if (pane.isConnected) draw(); });
    }
    draw();
  } catch (e) {
    pane.innerHTML = `<p class="err" role="alert">${esc(e)}</p>`;
  }
}

const asking = new Map(); // meeting id -> { question, request }: a question still waiting for its answer
const drafts = new Map(); // meeting id -> what was typed and not sent, kept while the detail view redraws

// The Chat tab: one thread per meeting, stored by the backend. `ready` is false until there is a transcript.
async function chat(pane, m, ready) {
  if (!ready) { pane.innerHTML = '<p class="s">Chat is available once the transcript is ready.</p>'; return; }
  try {
    const [st, thread] = await Promise.all([invoke("ai_state"), invoke("chat_get", { id: m.id })]);
    if (!pane.isConnected) return;
    let current = thread;
    let error = "";
    const blocked = st.chat ? st.problem : "Chat is off. Turn it on under AI provider in Settings.";

    function draw() {
      const waiting = asking.get(m.id);
      const said = (t) => `<div class="msg me"><div class="from">You</div><p>${esc(t.question)}</p></div>` + (t.answer == null ? "" : `<div class="msg"><div class="from">${esc(t.model)}</div><div class="out">${md(t.answer)}</div></div>`);
      const turns = current.turns.map(said).join("") + (waiting ? said({ question: waiting.question }) + '<p class="s" role="status">Thinking…</p>' : "");
      pane.innerHTML = `<div class="hdr"><span><b>${fmtTokens(current.context_tokens)}</b> tokens in context</span>${current.turns.length && current.cost != null ? `<span><b>≈ ${fmtCost(current.cost)}</b> this chat</span>` : ""}${st.off_device ? offMac : ""}</div>
${current.over_window ? `<p class="warn" role="status">This chat is about ${fmtTokens(current.context_tokens)} tokens, over the ${fmtTokens(current.window)} window you entered for ${esc(st.model)}. The provider may refuse it or drop the start of the transcript.</p>` : ""}
<div class="log">${turns || '<p class="s">Ask anything about this meeting. Each question sends the transcript, the summary and this chat again.</p>'}</div>
${blocked ? `<p class="s">${esc(blocked)}</p>` : ""}${error ? `<p class="err" role="alert">${esc(error)}</p>` : ""}
<form class="ask" id="ask"><input class="field" id="q" placeholder="Ask about this meeting…" aria-label="Ask" autocomplete="off" spellcheck="true"${waiting || blocked ? " disabled" : ""}><button class="btn icon" aria-label="Send"${waiting || blocked ? " disabled" : ""}>${icon("send", "sm")}</button></form>`;
      const q = pane.querySelector("#q");
      q.value = drafts.get(m.id) || "";
      q.oninput = () => drafts.set(m.id, q.value);
      pane.querySelector("#ask").onsubmit = (e) => { e.preventDefault(); send(q.value.trim()); };
      pane.querySelector(".log").scrollTop = 1e9;
    }

    function send(question) {
      if (!question || asking.has(m.id)) return;
      error = "";
      drafts.delete(m.id);
      const request = invoke("chat_send", { id: m.id, question })
        .then((c) => { current = c; }, (e) => { error = String(e); drafts.set(m.id, question); }) // a failed question goes back in the field
        .finally(() => {
          asking.delete(m.id);
          if (!pane.isConnected) return;
          draw();
          if (!pane.hidden) pane.querySelector("#q").focus();
        });
      asking.set(m.id, { question, request });
      draw();
    }

    if (asking.has(m.id)) {
      // Reopened mid-request: the answer is stored by the time it ends.
      asking.get(m.id).request.then(async () => { current = await invoke("chat_get", { id: m.id }); if (pane.isConnected) draw(); });
    }
    draw();
  } catch (e) {
    pane.innerHTML = `<p class="err" role="alert">${esc(e)}</p>`;
  }
}

let tab = "sum"; // the tab in use, kept while the detail view redraws

// The meeting detail's right-hand column: Summary and Chat.
function inspector(box, m, ready) {
  const tabs = [["sum", "Summary", summary], ["chat", "Chat", chat]];
  box.innerHTML = `<div class="tabs" role="tablist">${tabs.map(([k, label]) => `<button role="tab" id="tab-${k}" aria-selected="${k === tab}" aria-controls="pane-${k}" tabindex="${k === tab ? 0 : -1}">${label}</button>`).join("")}</div>`
    + tabs.map(([k]) => `<div class="pane${k === "chat" ? " chat" : ""}" id="pane-${k}" role="tabpanel" aria-labelledby="tab-${k}"${k === tab ? "" : " hidden"}></div>`).join("");
  const pick = (k) => {
    tab = k;
    for (const [id] of tabs) {
      box.querySelector("#tab-" + id).setAttribute("aria-selected", id === k);
      box.querySelector("#tab-" + id).tabIndex = id === k ? 0 : -1;
      box.querySelector("#pane-" + id).hidden = id !== k;
    }
    const log = box.querySelector("#pane-" + k + " .log");
    if (log) log.scrollTop = 1e9; // the thread was drawn while hidden
  };
  box.querySelector(".tabs").onclick = (e) => { const b = e.target.closest("[role=tab]"); if (b) pick(b.id.slice(4)); };
  box.querySelector(".tabs").onkeydown = (e) => {
    const d = { ArrowLeft: -1, ArrowRight: 1 }[e.key];
    if (!d) return;
    const next = tabs[(tabs.findIndex(([k]) => k === tab) + d + tabs.length) % tabs.length][0];
    pick(next);
    box.querySelector("#tab-" + next).focus();
  };
  for (const [k, , fill] of tabs) fill(box.querySelector("#pane-" + k), m, ready);
}

return { settings, inspector };
})();
