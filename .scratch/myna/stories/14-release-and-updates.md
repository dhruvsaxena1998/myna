# 14: Release and auto-update

**Status:** ready
**GitHub:** milestone "14 · Release and auto-update"
**Spec stories:** 4
**Depends on:** 01

Cut a release by pushing a tag; installed copies of Myna find it, download it and update themselves.

> Planned in the [auto-update map](../../auto-update/map.md). Facts and sources: [updater and signing facts](../../auto-update/research/02-updater-facts.md).

## Stories
- As a user, I want to install from a DMG and have Myna keep itself up to date, so that I never re-download DMGs.
- As a user, I want to choose how much Myna does on its own about updates, and whether I get betas.
- As a user, I want to see what's new before I restart.

## Done when
- [ ] Pushing a `v*` tag builds, signs and publishes a release; nothing else does
- [ ] An installed beta finds, downloads and installs the next beta, and Microphone and Accessibility stay allowed
- [ ] About, the update pop-up, the notification and the menubar item match `docs/index.html`
- [ ] A teammate installs on a clean Mac

## Decisions
- Releases come from GitHub Releases of the public repo, cut only by pushing a `v*` tag from your machine. `-beta.N` tags are the beta Channel, plain tags stable.
- Feeds live on one fixed release, tag `updater`: `stable.json` and `beta.json`, rewritten by the workflow on every release. Beta also gets newer stable releases; stable never sees betas. The feed URL is picked at runtime in Rust (`UpdaterBuilder::endpoints`), never from JS.
- Versions are semver with a dotted pre-release (`0.0.2-beta.1`). `0.0.1-beta2` sorts above every `beta.N`, so the first updater build is `0.0.2-beta.1` or later. Never downgrade, including when switching beta → stable.
- Signing: a self-signed code-signing certificate (10-year validity, same `.p12` forever) so TCC grants survive updates. The workflow imports it itself; Tauri's `APPLE_CERTIFICATE` import rejects self-signed certs. A paid Developer ID comes later and resets permissions once.
- Update mode: Check and download (default), Check only, Off. Automatic modes check at launch and every 24 h. Off checks only from the button.
- New installs start on their build's Channel; switchable in About.
- Notify = one macOS notification per version (permission asked the first time there is one), plus a lasting menubar item and the About row. The pop-up opens from the button, the menubar item or the notification, never on its own in the automatic modes.
- The pop-up shows the newest release's notes only, written as the annotated tag's message (Markdown, `git tag -a --cleanup=verbatim`), which becomes the release body and the feed's `notes`. Buttons: Later, Download and restart, Restart now, OK, Try again, Close. No Skip.
- Installing happens on Restart now, never at quit (not built into the updater), and never while dictating or recording.
- Automatic modes fail quietly and retry at the next check; a manual check shows the error; a bad signature is always shown.

## Tickets
1. Updater key and signing certificate (human): create both, store in the password manager and GitHub secrets, record the public key
2. Release workflow: `v*` tag → build → sign with the self-signed cert → GitHub Release with updater artifacts → rewrite `stable.json` / `beta.json` on the `updater` release; version moves to dotted pre-releases
3. Updater core and About: `tauri-plugin-updater`, Channel feed chosen in Rust, Update mode and Channel saved, launch + 24 h checks, download then install on restart, never while busy; About's Version row, Update mode and Channel rows
4. Update pop-up: its own small window with What's new (rendered Markdown) and every button state
5. Notification and menubar item: once per version, permission on first use, both open the pop-up
6. Fresh-install and update check (human): install a beta on a clean Mac, publish the next beta, update in place, permissions kept
