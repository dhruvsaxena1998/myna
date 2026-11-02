# Myna's MCP server: a stdio relay that forwards tool calls to the app over one Unix socket. Holds no data itself.
import json, socket, sys
SOCK = sys.argv[1]
RO = {"readOnlyHint": True, "destructiveHint": False, "openWorldHint": False}
TOOLS = [
    {"name": "search_transcript", "description": "Full-text search this meeting's transcript. Returns matching lines with time and speaker.",
     "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}, "speaker": {"type": "string"}}, "required": ["query"]}, "annotations": RO},
    {"name": "read_transcript", "description": "Read this meeting's transcript between two times (HH:MM:SS).",
     "inputSchema": {"type": "object", "properties": {"from": {"type": "string"}, "to": {"type": "string"}}, "required": ["from", "to"]}, "annotations": RO},
    {"name": "get_screenshots", "description": "Screenshots kept near a time in this meeting.",
     "inputSchema": {"type": "object", "properties": {"near": {"type": "string"}}, "required": ["near"]}, "annotations": RO},
]
def app(tool, args):
    s = socket.socket(socket.AF_UNIX); s.connect(SOCK)
    with s, s.makefile("rw") as f:
        f.write(json.dumps({"tool": tool, "args": args}) + "\n"); f.flush()
        return json.loads(f.readline())["text"]
def reply(i, result=None, error=None):
    msg = {"jsonrpc": "2.0", "id": i}; msg.update({"error": error} if error else {"result": result})
    sys.stdout.write(json.dumps(msg) + "\n"); sys.stdout.flush()
for line in sys.stdin:
    if not line.strip(): continue
    m = json.loads(line); i = m.get("id"); meth = m.get("method")
    if i is None: continue  # notification
    if meth == "initialize":
        reply(i, {"protocolVersion": m["params"].get("protocolVersion", "2025-06-18"), "capabilities": {"tools": {}}, "serverInfo": {"name": "myna", "version": "0.0.1"}})
    elif meth == "tools/list": reply(i, {"tools": TOOLS})
    elif meth == "tools/call":
        try: reply(i, {"content": [{"type": "text", "text": app(m["params"]["name"], m["params"].get("arguments") or {})}]})
        except Exception as e: reply(i, {"content": [{"type": "text", "text": f"Myna is unavailable: {e}"}], "isError": True})
    elif meth == "ping": reply(i, {})
    else: reply(i, error={"code": -32601, "message": "method not found"})
