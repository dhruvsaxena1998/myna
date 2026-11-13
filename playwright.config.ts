import { defineConfig } from "@playwright/test";

// Svelte UI in headless WebKit (closest to the app's WKWebView), Tauri IPC faked per test.
export default defineConfig({
  testDir: "tests",
  use: { baseURL: "http://127.0.0.1:1430", browserName: "webkit", colorScheme: "dark", reducedMotion: "reduce" },
  webServer: { command: "node_modules/.bin/vite dev --port 1430", url: "http://127.0.0.1:1430/settings", reuseExistingServer: false },
});
