// Mirrors src-tauri/src/models.rs.
export type Model = {
  id: string; name: string; vendor: string; desc: string;
  acc: number; spd: number; default: boolean; size: number; min_macos: number;
  repo: string; revision: string; files: unknown[]; system: boolean; sdk?: boolean; runtime: boolean; installed: boolean; needs: string | null;
};
/** `meeting_stt`: meeting notes' own pick from the speech-to-text catalog; voice typing's until one is saved. */
export type Task = { id: "stt" | "diar" | "meeting_stt"; models: Model[]; active: string | null };

// Decimal units, as Finder shows them.
export const fmtSize = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`);
