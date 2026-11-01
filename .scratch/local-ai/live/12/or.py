# Live OpenRouter checks for ticket 12. Reads OPENROUTER_API_KEY from repo .env.local; never prints it.
import json, sys, urllib.request, pathlib
env = pathlib.Path(__file__).resolve().parents[4] / ".env.local"
KEY = dict(l.split("=", 1) for l in env.read_text().split() if "=" in l)["OPENROUTER_API_KEY"].strip().strip('"')
def call(path, body=None):
    req = urllib.request.Request("https://openrouter.ai" + path, data=json.dumps(body).encode() if body else None,
        headers={"Authorization": "Bearer " + KEY, "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r: return json.load(r)
    except urllib.error.HTTPError as e: return {"http_error": e.code, "body": e.read().decode()}
if __name__ == "__main__":
    path = sys.argv[1]; body = json.loads(sys.argv[2]) if len(sys.argv) > 2 else None
    print(json.dumps(call(path, body), indent=1)[:4000])
