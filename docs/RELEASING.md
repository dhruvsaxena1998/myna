# Releasing Myna

Pushing a `v*` tag publishes a release. Nothing else does.

1. Set the same version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, and commit it on `main`. Use `X.Y.Z` for stable or `X.Y.Z-beta.N` for beta. Always put the dot in `beta.N`.
2. Write the release notes in Markdown as the message of an annotated tag:
   ```sh
   git tag -a --cleanup=verbatim v0.0.2-beta.1 -F notes.md   # --cleanup=verbatim keeps the # headings
   git push origin v0.0.2-beta.1
   ```
3. The Release workflow builds and signs the app, then publishes the GitHub release with the DMG and the update files. It also rewrites the feeds on the `updater` release:
   - a beta tag rewrites `beta.json`
   - a stable tag rewrites both `beta.json` and `stable.json`

   The notes become the release description and the "What's new" in the app.

If the run fails, fix the problem, then delete the tag (`git push --delete origin vX` and `git tag -d vX`) and the release if one was made, and push the tag again.

Signing keys and secrets: see issue 14.1 (#75).
