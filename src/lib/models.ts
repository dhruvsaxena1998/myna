// Mirrors src-tauri/src/models.rs.
export type Model = {
  id: string; name: string; vendor: string; desc: string;
  acc: number; spd: number; default: boolean; size: number; min_macos: number;
  repo: string; revision: string; files: unknown[]; system: boolean; sdk?: boolean; runtime: boolean; installed: boolean; needs: string | null;
  /** Added under Advanced: its runtime (one of ENGINES), and its file or folder when it's kept on this Mac. */
  engine?: string; path?: string;
};
/** The runtimes a model added under Advanced can run on, per list (models.rs ENGINES). */
export const ENGINES: Record<"stt" | "diar", [string, string][]> = {
  stt: [["whisper", "Whisper (ggml .bin)"], ["parakeet-tdt", "Parakeet TDT (ONNX)"], ["parakeet-unified", "Parakeet Unified (ONNX)"]],
  diar: [["sortformer", "Sortformer (ONNX)"]],
};
/** `meeting_stt`: meeting notes' own pick from the speech-to-text catalog; voice typing's until one is saved. */
export type Task = { id: "stt" | "diar" | "meeting_stt"; models: Model[]; active: string | null };

// Decimal units, as Finder shows them.
export const fmtSize = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`);
