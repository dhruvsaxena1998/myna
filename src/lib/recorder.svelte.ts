// The keycap recorder's behaviour, shared by Listen and Meeting notes: one recorder at a time, the old shortcuts
// paused while it listens (so they reach the page), Esc cancels, leaving the window ends it, and a pick is checked
// before it's saved. The button itself is ShortcutButton.svelte.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMount, tick } from "svelte";
import { fromKey } from "./shortcut";

export const PRESS = "Press the key or keys you want to use. Esc cancels.";

type Options<W extends string> = {
  /** Saves the pick; resolves when it's done (or has said why not). */
  save: (which: W, key: string) => Promise<unknown>;
  /** The error for a key that can't be used, or null. */
  check: (which: W, key: string) => string | null;
  /** fn alone is a voice typing key, but can't be a global shortcut. */
  fn: boolean;
};

export class Recorder<W extends string> {
  recording = $state<W | null>(null);
  /** The recorder's own errors; the page writes its save errors here too, so one line shows either. */
  error = $state("");
  #o: Options<W>;

  constructor(o: Options<W>) {
    this.#o = o;
    // The browser never sees fn; the backend tells us when it's let go while recording.
    onMount(() => {
      const off = listen("voice-fn", () => this.recording && this.#pick("Fn"));
      return () => void off.then((f) => f());
    });
  }

  #focus = (which: W | null) => void tick().then(() => document.getElementById(`rec-${which}`)?.focus());

  async record(which: W | null) {
    if (this.recording === which) return;
    const was = this.recording;
    this.recording = which;
    this.error = "";
    await tick();
    document.getElementById(`rec-${which ?? was}`)?.focus();
    // While recording, the old shortcuts must reach this page instead of firing.
    if ((was === null) !== (which === null)) await invoke("voice_pause", { paused: which !== null }).catch((err) => (this.error = String(err)));
  }

  onkeydown = (e: KeyboardEvent) => {
    if (!this.recording) return;
    e.preventDefault();
    if (e.key === "Escape") return void this.record(null);
    const key = fromKey(e);
    if (key === null) return;
    if (typeof key === "object") return void (this.error = key.error);
    this.#pick(key);
  };

  #pick(key: string) {
    const which = this.recording!;
    this.recording = null;
    const err = key === "Fn" && !this.#o.fn ? "fn alone can’t start meeting notes. Add another key with it." : this.#o.check(which, key);
    if (err) {
      this.error = err;
      invoke("voice_pause", { paused: false }).catch(() => {});
      return this.#focus(which);
    }
    this.#o.save(which, key).then(() => this.#focus(which));
  }
}
