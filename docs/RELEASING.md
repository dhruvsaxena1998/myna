# Releasing Myna

A release is built, signed and published from the developer's Mac with `scripts/release.sh`. Pushing a tag alone does nothing.

1. Set the same version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, and commit it on `main`. Use `X.Y.Z` for stable or `X.Y.Z-beta.N` for beta. Always put the dot in `beta.N`.
2. Write the release notes in Markdown as the message of an annotated tag, push it, and check it out:
   ```sh
   git tag -a --cleanup=verbatim v0.0.2-beta.1 -F notes.md   # --cleanup=verbatim keeps the # headings
   git push origin v0.0.2-beta.1
   git checkout v0.0.2-beta.1
   ```
3. Run the script from the tag: `scripts/release.sh v0.0.2-beta.1`. It checks the tag, builds and signs the app, checks the signature, then publishes the GitHub release with the DMG and the update files. It also rewrites the feeds on the `updater` release:
   - a beta tag rewrites `beta.json`
   - a stable tag rewrites both `beta.json` and `stable.json`

   The notes become the release description and the "What's new" in the app. A build takes about 10 minutes the first time and 4 to 6 after.

It needs the `Myna Self-Signed` certificate in your login keychain, the updater key at `~/.tauri/myna.key` (the script asks for its password, or reads `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), and `gh auth login`.

If the run fails, fix the problem, then delete the tag (`git push --delete origin vX` and `git tag -d vX`) and the release if one was made, and push the tag again.

The Release workflow in `.github/workflows/release.yml` is a manual fallback (`macos-15`, run from the Actions tab with the tag as the ref).

Signing keys and secrets: see issue 14.1 (#75).
