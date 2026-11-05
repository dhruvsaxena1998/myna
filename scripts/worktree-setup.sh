#!/bin/zsh
# Make a fresh git worktree build and test like the main checkout. Safe to re-run.
set -eu
cd "$(git rev-parse --show-toplevel)"
main=$(cd "$(git rev-parse --git-common-dir)/.." && pwd)
[ "$main" != "$PWD" ] || { echo "this is the main checkout, nothing to do"; exit 0; }

# Ignored local files only; cp -n never overwrites, so tracked files stay as they are.
# .secrets (signing keys) is left out: only releases need it, run those from the main checkout.
n=0
while IFS= read -r f; do
  [ -e "$f" ] && continue
  mkdir -p "$(dirname "$f")" && cp "$main/$f" "$f" && n=$((n+1))
done < <(git -C "$main" -c core.quotepath=off ls-files --others --ignored --exclude-standard -- .env.local .impeccable .scratch)
echo "local files: copied $n"

# whisper-rs-sys needs cmake; mise has no global version, so pin the installed one here.
if [ ! -f mise.local.toml ]; then
  v=$(mise ls --installed --json cmake 2>/dev/null | grep -o '"version": *"[^"]*"' | tail -1 | sed 's/.*"\([^"]*\)"$/\1/')
  if [ -n "$v" ]; then printf '[tools]\ncmake = "%s"\n' "$v" > mise.local.toml; mise trust -q mise.local.toml; echo "cmake: pinned $v"
  else echo "cmake: not installed in mise, run: mise install cmake"; fi
else echo "cmake: already pinned"; fi

if [ -d node_modules ]; then echo "node_modules: present"; else pnpm install --frozen-lockfile --silent && echo "node_modules: installed"; fi
