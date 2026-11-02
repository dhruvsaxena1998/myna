#!/bin/bash
# Usage: run.sh <claude|codex|opencode|pi>. Runs one real Chat question under Seatbelt with Myna's relay as the only tool source.
set -u
CLI=$1; HERE=$(cd "$(dirname "$0")" && pwd); RUN=/private/tmp/m27/$CLI
rm -rf "$RUN"; mkdir -p "$RUN/tmp" "$RUN/cfg"; cp "$HERE/relay.py" "$RUN/"
python3 -I "$HERE/app.py" "$RUN/s.sock" "$RUN/app.log" & APP=$!; trap 'kill $APP 2>/dev/null' EXIT; sleep 0.3
Q="${Q:-What date did Priya give for the launch, and why? Look it up with the meeting tools, then answer in one sentence.}"
RELAY_ARGS="\"-I\",\"$RUN/relay.py\",\"$RUN/s.sock\""
H=$HOME; PROF=$RUN/profile.sb; cp "$HERE/profile.sb" "$PROF"
# Seatbelt matches resolved paths, so allow the real path behind any symlink too
allow_read() { for p; do for q in "$p" "$(realpath "$p" 2>/dev/null)"; do [ -n "$q" ] && echo "(allow file-read-data (subpath \"$q\"))" >> "$PROF"; done; done; }
allow_write() { for p; do for q in "$p" "$(realpath "$p" 2>/dev/null)"; do [ -n "$q" ] && echo "(allow file-write* (subpath \"$q\"))" >> "$PROF"; done; done; }
SB=(sandbox-exec -f "$PROF" -D HOME="$H" -D RUN="$RUN")
case $CLI in
claude)
  allow_read "$H/.local/bin" "$H/.local/share/claude" "$H/.claude" "$H/.claude.json" "$H/Library/Keychains" ${EXTRA_READ:-}
  [ -n "${EXTRA_WRITE:-}" ] && allow_write $EXTRA_WRITE
  CMD=(env ENABLE_CLAUDEAI_MCP_SERVERS=false claude -p "$Q" --model haiku --output-format stream-json --verbose
    --mcp-config "{\"mcpServers\":{\"myna\":{\"command\":\"/usr/bin/python3\",\"args\":[$RELAY_ARGS]}}}" --strict-mcp-config
    --tools "" --allowedTools 'mcp__myna__*' --permission-mode dontAsk --disable-slash-commands --no-session-persistence ${CLAUDE_EXTRA:-}
    --settings '{"disableAllHooks":true,"disableClaudeAiConnectors":true}') ;;
codex)
  allow_read "$H/.codex" "$H/Library/Keychains" ${EXTRA_READ:-}
  [ -n "${EXTRA_WRITE:-}" ] && allow_write $EXTRA_WRITE
  CMD=(codex exec --json --ephemeral --skip-git-repo-check --ignore-user-config --ignore-rules -s read-only -m "${MODEL:-gpt-6-luna}"
    -c features.plugins=false -c features.apps=false -c features.shell_tool=false -c features.view_image=false -c 'web_search="disabled"' ${CODEX_OFF:-}
    -c 'mcp_servers.myna.command="/usr/bin/python3"' -c "mcp_servers.myna.args=[$RELAY_ARGS]"
    -c mcp_servers.myna.required=true -c 'mcp_servers.myna.default_tools_approval_mode="approve"' "$Q") ;;
opencode)
  allow_read "$H/.opencode" "$H/.local/share/opencode" "$H/.cache/opencode" "$H/Library/Keychains" ${EXTRA_READ:-}
  allow_write "$H/.local/share/opencode" "$H/.cache/opencode" ${EXTRA_WRITE:-}
  CMD=(env XDG_CONFIG_HOME="$RUN/cfg" OPENCODE_DISABLE_PROJECT_CONFIG=1 OPENCODE_DISABLE_CLAUDE_CODE=1
    OPENCODE_CONFIG_CONTENT="{\"mcp\":{\"myna\":{\"type\":\"local\",\"command\":[\"/usr/bin/python3\",$RELAY_ARGS],\"enabled\":true}},\"permission\":{\"*\":\"deny\",\"skill\":\"deny\",\"myna_*\":\"allow\"}${OC_EXTRA:-}}"
    opencode run --standalone --format json -m "${MODEL:-openai/gpt-6-luna}" "$Q") ;;
pi)
  allow_read "$H/.local/share/mise/installs/node" "$H/.pi/agent" "$H/Library/Keychains" ${EXTRA_READ:-}
  [ -n "${EXTRA_WRITE:-}" ] && allow_write $EXTRA_WRITE
  echo "export default function (pi) { pi.registerMcpServer(\"myna\", { command: \"/usr/bin/python3\", args: [$RELAY_ARGS], exposure: \"direct\" }); }" > "$RUN/myna.js"
  CMD=(pi -p --mode json --no-session -ne -e builtin:mcp -e "$RUN/myna.js" --tools 'mcp__myna__*' -nc -ns -np --no-themes
    --provider "${PROVIDER:-openai-codex}" --model "${MODEL:-gpt-6-luna}" "$Q") ;;
esac
echo "== probes under the same profile"
"${SB[@]}" /bin/sh -c "cat '$H/.zshrc' >/dev/null && echo 'READ ~/.zshrc: ALLOWED' || echo 'read ~/.zshrc: denied'
  touch '$H/m27-probe' 2>/dev/null && echo 'WRITE ~: ALLOWED' || echo 'write ~: denied'
  touch /private/tmp/m27-probe 2>/dev/null && echo 'WRITE /tmp: ALLOWED' || echo 'write /tmp outside run dir: denied'
  for f in '$H/.codex/auth.json' '$H/.pi/agent/auth.json' '$H/.local/share/opencode/auth.json' '$H/.ssh/id_ed25519'; do cat \"\$f\" >/dev/null 2>&1 && echo \"READ \$f: ALLOWED\" || echo \"read \$f: denied\"; done"
rm -f "$H/m27-probe" /private/tmp/m27-probe
echo "== run"; cd "$RUN"
START=$(date +%s); TMPDIR=$RUN/tmp "${SB[@]}" "${CMD[@]}" > "$RUN/out.txt" 2> "$RUN/err.txt" < /dev/null; echo "exit $? in $(( $(date +%s)-START ))s"
echo "== app log (tool calls that reached the socket)"; cat "$RUN/app.log" 2>/dev/null || echo "(none)"
echo "== stderr tail"; tail -5 "$RUN/err.txt"
