"""Run a sequence of kiln MCP tool calls in one server session.

Usage:
    python3 -I tools/mcp-smoke/session.py <kiln-binary> <workdir> <calls.json> [serve flags, e.g. --live]

`calls.json` is a list of steps:
    {"tool": "kiln_sim_run", "args": {...}}            # call a tool, print its text
    {"wait": "<run_id>"}                               # poll kiln_sim_status until it settles
    {"sleep": 3}                                       # pause (to cancel a run mid-way)
A string argument starting with `@` is replaced by that file's contents
(`{"scenario": "@board.yaml"}`).
Every result is printed as `== <tool> (error)?` followed by the text, so an
agent (or a grader) can read the session like a transcript.
"""
import json, subprocess, sys, time

binary, workdir, calls_path = sys.argv[1], sys.argv[2], sys.argv[3]
steps = json.load(open(calls_path))
extra = sys.argv[4:]  # more `kiln mcp serve` flags, e.g. --live
proc = subprocess.Popen([binary, "mcp", "serve", "--workdir", workdir, "--threads", "2", *extra],
                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
nid = 0


def rpc(method, params=None, notify=False):
    global nid
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    if not notify:
        nid += 1
        msg["id"] = nid
    proc.stdin.write(json.dumps(msg) + "\n")
    proc.stdin.flush()
    if notify:
        return None
    while True:
        line = proc.stdout.readline()
        if not line:
            raise SystemExit("server closed")
        r = json.loads(line)
        if r.get("id") == nid:
            return r


def call(tool, args):
    r = rpc("tools/call", {"name": tool, "arguments": args})
    if "error" in r:
        return True, json.dumps(r["error"])
    res = r["result"]
    return bool(res.get("isError")), "".join(c.get("text", "") for c in res.get("content", []))


rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "eval", "version": "0"}})
rpc("notifications/initialized", notify=True)
calls = 0
for s in steps:
    if "sleep" in s:
        time.sleep(s["sleep"])
        continue
    if "wait" in s:
        while True:
            err, text = call("kiln_sim_status", {"run_id": s["wait"]})
            calls += 1
            st = json.loads(text) if not err else {"status": text}
            if st.get("status") not in ("running", "starting"):
                print(f"== kiln_sim_status (after waiting)\n{text}\n")
                break
            time.sleep(1)
        continue
    args = {k: open(v[1:]).read() if isinstance(v, str) and v.startswith("@") else v
            for k, v in s["args"].items()}
    err, text = call(s["tool"], args)
    calls += 1
    print(f"== {s['tool']}{' (error)' if err else ''}\n{text}\n")
print(f"== tool calls: {calls}")
proc.stdin.close()
proc.wait(timeout=60)
