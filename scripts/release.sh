#!/usr/bin/env bash
# Build, sign and publish a release from this Mac. See docs/RELEASING.md.
#   git checkout v0.0.2-beta.8 && scripts/release.sh v0.0.2-beta.8
# The tag must be annotated (its message is the release notes) and checked out, with the version files matching.
# Needs: the "Myna Self-Signed" certificate in your login keychain, the updater key at ~/.tauri/myna.key
# (its password comes from TAURI_SIGNING_PRIVATE_KEY_PASSWORD, or is asked for), and `gh auth login`.
set -euo pipefail
cd "$(dirname "$0")/.."
fail() { echo "error: $*" >&2; exit 1; }

TAG=${1:?usage: scripts/release.sh vX.Y.Z[-beta.N]}
V=${TAG#v}
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)

for f in "$(jq -r .version package.json)" \
         "$(jq -r .version src-tauri/tauri.conf.json)" \
         "$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)"; do
  [ "$f" = "$V" ] || fail "tag $TAG but a version file says $f"
done
# The updater compares semver; "beta2" sorts above every "beta.N".
[[ "$V" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$ ]] || fail "use X.Y.Z or X.Y.Z-beta.N, not $V"
[ "$(git cat-file -t "$TAG" 2>/dev/null)" = tag ] || fail "$TAG needs to be an annotated tag; its message is the release notes"
[ "$(git rev-parse HEAD)" = "$(git rev-list -n1 "$TAG")" ] || fail "check out $TAG first (git checkout $TAG)"
[ -z "$(git status --porcelain --untracked-files=no)" ] || fail "uncommitted changes"
git ls-remote --exit-code --tags origin "$TAG" >/dev/null || fail "push $TAG first"
gh release view "$TAG" >/dev/null 2>&1 && fail "release $TAG already exists; delete it first to redo it"

IDENTITY=${MYNA_SIGNING_IDENTITY:-Myna Self-Signed}
security find-identity -v -p codesigning | grep -q "\"$IDENTITY\"" || fail "no code-signing identity named \"$IDENTITY\" in your keychain"
KEY=${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.tauri/myna.key}
[ -f "$KEY" ] || fail "no updater key at $KEY"
if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD+x}" ]; then
  read -rs -p "Updater key password: " TAURI_SIGNING_PRIVATE_KEY_PASSWORD; echo
fi
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD TAURI_SIGNING_PRIVATE_KEY=$(cat "$KEY")
export APPLE_SIGNING_IDENTITY=$IDENTITY

pnpm install --frozen-lockfile
# Updater artifacts only here, so everyday builds don't need the private key. mise supplies cmake when it's there.
RUN=(); command -v mise >/dev/null && RUN=(mise exec cmake@4.4.5 --)
"${RUN[@]}" pnpm tauri build --config '{"bundle":{"createUpdaterArtifacts":true}}'

B=src-tauri/target/release/bundle
APP=$B/macos/Myna.app
codesign --verify --deep --strict "$APP"
codesign -dr - "$APP" 2>&1 | tee /dev/stderr | grep -q 'certificate leaf = H"' || fail "not signed with a certificate"
codesign -d --entitlements - --xml "$APP" | grep -q audio-input || fail "the microphone entitlement is missing"

PRE=; [[ "$TAG" == *-beta.* ]] && PRE=--prerelease
gh release create "$TAG" --verify-tag --notes-from-tag --title "Myna $V" $PRE \
  "$B"/dmg/*.dmg "$B/macos/Myna.app.tar.gz" "$B/macos/Myna.app.tar.gz.sig"

T=$(mktemp -d)
jq -n --arg v "$V" \
      --arg notes "$(gh release view "$TAG" --json body -q .body)" \
      --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
      --arg url "https://github.com/$REPO/releases/download/$TAG/Myna.app.tar.gz" \
      --rawfile sig "$B/macos/Myna.app.tar.gz.sig" \
  '{version:$v, notes:$notes, pub_date:$date, platforms:{"darwin-aarch64":{url:$url, signature:$sig}}}' > "$T/feed.json"
cat "$T/feed.json"
# A pre-release, so GitHub never shows the feeds as the Latest release while every Myna build is a beta.
gh release view updater >/dev/null 2>&1 || \
  gh release create updater --prerelease --latest=false --title "Update feeds" --notes "Feeds the app reads to find updates. Rewritten by scripts/release.sh; don't edit by hand."
# Beta also gets newer stable releases; stable never sees betas.
cp "$T/feed.json" "$T/beta.json"
FILES=("$T/beta.json")
[[ "$TAG" == *-beta.* ]] || { cp "$T/feed.json" "$T/stable.json"; FILES+=("$T/stable.json"); }
gh release upload updater "${FILES[@]}" --clobber
echo "Released $TAG: https://github.com/$REPO/releases/tag/$TAG"
