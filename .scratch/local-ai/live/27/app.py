# Stand-in for the Myna app: answers transcript tools for one meeting over a Unix socket and logs every call.
import json, os, socket, sys
sock_path, log_path = sys.argv[1], sys.argv[2]
LINES = [  # fixture meeting; the launch date is only findable through the tools
    ("00:00:05", "Sam", "Okay, let's go through the launch checklist."),
    ("00:03:12", "Priya", "The launch moves to November 14th because the payments review slipped a week."),
    ("00:03:40", "Sam", "Then design review has to happen next Tuesday."),
    ("00:07:02", "Priya", "I'll send the updated plan to the leads tonight."),
]
def answer(tool, args):
    if tool == "search_transcript":
        q = args.get("query", "").lower().split()
        hits = [l for l in LINES if any(w in l[2].lower() for w in q) and (not args.get("speaker") or args["speaker"].lower() == l[1].lower())]
        return "\n".join(f"[{t}] {s}: {x}" for t, s, x in hits) or "No matches."
    if tool == "read_transcript":
        return "\n".join(f"[{t}] {s}: {x}" for t, s, x in LINES if args.get("from", "") <= t <= args.get("to", "99"))
    return "No screenshots in this meeting."
if os.path.exists(sock_path): os.unlink(sock_path)
srv = socket.socket(socket.AF_UNIX); srv.bind(sock_path); srv.listen()
while True:
    c, _ = srv.accept()
    with c, c.makefile("rw") as f:
        for line in f:
            req = json.loads(line); text = answer(req["tool"], req.get("args") or {})
            with open(log_path, "a") as log: log.write(json.dumps({"tool": req["tool"], "args": req.get("args")}) + "\n")
            f.write(json.dumps({"text": text}) + "\n"); f.flush()
