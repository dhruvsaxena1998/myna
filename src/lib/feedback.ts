/** Feedback on a dictation and the fixes Myna suggests from it (src-tauri/src/feedback.rs). */
import { invoke } from "@tauri-apps/api/core";

export type Rating = "up" | "down" | null;
export type Fix = { key: string; kind: "alias" | "word" | "keep"; from: string | null; to: string; count: number };

export const rate = (trace: number, rating: Rating, corrected: string | null) => invoke<Fix[]>("feedback_set", { trace, rating, corrected });
export const fixText = (f: Fix) => (f.kind === "keep" ? `Never remove “${f.to}”` : `Add “${f.kind === "alias" ? `${f.from} → ${f.to}` : f.to}” to Words to remember`);
