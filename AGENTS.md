# AGENTS.md

## Agent skills

### Issue tracker

Issues live as local markdown under `.scratch/` (gitignored). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` + `docs/adr/`. See `docs/agents/domain.md`.

### Worktrees

After creating a git worktree, run `scripts/worktree-setup.sh` in it: copies the gitignored local files (`.env.local`, `.scratch/`, `.impeccable/`), pins cmake for mise, runs `pnpm install`.

### Compatibility: a user's data outlives the build

An older Myna must keep working on data a newer Myna wrote (an update, a dev build on the same folder, a user going back a version). Never ship a change that locks a build out or breaks it. When you add or change anything Myna stores, keep these:

- **Database (`src-tauri/src/storage.rs`):** migrations only add: new tables, new columns that are nullable or have a `DEFAULT`. Never drop, rename, retype or rewrite what an older build reads or writes. The `migrations_only_add` test enforces the common cases; don't edit it to get a change through. To retire something, stop using it in one release and remove it a release or two later, if ever. `open` still opens a database with a higher `user_version`.
- **Settings keys and JSON blobs (the `setting` table, trace step `settings`, styles):** add keys and fields, never rename or reuse one for something else. A new field gets `#[serde(default)]`, so data written before it still loads. Don't use `deny_unknown_fields`.
- **Other names a build looks up:** the Keychain service and account names, the models folder layout and the data folder path stay as they are.
- **The update feed (`beta.json`, `stable.json`):** add fields only; an installed app reads the ones it knows.

Dev builds (`pnpm tauri dev`) use their own data folder, `~/Library/Application Support/Myna Dev`, copied once from `Myna` on first launch, so trying a migration never touches the installed app's data. Models stay shared in `Myna/models`.

Builds before the one that added this rule (up to 0.0.2-beta.7) still refuse a newer database, so a user on one of them who opens a newer data file sees "written by a newer Myna". Updating fixes it.
