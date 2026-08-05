#!/usr/bin/env python3
"""S0-1 probe: ACP handshake against `hermes -p crewspike acp`.

Speaks newline-delimited JSON-RPC over stdio (the ACP transport buzz-acp
uses): initialize -> session/new -> session/prompt (trivial) -> capture
models catalog + stopReason. Asserts nothing itself; prints evidence.
"""
import json
import os
import subprocess
import sys
import threading
import time

CMD = ["hermes", "-p", "crewspike", "acp"]
TIMEOUT_S = 240

env = dict(os.environ)
env["HERMES_ACP_SKIP_CONFIGURED_MCP"] = "1"  # mirror buzz-acp default_agent_env

proc = subprocess.Popen(
    CMD,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    env=env,
    text=True,
    bufsize=1,
)

stderr_lines = []
def drain_stderr():
    for line in proc.stderr:
        stderr_lines.append(line.rstrip())
threading.Thread(target=drain_stderr, daemon=True).start()

responses = {}
notifications = []
lock = threading.Condition()

def reader():
    for line in proc.stdout:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        with lock:
            if "id" in msg and ("result" in msg or "error" in msg):
                responses[msg["id"]] = msg
                lock.notify_all()
            else:
                notifications.append(msg)
threading.Thread(target=reader, daemon=True).start()

def send(id_, method, params):
    req = {"jsonrpc": "2.0", "id": id_, "method": method, "params": params}
    proc.stdin.write(json.dumps(req) + "\n")
    proc.stdin.flush()

def wait(id_, timeout=TIMEOUT_S):
    deadline = time.time() + timeout
    with lock:
        while id_ not in responses:
            remaining = deadline - time.time()
            if remaining <= 0:
                return None
            lock.wait(remaining)
        return responses[id_]

t0 = time.time()
send(1, "initialize", {
    "protocolVersion": 1,
    "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}},
})
init = wait(1)
print(f"[{time.time()-t0:.1f}s] initialize -> {json.dumps(init)[:600]}")
if not init or "error" in (init or {}):
    print("STDERR tail:", "\n".join(stderr_lines[-15:]))
    proc.kill()
    sys.exit(1)

send(2, "session/new", {"cwd": os.getcwd(), "mcpServers": []})
new = wait(2)
print(f"[{time.time()-t0:.1f}s] session/new -> {json.dumps(new)[:1500]}")
if not new or "error" in (new or {}):
    print("STDERR tail:", "\n".join(stderr_lines[-15:]))
    proc.kill()
    sys.exit(1)

sid = new["result"]["sessionId"]
models = new["result"].get("models")
print("MODELS_PAYLOAD:", json.dumps(models)[:800])

send(3, "session/prompt", {
    "sessionId": sid,
    "prompt": [{"type": "text", "text": "Reply with exactly the word: pong. Nothing else. Do not use tools."}],
})
resp = wait(3)
print(f"[{time.time()-t0:.1f}s] session/prompt -> {json.dumps(resp)[:400]}")

chunks = []
for n in notifications:
    if n.get("method") == "session/update":
        upd = n.get("params", {}).get("update", {})
        if upd.get("sessionUpdate") == "agent_message_chunk":
            c = upd.get("content", {})
            if c.get("type") == "text":
                chunks.append(c.get("text", ""))
print("AGENT_REPLY:", "".join(chunks)[:200])
print("NOTIFICATION_COUNT:", len(notifications))

proc.stdin.close()
try:
    proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.kill()
print("STDERR tail:", "\n".join(stderr_lines[-8:]))
print("DONE exit", proc.returncode)
