#!/usr/bin/env python3
"""S0-4 probe: two concurrent `hermes -p crewspike acp` processes, one
prompt each, overlapping in time. Watches for SQLite lock errors, crashes,
and cross-talk. Prints per-process evidence lines."""
import json
import os
import subprocess
import sys
import threading
import time

PROFILE = "crewspike"
TIMEOUT_S = 240

class AcpProc:
    def __init__(self, tag):
        self.tag = tag
        env = dict(os.environ)
        env["HERMES_ACP_SKIP_CONFIGURED_MCP"] = "1"
        self.proc = subprocess.Popen(
            ["hermes", "-p", PROFILE, "acp"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, env=env, text=True, bufsize=1,
        )
        self.responses = {}
        self.chunks = []
        self.stderr_lines = []
        self.lock = threading.Condition()
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()

    def _read_stdout(self):
        for line in self.proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            with self.lock:
                if "id" in msg and ("result" in msg or "error" in msg):
                    self.responses[msg["id"]] = msg
                    self.lock.notify_all()
                elif msg.get("method") == "session/update":
                    upd = msg.get("params", {}).get("update", {})
                    if upd.get("sessionUpdate") == "agent_message_chunk":
                        c = upd.get("content", {})
                        if c.get("type") == "text":
                            self.chunks.append(c.get("text", ""))

    def _read_stderr(self):
        for line in self.proc.stderr:
            self.stderr_lines.append(line.rstrip())

    def send(self, id_, method, params):
        self.proc.stdin.write(json.dumps(
            {"jsonrpc": "2.0", "id": id_, "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()

    def wait(self, id_, timeout=TIMEOUT_S):
        deadline = time.time() + timeout
        with self.lock:
            while id_ not in self.responses:
                rem = deadline - time.time()
                if rem <= 0:
                    return None
                self.lock.wait(rem)
            return self.responses[id_]

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()


def run_turn(p: AcpProc, word: str, results: dict):
    p.send(1, "initialize", {"protocolVersion": 1, "clientCapabilities": {}})
    if not p.wait(1):
        results[p.tag] = "FAIL initialize timeout"
        return
    p.send(2, "session/new", {"cwd": os.getcwd(), "mcpServers": []})
    new = p.wait(2)
    if not new or "error" in new:
        results[p.tag] = f"FAIL session/new {json.dumps(new)[:200]}"
        return
    sid = new["result"]["sessionId"]
    p.send(3, "session/prompt", {
        "sessionId": sid,
        "prompt": [{"type": "text",
                    "text": f"Reply with exactly the word: {word}. Nothing else. Do not use tools."}],
    })
    resp = p.wait(3)
    reply = "".join(p.chunks).strip()
    results[p.tag] = {
        "sessionId": sid,
        "stop": (resp or {}).get("result", {}).get("stopReason"),
        "reply": reply[:120],
        "expected": word,
        "ok": word in reply,
    }


a, b = AcpProc("A"), AcpProc("B")
results = {}
ta = threading.Thread(target=run_turn, args=(a, "alpha", results))
tb = threading.Thread(target=run_turn, args=(b, "bravo", results))
t0 = time.time()
ta.start(); tb.start()
ta.join(TIMEOUT_S); tb.join(TIMEOUT_S)
print(f"elapsed {time.time()-t0:.1f}s")
for tag, r in sorted(results.items()):
    print(tag, json.dumps(r))

def lock_errors(lines):
    return [l for l in lines if "locked" in l.lower() or "sqlite" in l.lower()
            or "traceback" in l.lower()]

print("A stderr lock/crash lines:", lock_errors(a.stderr_lines)[:5])
print("B stderr lock/crash lines:", lock_errors(b.stderr_lines)[:5])
a.close(); b.close()
print("exit codes:", a.proc.returncode, b.proc.returncode)
