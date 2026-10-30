// A mocked Tauri backend for dev/index.html: the commands the Meetings window and the AI tabs call,
// answered from memory. Nothing here talks to the network or the disk. State resets on reload.
// Query switches: ?window=N sets the model's context window, ?noprice blanks the prices, ?local is a local
// provider (no off-device pill), ?off turns chat off, ?unready has no API key, ?slow makes answers take 3 s,
// ?auto shows the muted line as included by triage, ?preset=jev|openai|custom|laya picks the decision preset, ?laya=none|installed|running|error is Laya's state,
// ?tab=chat opens that inspector tab, ?empty starts with no chat turns, ?nomedia has no audio, video or screenshots on disk. A question containing "fail" gets an error.
(() => {
  const q = new URLSearchParams(location.search);
  const ai = {
    presets: [{ id: "openrouter", name: "OpenRouter", base_url: "https://openrouter.ai/api/v1", model: "openai/gpt-5-mini" }, { id: "custom", name: "Custom (OpenAI-compatible)", base_url: "http://localhost:11434/v1", model: "llama3.1" }],
    preset: q.has("local") ? "custom" : "openrouter", base_url: "https://openrouter.ai/api/v1", model: "gpt-5-mini", has_key: !q.has("unready"),
    input_price: q.has("noprice") ? null : 0.25, output_price: q.has("noprice") ? null : 2, context: q.has("window") ? Number(q.get("window")) : 400000,
    summaries: true, folders: false, chat: !q.has("off"), problem: q.has("unready") ? "Add an API key under AI provider in Settings." : null, off_device: !q.has("local"),
  };
  const dpresets = [{ id: "laya", name: "Laya", base_url: "", wire: "jev", model: "" }, { id: "jev", name: "Jev", base_url: "https://api.jev.ai/v1", wire: "jev", model: "" }, { id: "openai", name: "OpenAI Decisions", base_url: "https://api.openai.com/v1", wire: "openai", model: "gpt-5-mini" }, { id: "custom", name: "Custom", base_url: "http://localhost:8000/v1", wire: "jev", model: "" }];
  const laya = q.get("laya") || "none";
  const dec = { preset: q.get("preset") || "laya", wire: "jev", base_url: "", model: "", has_key: false, triage: false, laya: { installed: laya === "installed" || laya === "running", step: null, error: laya === "error" ? "Laya's setup needs uv. Install it (brew install uv), then try again." : null, running: laya === "running", dir: "~/Library/Application Support/Myna/laya" } };
  const decState = () => {
    const p = dpresets.find((p) => p.id === dec.preset);
    const problem = dec.preset === "laya" ? (dec.laya.installed ? null : "Set up Laya under Decision provider in Settings.") : dec.has_key || dec.preset === "custom" ? null : "Add an API key under Decision provider in Settings.";
    return { presets: dpresets, ...dec, base_url: dec.base_url || p.base_url, problem, off_device: dec.preset === "laya" ? false : dec.preset !== "custom" || !/localhost|127\.0\.0\.1/.test(dec.base_url || p.base_url) };
  };
  const meeting = { id: 1, title: "Design review · Meetings module", started_at: Math.floor(Date.now() / 1000) - 7200, duration_ms: 42 * 60000, app: "Google Meet · Chrome", note: null, status: "done", progress: 1, error: null, segments: 6, has_them: true, folder_id: null };
  const line = (speaker, start_ms, text, extra = {}) => ({ id: start_ms + 1, speaker, start_ms, end_ms: start_ms + 8000, text, mute: "unmuted", included: false, ...extra });
  const segments = [
    line("them", 71000, "Then it's unknown, which we treat the same as muted. The popover says \"manual\" so you know."),
    line("me", 758000, "Okay so the thing I keep coming back to is the mute case. If I'm muted in Meet and the app keeps recording, where does that speech go?"),
    line("them", 772000, "Collapsed in the transcript and never sent to a provider unless you include it."),
    line("me", 790000, "Fine. Tom, can you measure the screenshot hash threshold on three recorded calls?"),
    line("them", 802000, "Yes, I'll take that."),
    line("me", 900000, "Sorry, I was on mute. The budget number was forty thousand.", { mute: "muted", included: q.has("auto"), auto: q.has("auto") }),
  ];
  const summary = { text: "## Summary\nThree-person review of the Meetings module.\n## Decisions\n- Muted and unknown segments are collapsed, never sent to a provider unless included.\n## Action items\n- Tom: measure the screenshot hash threshold on 3 recorded calls.", prompt: "Summarize this meeting.", model: "gpt-5-mini", provider: "OpenRouter", tokens_in: 9000, tokens_out: 120, cost: 0.002, off_device: true, generated_at: Math.floor(Date.now() / 1000) };
  const templates = [{ id: 1, name: "Summary", prompt: "Summarize this meeting.", builtin: true }];
  const turns = [];
  if (!q.has("empty")) {
    turns.push({ question: "What did Tom commit to?", answer: "Tom took one action item: measure the screenshot hash threshold on three recorded calls.", model: ai.model, tokens_in: 3200, tokens_out: 40 });
    turns.push({ question: "Did anyone object to the extension approach?", answer: "No. Priya asked how apps without readable mute state are handled, and accepted \"unknown is treated as muted\".\n\n- Unknown counts as muted\n- Meet ships first", model: ai.model, tokens_in: 3300, tokens_out: 60 });
  }

  // Same arithmetic as the Rust side: characters ÷ 4, cost from the entered prices, warning past the window.
  const usage = () => {
    const chars = segments.filter((s) => s.mute === "unmuted" || s.included).reduce((n, s) => n + s.text.length, 0) + summary.text.length + turns.reduce((n, t) => n + t.question.length + t.answer.length, 0);
    const context_tokens = Math.floor(chars / 4);
    const spent = turns.reduce((n, t) => n + t.tokens_in * (ai.input_price ?? NaN) + t.tokens_out * (ai.output_price ?? NaN), 0) / 1e6;
    return { turns: [...turns], context_tokens, cost: Number.isNaN(spent) ? null : spent, window: ai.context, over_window: ai.context != null && context_tokens > ai.context };
  };
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  let media_bytes = q.has("nomedia") ? 0 : 412e6; // ?nomedia is a meeting whose audio, video and screenshots are gone
  const commands = {
    list_meetings: () => [meeting],
    list_models: () => ({ models: [{ id: "uhm", size: 47e6, status: "missing", downloading: null, error: null, unavailable: null }] }),
    get_setting: ({ key }) => (key === "ai_triage" ? String(dec.triage) : null),
    list_folders: () => [{ id: 1, name: "Acme" }],
    get_meeting: () => ({ meeting, segments, shots: [], shots_taken: 0, audio: [], video: null, media_bytes }),
    delete_media: () => { media_bytes = 0; },
    video_estimate: () => "about 400 MB for an hour",
    ai_state: () => ai,
    template_list: () => templates,
    summary_get: () => summary,
    chat_get: () => usage(),
    chat_send: async ({ question }) => {
      await wait(q.has("slow") ? 3000 : 600);
      if (/fail/i.test(question)) throw "The provider answered 500: overloaded";
      turns.push({ question, answer: `You asked: “${question}”. In this mock the answer is canned.`, model: ai.model, tokens_in: 3400, tokens_out: 30 });
      return usage();
    },
    decision_state: () => decState(),
    decision_set_key: ({ key }) => { dec.has_key = !!key.trim(); },
    laya_setup: () => { dec.laya.error = null; dec.laya.step = "Creating the environment"; setTimeout(() => { dec.laya.step = "Installing Laya"; }, 1200); setTimeout(() => { dec.laya.step = null; dec.laya.installed = true; }, 2400); },
    laya_stop: () => { dec.laya.running = false; },
    set_setting: ({ key, value }) => {
      if (key === "decision_preset") dec.preset = value;
      if (key === "decision_wire") dec.wire = value;
      if (key === "decision_base_url") dec.base_url = value;
      if (key === "decision_model") dec.model = value;
      if (key === "ai_triage") dec.triage = value === "true"; if (key === "ai_chat") ai.chat = value === "true"; if (key === "ai_summaries") ai.summaries = value === "true"; },
  };
  // The page is written in after this script runs, so wait for the tab to exist.
  const tab = q.get("tab");
  if (tab) { const t = setInterval(() => { const b = document.getElementById("tab-" + tab); if (b) { b.click(); clearInterval(t); } }, 50); }
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => (commands[cmd] ? commands[cmd](args) : null) },
    event: { listen: async () => () => {} },
  };
})();
