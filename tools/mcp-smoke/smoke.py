"""A minimal MCP stdio client that exercises every `kiln mcp serve` tool.

Usage (after `cargo build --release`, with the card DB built):
    python3 -I tools/mcp-smoke/smoke.py . /tmp/kiln-mcp-smoke

It imports the four test decks, runs a small pod, polls it, reads reports,
inspects a game, compares runs, validates playbooks and runs a rules scenario.
"""
import json, subprocess, sys, time, pathlib

repo = pathlib.Path(sys.argv[1])
work = pathlib.Path(sys.argv[2])
work.mkdir(parents=True, exist_ok=True)
proc = subprocess.Popen(
    [str(repo / "target/release/kiln"), "mcp", "serve", "--workdir", str(work), "--threads", "2"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, cwd=repo)
nid = 0

def send(method, params=None, notify=False):
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

def call(name, args):
    r = send("tools/call", {"name": name, "arguments": args})
    if "error" in r:
        return True, json.dumps(r["error"])
    res = r["result"]
    text = "".join(c.get("text", "") for c in res.get("content", []))
    return bool(res.get("isError")), text

init = send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}})
print("server:", init["result"]["serverInfo"], "| instructions:", init["result"].get("instructions", "")[:60], "…")
send("notifications/initialized", notify=True)
tools = send("tools/list")["result"]["tools"]
print("tools:", [t["name"] for t in tools])

decks = {}
for d in ["kinnan-verified", "thrasios-tymna", "najeela", "rograkh-silas"]:
    err, text = call("kiln_deck_import", {"source": str(repo / f"tests/fixtures/decks/{d}.txt"), "name": d})
    j = json.loads(text)
    decks[d] = j["file"]
    print("import", d, "legal", j["legal"], "unsupported", len(j["unsupported"]), "file", j["file"])

err, text = call("kiln_card_lookup", {"names": ["Thassas Oracle", "Sol Ring", "Xyzzy Nonexistent"]})
for c in json.loads(text)["cards"]:
    print("lookup:", c.get("name") or c.get("query"), c.get("engine_support") or c.get("did_you_mean"))

spec = "spec: kiln-spec/1\nname: mcp-pod\ngames: 8\ndecks:\n" + "".join(
    f"  - {{ id: {k.replace('-', '_')}, source: {{ file: {v} }} }}\n" for k, v in decks.items()) + \
    "policies:\n  default: { kind: greedy }\nlimits: { max_turns: 60 }\n"
err, text = call("kiln_sim_run", {"spec_text": spec})
print("run:", err, json.loads(text)["status"] if not err else text)
err, text = call("kiln_sim_run", {"spec_text": spec.replace("games: 8", "gmaes: 8")})
print("bad spec:", err, text[:160])
for _ in range(120):
    err, text = call("kiln_sim_status", {"run_id": "mcp-pod"})
    st = json.loads(text)
    if st["status"] not in ("running", "starting"):
        break
    time.sleep(0.5)
print("status:", st["status"], st["games_done"], "/", st["games_total"])
err, text = call("kiln_sim_report", {"run_id": "mcp-pod"})
rep = json.loads(text)
print("concise report:", len(text), "chars; decks", [w["deck"] for w in rep["win_rates"]["base"]], "warnings", len(rep["warnings"]))
err, text = call("kiln_sim_report", {"run_id": "mcp-pod", "format": "detailed", "sections": ["win_rates"]})
print("detailed keys:", list(json.loads(text)["configs"][0].keys()))
err, text = call("kiln_game_inspect", {"run_id": "mcp-pod", "game_index": 0, "events": 5})
print("inspect:", err, json.loads(text)["record"]["outcome"], len(json.loads(text)["events"]), "events")
err, text = call("kiln_sim_compare", {"run_a": "mcp-pod", "run_b": "mcp-pod"})
print("compare:", err, [d["difference"] for d in json.loads(text)["decks"]])
book = (repo / "examples/playbooks/najeela.rhai").read_text()
err, text = call("kiln_playbook_validate", {"deck": decks["najeela"], "playbook_text": book, "name": "najeela", "games": 4})
j = json.loads(text)
print("playbook:", err, j.get("compile_error"), j.get("playbook_share"), j.get("file"))
err, text = call("kiln_playbook_validate", {"deck": decks["najeela"], "playbook_text": "fn priority(view, d) {\n let = 1;\n}", "games": 2})
print("bad playbook:", json.loads(text).get("compile_error"))
scen = (repo / "tests/scenarios-real/thassas-oracle.yaml").read_text()
err, text = call("kiln_rules_scenario", {"scenario": scen})
j = json.loads(text)
print("scenario:", err, j.get("expectations_held"), j.get("result"), len(j.get("events", [])), "events")
proc.stdin.close()
proc.wait(timeout=30)
