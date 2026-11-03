// Mirrors src-tauri/src/models.rs.
export type Model = {
  id: string; name: string; vendor: string; desc: string;
  acc: number; spd: number; default: boolean; size: number; min_macos: number;
  repo: string; revision: string; files: unknown[]; system: boolean; installed: boolean; needs: string | null;
};
export type Task = { id: "stt" | "diar"; models: Model[]; active: string | null };

// Decimal units, as Finder shows them.
export const fmtSize = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`);
