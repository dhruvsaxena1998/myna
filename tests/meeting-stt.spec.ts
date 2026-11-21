import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// Meeting speech-to-text (#45): Local model by default; an Endpoint only once its model passes Test.
const m = { id: "a", name: "Model a", vendor: "V", desc: "d", acc: 4, spd: 4, default: true, size: 1, min_macos: 0, repo: "r", revision: "x", files: [{}], system: false, runtime: false, installed: true, needs: null };
const OR = { id: 1, kind: "openrouter", name: "OpenRouter", url: "https://openrouter.ai/api/v1", has_key: true, off_device: true };
const LOCAL = { id: 2, kind: "openai", name: "Whisper server", url: "http://localhost:8000/v1", has_key: false, off_device: false };

async function open(page: Page, stt: unknown = { kind: "local" }, providers: unknown[] = [OR, LOCAL], extra = "") {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna",
    meeting_get: () => ({ on: true, key: "Ctrl+Alt+KeyM", keep_teams: false }),
    models_list: () => [{ id: "stt", active: "a", models: [${JSON.stringify(m)}] }, { id: "diar", active: null, models: [] }, { id: "meeting_stt", active: "a", models: [${JSON.stringify(m)}] }],
    ai_get: () => ({ providers: ${JSON.stringify(providers)}, main: null, fast: null }),
    ai_models: () => [],
    meeting_stt_get: () => (window.__stt ??= ${JSON.stringify(stt)}),
    meeting_stt_local: () => { window.__stt = { kind: "local" }; },
    meeting_stt_models: ({ id }) => id === 1 ? [{ id: "openai/whisper-large-v3" }, { id: "openai/gpt-4o-transcribe" }] : [],
    meeting_stt_test: ({ provider, model }) => {
      if (model === "openai/gpt-4o-transcribe") throw "openai/gpt-4o-transcribe can't transcribe meetings: Couldn't reach https://openrouter.ai/api/v1: response_format 'verbose_json' is not compatible with this model";
      window.__stt = { kind: "endpoint", provider, model };
      return { ms: 1234, segments: 2, cost: { kind: "usd", usd: 0.0002 } };
    },
    ${extra}
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Meeting notes" }).click();
}
const via = (page: Page) => page.getByRole("combobox", { name: "Transcribe with" });
const row = (page: Page) => page.locator(".row", { has: via(page) });
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("the Local model is the default and says nothing leaves the Mac", async ({ page }) => {
  await open(page);
  await expect(via(page)).toHaveValue("local");
  await expect(row(page)).toContainText("Nothing leaves this Mac.");
  await expect(row(page)).toContainText("Stays on this Mac");
  await expect(page.getByRole("button", { name: "Test" })).toHaveCount(0);
  expect(await calls(page, "meeting_stt_test")).toEqual([]);
});

test("an OpenRouter model is picked only once it passes Test, and the setting then says audio leaves the Mac", async ({ page }) => {
  await open(page);
  await via(page).selectOption("endpoint");
  await expect(row(page)).toContainText("Still the Local model until a model passes Test.");
  const model = page.getByRole("combobox", { name: "Transcription model" });
  await model.selectOption("openai/gpt-4o-transcribe");
  await page.getByRole("button", { name: "Test" }).click();
  await expect(page.getByText("verbose_json' is not compatible with this model")).toBeVisible();
  await expect(row(page)).toContainText("Still the Local model");
  await model.selectOption("openai/whisper-large-v3");
  await page.getByRole("button", { name: "Test" }).click();
  await expect(page.getByText("Works · 2 segments · 1.2 s · $0.00020. New meetings use it.")).toBeVisible();
  await expect(row(page)).toContainText("openai/whisper-large-v3 · OpenRouter, after the meeting.");
  await expect(row(page)).toContainText("Sends audio off this Mac");
  expect(await calls(page, "meeting_stt_test")).toEqual([{ provider: 1, model: "openai/gpt-4o-transcribe" }, { provider: 1, model: "openai/whisper-large-v3" }]);
});

test("another provider's model id is typed in", async ({ page }) => {
  await open(page);
  await via(page).selectOption("endpoint");
  await page.getByRole("combobox", { name: "Provider" }).selectOption("2");
  await page.getByRole("textbox", { name: "Transcription model" }).fill("whisper-1");
  await page.getByRole("button", { name: "Test" }).click();
  await expect(row(page)).toContainText("whisper-1 · Whisper server");
  await expect(row(page)).toContainText("Stays on this Mac");
});

test("back to Local model saves at once", async ({ page }) => {
  await open(page, { kind: "endpoint", provider: 1, model: "openai/whisper-large-v3" });
  await expect(via(page)).toHaveValue("endpoint");
  await via(page).selectOption("local");
  await expect(row(page)).toContainText("Nothing leaves this Mac.");
  expect((await calls(page, "meeting_stt_local")).length).toBe(1);
});

test("a removed provider shows Not set", async ({ page }) => {
  await open(page, { kind: "not_set" }, [LOCAL]);
  await expect(row(page)).toContainText("Not set: its provider was removed. New meetings wait");
});

test("Models: Transcribe with comes first, then the Local model or the Endpoint, never both", async ({ page }) => {
  const names = () => page.getByRole("region", { name: "Models" }).locator(".row .name");
  await open(page);
  await expect(names()).toHaveText(["Transcribe with", "Speech-to-text model", "Speaker diarization"]);
  await via(page).selectOption("endpoint");
  await expect(names()).toHaveText(["Transcribe with", "Endpoint", "Speaker diarization"]);
});

test("a saved Endpoint names the Local model that keeps muted stretches on this Mac", async ({ page }) => {
  await open(page, { kind: "endpoint", provider: 1, model: "openai/whisper-large-v3" });
  await expect(row(page)).toContainText("What you say while muted stays on this Mac, with Model a.");
});

test("an Agent isn't offered: Claude Code and Codex take no audio", async ({ page }) => {
  const CLAUDE = { id: 3, kind: "claude-code", name: "Claude Code", url: "", has_key: false, off_device: true };
  await open(page, { kind: "local" }, [CLAUDE, LOCAL]);
  await via(page).selectOption("endpoint");
  await expect(page.getByRole("combobox", { name: "Provider" }).locator("option")).toHaveText(["Whisper server"]);
});

test("while OpenRouter's list loads, its model menu is already there, disabled, so nothing swaps in later", async ({ page }) => {
  await open(page, { kind: "local" }, [OR, LOCAL], `meeting_stt_models: () => new Promise(() => {}),`);
  await via(page).selectOption("endpoint");
  const model = page.getByRole("combobox", { name: "Transcription model" });
  await expect(model).toBeDisabled();
  await expect(model).toContainText("Loading models…");
  await expect(page.getByRole("textbox", { name: "Transcription model" })).toHaveCount(0);
});
