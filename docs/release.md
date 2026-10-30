# Releasing Myna

A release is a `v*` tag push (or a manual run of the `release` workflow). The workflow builds an arm64 DMG on `macos-14`, signs it with your Developer ID, notarizes it, and publishes a GitHub Release with the DMG, the updater archive (`Myna.app.tar.gz` and `.sig`) and `latest.json`. Installed copies read `https://github.com/dhruvsaxena1998/myna/releases/latest/download/latest.json` from About → Check for updates.

The workflow stops before building if the updater `pubkey` is still the placeholder or a secret is missing. Nothing is signed locally; `npm run tauri build -- --bundles app` still works with no secrets.

## One-time setup

1. **Developer ID certificate.** In [developer.apple.com](https://developer.apple.com/account/resources/certificates) create a *Developer ID Application* certificate (needs the paid Apple Developer Program). Download it and add it to Keychain Access.
2. **Export it as a base64 `.p12`.** In Keychain Access, select the certificate with its private key, File → Export Items, format `.p12`, and set a password. Then:
   ```sh
   base64 -i Certificates.p12 | pbcopy
   ```
3. **Signing identity.** Run `security find-identity -v -p codesigning` and copy the full name, for example `Developer ID Application: Your Name (TEAMID)`.
4. **App-specific password.** At [appleid.apple.com](https://appleid.apple.com) → Sign-In and Security → App-Specific Passwords, create one named `myna-notarize`.
5. **Team ID.** The ten-character ID shown in the identity name above, or under Membership on developer.apple.com.
6. **Updater key pair.**
   ```sh
   npm run tauri signer generate -- -w ~/.tauri/myna.key
   ```
   Pick a password. This prints the public key and writes the private key to `~/.tauri/myna.key`. Keep the private key safe: if you lose it, installed copies can never update.
7. **Public key.** Replace `REPLACE_WITH_UPDATER_PUBKEY` in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) with the contents of `~/.tauri/myna.key.pub`, and commit it. The public key is not secret.
8. **Repository secrets** (Settings → Secrets and variables → Actions → New repository secret):

   | Secret | Value |
   | --- | --- |
   | `APPLE_CERTIFICATE` | the base64 `.p12` from step 2 |
   | `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
   | `APPLE_SIGNING_IDENTITY` | the name from step 3 |
   | `APPLE_ID` | your Apple ID email |
   | `APPLE_PASSWORD` | the app-specific password from step 4 |
   | `APPLE_TEAM_ID` | the team ID from step 5 |
   | `TAURI_SIGNING_PRIVATE_KEY` | the contents of `~/.tauri/myna.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password from step 6 |

## Cutting a release

1. Run the checklist below on a real Mac.
2. Bump `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`, and commit.
3. `git tag v0.1.0 && git push origin v0.1.0`. A manual run from the Actions tab publishes a draft instead.
4. When the run is green, download the DMG on a clean Mac, drag Myna to Applications, and open it. There must be no Gatekeeper warning. Install an older build first to check About → Check for updates.

## Release checklist

These need hardware and permissions that CI does not have. Run them on a Mac on macOS 14.2 or later, using the built `.app` (not `tauri dev`), with Microphone, Accessibility and Screen Recording freshly granted.

- [ ] **Hotkey.** Hold the dictation hotkey, speak, release: the pill shows, the text is pasted. Test both hold and toggle modes, and the mute hotkey in a meeting.
- [ ] **Paste.** Paste lands in a text field in a native app, a browser and a terminal. The clipboard is restored afterwards. Try the keystrokes method too.
- [ ] **Core Audio taps.** Start a meeting with the system audio source while a call or video plays. The system track has audio, and it stops when the meeting ends.
- [ ] **ScreenCaptureKit.** Screenshots are kept during a meeting, window video records when enabled, and the Screen Recording prompt appears once.
- [ ] **Swift bridge.** With Voz selected, dictation works. With Uhm on, fillers are removed. With Who selected, a recorded meeting gets separate speakers. Each model's off-device note shows in Settings.
- [ ] **Extension.** Load `extension/` (Reveal in Meetings settings), pair it, join Google Meet or Teams in Chrome. Mute and unmute show in the popover, and captions give speaker names where available.
- [ ] **Updater.** From the previous release, About → Check for updates finds the new version, installs it, and relaunches into it.
- [ ] **Clean install.** First launch on a Mac that never ran Myna: onboarding, permissions, no Gatekeeper warning.
