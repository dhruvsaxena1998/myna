// AI provider: its Settings page and the Summary tab of a meeting.
// Uses invoke, get, set, icon, esc and ICONS from the inline script in index.html.
// Wrapped so its helpers stay out of the page's global scope; only `ai` is exposed.
const ai = (() => {
// Shown only while a request would leave the Mac, so a local server shows none.
const offMac = '<span class="pill alert offdev">sends text off this Mac</span>';
const offImg = '<span class="pill alert offdev">sends images off this Mac</span>';
// Screenshots go with summaries and chat only when the experimental toggle and the model's images box are both on.
const withImages = (st) => st.shots && st.images;

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
<section class="group"><h2>This model</h2><div class="box">
<div class="prow single"><div class="l">Price</div><div class="r"><span class="xs meta" id="ai-price"></span></div></div>
${row("ai-ctx", "Context window (tokens)", text("ai-ctx", "num"))}
<div class="prow single"><div class="l" id="ai-img-l">Model accepts images</div><div class="r"><button class="switch" role="switch" id="ai-img" aria-labelledby="ai-img-l"></button></div></div>
</div><p class="note">Saved for the model named above. The price comes from the model (OpenRouter's list, or Myna's own table), never from you; a model in neither shows “cost unknown”. Counts are approximate (characters ÷ 4). “Model accepts images” only matters for the two screenshot options under Experimental.</p></section>
<section class="group"><h2>Uses this provider</h2><div class="box">
<div class="prow single"><div class="l row"><span id="ai-sum-l">Meeting summaries</span> ${offMac}</div><div class="r"><button class="switch" role="switch" id="ai-sum" aria-labelledby="ai-sum-l"></button></div></div>
<div class="prow single"><div class="l row"><span id="ai-fold-l">Suggest a folder for new meetings</span> ${offMac}</div><div class="r"><button class="switch" role="switch" id="ai-fold" aria-labelledby="ai-fold-l"></button></div></div>
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
    put($("ai-ctx"), st.context);
    $("ai-ctx").disabled = !st.model;
    $("ai-price").textContent = !st.model ? "Add a model to see its price" : st.input_price == null ? "Cost unknown" : st.input_price + st.output_price === 0 ? "Free" : `${fmtPrice(st.input_price)} in · ${fmtPrice(st.output_price)} out, per 1M tokens`;
    $("ai-img").setAttribute("aria-checked", st.images);
    $("ai-img").disabled = !st.model;
    $("ai-sum").setAttribute("aria-checked", st.summaries);
    $("ai-fold").setAttribute("aria-checked", st.folders);
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
  // The context window is per model: the backend reads ai_ctx:<model>.
  $("ai-ctx").onchange = (e) => save(`ai_ctx:${st.model}`, e.target.value.trim());
  $("ai-img").onclick = () => save(`ai_img:${st.model}`, !st.images);
  $("ai-sum").onclick = () => save("ai_summaries", !st.summaries);
  $("ai-fold").onclick = () => save("ai_folders", !st.folders);
  $("ai-chat").onclick = () => save("ai_chat", !st.chat);
  fill();
}

// Decision provider: yes/no questions for the muted-segment triage, from Laya on this Mac or a hosted service.
async function decision(page) {
  let st = await invoke("decision_state");
  const $ = (id) => page.querySelector("#" + id);
  const row = (id, label, control, cls = "") => `<div class="prow single ${cls}" id="${id}-row"><label class="l" for="${id}">${label}</label><div class="r">${control}</div></div>`;
  const text = (id, cls = "wide") => `<input class="field mono ${cls}" id="${id}" spellcheck="false" autocomplete="off">`;
  page.innerHTML = `<section class="group"><h2>Decision model</h2><div class="box">
${row("dec-preset", `Preset ${offMac}`, `<select class="field" id="dec-preset">${st.presets.map((p) => `<option value="${p.id}">${esc(p.name)}</option>`).join("")}</select>`)}
${row("dec-wire", "Wire format", '<select class="field" id="dec-wire"><option value="jev">Jev</option><option value="openai">OpenAI Decisions</option></select>')}
${row("dec-url", "Base URL", text("dec-url"))}
<div class="prow single" id="dec-key-row"><label class="l" for="dec-key">API key</label><div class="r"><input class="field mono wide" id="dec-key" type="password" spellcheck="false" autocomplete="off"><span class="xs row">${icon("lock", "sm")} Keychain</span><button class="btn quiet" id="dec-key-rm">Remove</button></div><div class="d err" id="dec-err" role="alert" hidden></div></div>
${row("dec-model", "Model", text("dec-model"))}
</div><p class="note" id="dec-note"></p></section>
<section class="group" id="laya"><h2>Laya on this Mac</h2><div class="box">
<div class="prow"><div class="l">Local decision model</div><div class="d" id="laya-d"></div><div class="r" id="laya-r"></div></div>
</div><p class="note">Myna installs Laya into its own Python environment with uv, starts it when triage needs it, and stops it when you quit. It listens on this Mac only, behind a key Myna generated.</p></section>`;

  const put = (el, v) => { if (el !== document.activeElement) el.value = v ?? ""; };
  function fill() {
    const laya = st.preset === "laya", custom = st.preset === "custom";
    put($("dec-preset"), st.preset);
    $("dec-wire").value = st.wire;
    put($("dec-url"), st.base_url);
    put($("dec-model"), st.model);
    $("dec-model").placeholder = st.presets.find((p) => p.id === st.preset).model;
    $("dec-key").placeholder = st.has_key ? "Saved in Keychain" : custom ? "Optional for a local server" : "Paste your key";
    $("dec-key-rm").hidden = !st.has_key;
    $("dec-wire-row").hidden = !custom;
    $("dec-url-row").hidden = laya;
    $("dec-key-row").hidden = laya;
    $("dec-model-row").hidden = laya || st.wire !== "openai";
    page.querySelector(".offdev").hidden = !st.off_device;
    $("dec-note").textContent = "Used for one thing right now: deciding whether a possibly-muted segment belongs in the conversation, when that experimental option is on.";
    $("laya").hidden = !laya;
    const l = st.laya;
    $("laya-d").className = "d" + (l.error ? " err" : "");
    $("laya-d").textContent = l.error || (l.step ? l.step + "…" : l.installed ? "Installed in " + l.dir : "Needs uv. Installs about 2 GB the first time.");
    $("laya-r").innerHTML = l.step ? '<button class="btn" disabled>Setting up…</button>'
      : l.installed ? `<span class="pill ok">${icon("check", "sm")} ${l.running ? "Running" : "Installed"}</span>${l.running ? '<button class="btn quiet" id="laya-stop">Stop</button>' : ""}`
      : `<button class="btn" id="laya-setup">${l.error ? "Try again" : "Set up Laya"}</button>`;
    if ($("laya-setup")) $("laya-setup").onclick = async () => { await invoke("laya_setup"); await sync(); };
    if ($("laya-stop")) $("laya-stop").onclick = async () => { await invoke("laya_stop"); await sync(); };
    clearTimeout(pollTimer);
    if (l.step) pollTimer = setTimeout(() => page.isConnected && sync(), 1000);
  }
  const sync = async () => { st = await invoke("decision_state"); fill(); };
  const save = async (key, value) => { await set(key, value); await sync(); };
  const setKey = async (key) => {
    const err = $("dec-err");
    err.hidden = true;
    try { await invoke("decision_set_key", { preset: st.preset, key }); } catch (e) { err.textContent = String(e); err.hidden = false; }
    await sync();
  };
  $("dec-preset").onchange = async (e) => {
    const p = st.presets.find((p) => p.id === e.target.value);
    await set("decision_preset", p.id);
    await set("decision_base_url", p.base_url);
    await save("decision_model", "");
  };
  $("dec-wire").onchange = (e) => save("decision_wire", e.target.value);
  $("dec-url").onchange = (e) => save("decision_base_url", e.target.value.trim());
  $("dec-model").onchange = (e) => save("decision_model", e.target.value.trim());
  $("dec-key").onchange = (e) => { const key = e.target.value; e.target.value = ""; setKey(key); };
  $("dec-key-rm").onclick = () => setKey("");
  fill();
}

const fmtTokens = (n) => (n >= 1000 ? (n / 1000).toFixed(1) + "k" : String(n));
const fmtCost = (c) => (c < 0.001 ? "<$0.001" : "$" + c.toFixed(c < 0.1 ? 3 : 2));
const fmtPrice = (p) => "$" + +p.toFixed(4);
// A cost of null is a model with no known price; 0 is free (a model on this Mac). Never a guess.
const costText = (c) => (c == null ? "cost unknown" : c === 0 ? "free" : "≈ " + fmtCost(c));
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
${withImages(st) && st.off_device && !blocked ? `<p class="xs meta">Up to 20 screenshots from the transcript’s live stretches go with the request · ${offImg}</p>` : ""}
${blocked ? `<p class="s" id="why">${esc(blocked)}</p>` : ""}${error ? `<p class="err" role="alert">${esc(error)}</p>` : ""}${busy ? '<p class="s" role="status">Generating…</p>' : ""}
${editing ? `<div class="edit"><label class="xs" for="prompt">Prompt</label><textarea class="field" id="prompt" rows="6">${esc(prompt)}</textarea>
<div class="foot"><input class="field grow" id="tname" placeholder="Template name" aria-label="Template name" value="${picked && !picked.builtin ? esc(picked.name) : ""}"><button class="btn" id="save">Save as template</button></div>
<div class="foot"><button class="btn quiet" id="del"${picked && !picked.builtin ? "" : " hidden"}>Delete template</button><span class="sp"></span><button class="btn quiet" id="cancel">Cancel</button><button class="btn primary" id="run"${busy || blocked ? " disabled" : ""}>Generate</button></div></div>` : ""}
${current ? `<p class="xs meta">Generated ${when} with ${esc(current.model)} via ${esc(current.provider)} · ${fmtTokens(current.tokens_in + current.tokens_out)} tokens${" · " + costText(current.cost)}${current.off_device ? ` · <span class="pill alert">sent text off this Mac</span>` : ""}</p><div class="out">${md(current.text)}</div>` : blocked || busy ? "" : '<p class="s">No summary yet.</p>'}`;

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
      pane.innerHTML = `<div class="hdr"><span><b>${fmtTokens(current.context_tokens)}</b> tokens in context</span>${current.turns.length ? `<span><b>${costText(current.cost)}</b>${current.cost ? " this chat" : ""}</span>` : ""}${st.off_device ? (withImages(st) ? offImg : offMac) : ""}</div>
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

const asked = new Map(); // meeting id -> suggestion request, so a redraw mid-request does not send a second

// The "Suggested: Acme · Accept / Dismiss" chip above the inspector tabs. `accept(folder)` files the meeting.
// Stays hidden unless the provider answers with one of the user's folders; the backend decides when to ask.
async function suggestion(box, m, accept) {
  try {
    const st = await invoke("ai_state");
    if (!st.folders || st.problem || m.folder_id !== null) return;
    if (!asked.has(m.id)) asked.set(m.id, invoke("folder_suggest", { id: m.id }).finally(() => asked.delete(m.id)));
    const f = await asked.get(m.id);
    if (!f || !box.isConnected || box.querySelector(".suggest")) return;
    box.insertAdjacentHTML("afterbegin", `<div class="suggest" role="status"><span class="pill neutral">Suggested folder: ${esc(f.name)}</span><button class="btn quiet" id="sg-yes">Accept</button><button class="btn quiet" id="sg-no">Dismiss</button>${st.off_device ? offMac : ""}</div>`);
    box.querySelector("#sg-yes").onclick = () => accept(f);
    box.querySelector("#sg-no").onclick = async () => { await invoke("folder_suggest_dismiss", { id: m.id }); box.querySelector(".suggest")?.remove(); };
  } catch (e) { /* a failed or unreachable provider just means no chip */ }
}

return { settings, decision, inspector, suggestion };
})();
