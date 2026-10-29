# AGENTS.md

## Agent skills

### Issue tracker

Issues live as local markdown under `.scratch/` (gitignored). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` + `docs/adr/`. See `docs/agents/domain.md`.

## Before opening a PR

CI only runs `cargo test`. Build the app locally before you push:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --bundles app
```
