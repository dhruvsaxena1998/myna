# Myna

Myna is a macOS menu bar app for voice typing and, later, meeting notes. Speech-to-text runs on your Mac, and Myna runs no AI model of its own: text cleanup uses an AI provider you add.

The app is in beta. Voice typing works today. Meeting notes are planned and not built.

## What works today

- Voice typing with a global hotkey, in tap or hold mode. The fn key can be the hotkey.
- A floating pill that shows when Myna is listening, working, done or has failed.
- Paste or type: Myna pastes the text and restores your clipboard, or types it letter by letter for apps that block pasting.
- Cleanup by rules: removes um, uh and repeated words, writes numbers as digits, and spells your "Words to remember" the way you want.
- Dictate with cleanup: a second shortcut sends the text through your AI provider's Fast model first. Cleanup styles set the instructions per app. If the provider is slow or fails, Myna pastes the rules result.
- AI providers: add Endpoints, choose a Main and a Fast model, and Test an Endpoint.
- Voice typing history, and Stats for nerds (experimental, in Settings › Experimental features): where the time went in each dictation and what Myna changed.
- Speech models: Parakeet TDT v3, Whisper large-v3-turbo, and Apple Speech (needs macOS 26 or later).
- Look & appearance: light or dark, accent colour, see-through panels. Liquid Glass (experimental, macOS 26 or later) uses macOS's own glass.
- An in-app updater with a stable or beta channel.

## Planned

Not built yet. The stories are in [`.scratch/myna/stories/`](.scratch/myna/stories/README.md).

- Meeting recording and notes, including mute-aware capture
- Speakers and transcripts
- Browser extension for Google Meet and Teams
- Screenshots and window video
- Folders
- Summary and transcript cleanup
- Chat

## Install

Apple Silicon Macs on macOS 14.2 or later.

1. Download the DMG from the [latest release](https://github.com/dhruvsaxena1998/myna/releases/latest) and drag Myna.app to Applications. Every release so far is marked pre-release, so if that link shows nothing, open the [releases page](https://github.com/dhruvsaxena1998/myna/releases).
2. The build is self-signed, not notarized. The first time, right-click Myna.app and choose Open.
3. Grant Microphone and Accessibility access when asked. Microphone is for listening; Accessibility is for the hotkey and for pasting or typing into other apps.

Some features above may be on `main` and not in the latest release yet. Dictate with cleanup is one of them at the time of writing.

## Develop

You need:

- macOS 14.2 or later, with Xcode (the app has a Swift part)
- [Rust](https://rustup.rs) (edition 2024, so a recent stable toolchain)
- Node.js and [pnpm](https://pnpm.io)
- [mise](https://mise.jdx.dev), to supply CMake 4.4.5, which the Whisper build needs: `mise install cmake@4.4.5`

```sh
pnpm install
mise exec cmake@4.4.5 -- pnpm tauri dev      # run the app; port 1420 must be free
pnpm run check                               # type-check the Svelte and TypeScript code
pnpm run test:ui                             # UI tests in headless WebKit, Tauri IPC faked
(cd src-tauri && mise exec cmake@4.4.5 -- cargo test)   # Rust tests
```

- `pnpm tauri dev` starts the Vite dev server on port 1420 and opens the app in the menu bar.
- `pnpm run test:ui` runs Playwright against its own Vite server on port 1430. Install the browser once with `pnpm exec playwright install webkit`.
- `cargo test` skips the tests that need downloaded speech models (they are marked `#[ignore]`).

In a git worktree, run `scripts/worktree-setup.sh` first. See [AGENTS.md](AGENTS.md).

## Repo map

```
src/          SvelteKit UI: Settings, the main window, the pill and the update window
src-tauri/    Rust and Swift: hotkey, audio, speech models, paste, cleanup, updater
docs/         docs/index.html is the clickable design prototype; flow.html, RELEASING.md, ADRs
design/       design tokens (tokens.css), a specimen page, a contrast check
tests/        Playwright UI tests
scripts/      release.sh and worktree-setup.sh
models/       the speech and AI model catalogs
.scratch/     the spec and stories (.scratch/myna/)
```

Terms are in [GLOSSARY.md](GLOSSARY.md). The design rules are in [DESIGN.md](DESIGN.md). Open [docs/index.html](docs/index.html) in a browser to click through the screens.

## Releasing

Releases are built on the maintainer's Mac with `scripts/release.sh`, run from an annotated version tag. There is no CI release workflow. Steps are in [docs/RELEASING.md](docs/RELEASING.md).
