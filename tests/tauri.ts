import type { Page } from "@playwright/test";

/** Fake the Tauri backend: `handlers[cmd](args)` answers `invoke(cmd, args)`; a thrown string becomes the rejection. */
export async function fakeTauri(page: Page, handlers: string) {
  await page.addInitScript((src) => {
    const h = new Function(`return (${src})`)();
    const cbs: Record<number, Function> = {};
    const listeners: Record<string, number[]> = {}; // one per component listening
    // Lets a test play the backend's events: `window.__emit("model-state", {...})`.
    (window as any).__emit = (event: string, payload: unknown) => (listeners[event] ?? []).forEach((id) => cbs[id]({ event, id: 0, payload }));
    const calls: [string, unknown][] = ((window as any).__calls = []);
    (window as any).__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "settings" }, currentWebview: { label: "settings" } },
      transformCallback: (cb: Function) => { const id = Object.keys(cbs).length + 1; cbs[id] = cb; return id; },
      invoke: async (cmd: string, args: unknown) => {
        calls.push([cmd, args]);
        if (cmd === "plugin:app|version") return "0.1.0";
        if (cmd === "plugin:event|listen") return void (listeners[(args as any).event] ??= []).push((args as any).handler);
        if (cmd === "plugin:event|unlisten") return;
        if (cmd === "plugin:window|hide") return;
        if (cmd === "permissions_state" && !(cmd in h)) return { mic: "allowed", ax: true };
        if (cmd === "stt_unload_get" && !(cmd in h)) return 300;
        if (cmd === "pill_look_get" && !(cmd in h)) return { style: "capsule", place: "bottom" };
        if (cmd === "look_get" && !(cmd in h)) return { theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: false };
        if (cmd === "glass_set" && !(cmd in h)) return;
        if (cmd === "globe_ok" && !(cmd in h)) return true;
        if (cmd === "voice_get" && !(cmd in h)) return { on: true, key: "Alt+Space", mode: "hold", paste: "clipboard" };
        if (cmd === "rules_get" && !(cmd in h)) return { settings: { language: "en", tidy: true, words: [] }, languages: [["en", "English"], ["de", "German"]] };
        if (cmd === "rules_set" && !(cmd in h)) return;
        if (cmd === "traces_list" && !(cmd in h)) return [];
        if (cmd === "updates_get" && !(cmd in h)) return { mode: "auto", channel: "beta" };
        if (cmd === "update_state" && !(cmd in h)) return { state: "idle" };
        if (!(cmd in h)) throw `no fake for ${cmd}`;
        return h[cmd](args);
      },
    };
  }, handlers);
}
