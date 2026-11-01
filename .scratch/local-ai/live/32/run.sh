#!/bin/bash
# Usage: run.sh <claude|codex>. Sends test.png under ticket 27's Seatbelt profile, no tools, no MCP.
set -u
CLI=$1; HERE=$(cd "$(dirname "$0")" && pwd); RUN=/private/tmp/m32/$CLI; H=$HOME
rm -rf "$RUN"; mkdir -p "$RUN/tmp"; cp "$HERE/test.png" "$RUN/"
PROF=$RUN/profile.sb; cp "$HERE/../27/profile.sb" "$PROF"
allow_read() { for p; do for q in "$p" "$(realpath "$p" 2>/dev/null)"; do [ -n "$q" ] && echo "(allow file-read-data (subpath \"$q\"))" >> "$PROF"; done; done; }
allow_write() { for p; do for q in "$p" "$(realpath "$p" 2>/dev/null)"; do [ -n "$q" ] && echo "(allow file-write* (subpath \"$q\"))" >> "$PROF"; done; done; }
SB=(sandbox-exec -f "$PROF" -D HOME="$H" -D RUN="$RUN")
Q="What text is in this image?"
cd "$RUN"
if [ $CLI = claude ]; then
  allow_read "$H/.local/bin" "$H/.local/share/claude" "$H/.claude" "$H/.claude.json" "$H/Library/Keychains"
  python3 -I -c 'import base64,json,sys;print(json.dumps({"type":"user","message":{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":base64.b64encode(open("test.png","rb").read()).decode()}},{"type":"text","text":sys.argv[1]}]}}))' "$Q" > in.jsonl
  TMPDIR=$RUN/tmp "${SB[@]}" env ENABLE_CLAUDEAI_MCP_SERVERS=false claude -p --input-format stream-json --output-format stream-json --verbose --model haiku \
    --mcp-config '{"mcpServers":{}}' --strict-mcp-config --tools "" --permission-mode dontAsk --disable-slash-commands --no-session-persistence \
    --settings '{"disableAllHooks":true,"disableClaudeAiConnectors":true}' < in.jsonl > out.txt 2> err.txt
else
  allow_read "$H/.codex" "$H/Library/Keychains"; allow_write "$H/.codex"
  TMPDIR=$RUN/tmp "${SB[@]}" codex exec --json --ephemeral --skip-git-repo-check --ignore-user-config --ignore-rules -s read-only -m gpt-6-luna \
    -c features.plugins=false -c features.apps=false -c features.shell_tool=false -c features.view_image=false -c 'web_search="disabled"' \
    "$Q" --image test.png > out.txt 2> err.txt < /dev/null
fi
echo "exit $?"; tail -3 err.txt
