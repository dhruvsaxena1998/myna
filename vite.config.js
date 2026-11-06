import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
// @ts-expect-error type error without @types/node package
import { execSync } from "node:child_process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(({ command }) => ({
  plugins: [sveltekit()],
  // Dev runs label their version with the commit, so About shows which code is running.
  define: { "import.meta.env.VITE_BUILD": JSON.stringify(command === "serve" ? devBuild() : "") },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));

function devBuild() {
  /** @param {string} args */
  const git = (args) => execSync(`git ${args}`, { encoding: "utf8" }).trim();
  try {
    return `dev · ${git("rev-parse --abbrev-ref HEAD")} @ ${git("rev-parse --short HEAD")}${git("status --porcelain -uno") ? " + changes" : ""}`;
  } catch {
    return "dev";
  }
}
