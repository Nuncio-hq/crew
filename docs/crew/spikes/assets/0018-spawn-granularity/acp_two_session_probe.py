#!/usr/bin/env python3
"""Real-engine ACP probe for spike 0018.

Drives one engine process over ACP stdio with TWO live sessions and observes:
  Q1 context isolation: does session A's content leak into session B?
  Q2 per-session capability: is session-addressed set_config_option honored, and
     does the native-tool permission floor differ per session?

Writes a credential-free transcript to the given output JSON path.
"""

import argparse
import json
import os
import queue
import subprocess
import sys
import threading
import time
import uuid

SECRET = "PLUMBUS-7742"


class AcpClient:
    def __init__(self, cmd, cwd, env=None, log=None):
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=cwd,
            env=env or os.environ.copy(),
            text=True,
            bufsize=1,
        )
        self._id = 0
        self._responses = {}
        self._notifications = []
        self._incoming_requests = []
        self._lock = threading.Lock()
        self._q = queue.Queue()
        self.stderr_lines = []
        self.log = log if log is not None else []
        threading.Thread(target=self._reader, daemon=True).start()
        threading.Thread(target=self._stderr_reader, daemon=True).start()

    def _stderr_reader(self):
        for line in self.proc.stderr:
            self.stderr_lines.append(line.rstrip())

    def _reader(self):
        for line in self.proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                self.log.append({"unparsed_stdout": line[:500]})
                continue
            self.log.append({"recv": msg})
            if "id" in msg and ("result" in msg or "error" in msg):
                with self._lock:
                    self._responses[msg["id"]] = msg
            elif "id" in msg and "method" in msg:
                self._incoming_requests.append(msg)
                self._auto_reply(msg)
            else:
                self._notifications.append(msg)

    def _auto_reply(self, msg):
        """Answer agent->client requests so turns can proceed."""
        method = msg.get("method")
        result = {}
        if method == "session/request_permission":
            opts = msg.get("params", {}).get("options", [])
            allow = next(
                (o for o in opts if o.get("kind", "").startswith("allow")), None
            )
            if allow is None and opts:
                allow = opts[0]
            result = {
                "outcome": {
                    "outcome": "selected",
                    "optionId": allow.get("optionId") if allow else "allow",
                }
            }
        elif method == "fs/read_text_file":
            path = msg.get("params", {}).get("path")
            try:
                with open(path) as fh:
                    result = {"content": fh.read()}
            except OSError as exc:
                self._send({"jsonrpc": "2.0", "id": msg["id"],
                            "error": {"code": -32000, "message": str(exc)}})
                return
        elif method == "fs/write_text_file":
            params = msg.get("params", {})
            try:
                with open(params.get("path"), "w") as fh:
                    fh.write(params.get("content", ""))
                result = {}
            except OSError as exc:
                self._send({"jsonrpc": "2.0", "id": msg["id"],
                            "error": {"code": -32000, "message": str(exc)}})
                return
        self._send({"jsonrpc": "2.0", "id": msg["id"], "result": result})

    def _send(self, obj):
        self.log.append({"send": obj})
        self.proc.stdin.write(json.dumps(obj) + "\n")
        self.proc.stdin.flush()

    def request(self, method, params, timeout=180):
        self._id += 1
        rid = self._id
        self._send({"jsonrpc": "2.0", "id": rid, "method": method,
                    "params": params})
        deadline = time.time() + timeout
        while time.time() < deadline:
            with self._lock:
                if rid in self._responses:
                    return self._responses.pop(rid)
            if self.proc.poll() is not None:
                raise RuntimeError(
                    "engine exited rc=%s stderr=%s"
                    % (self.proc.returncode, "\n".join(self.stderr_lines[-20:]))
                )
            time.sleep(0.05)
        raise TimeoutError("no response to %s in %ss" % (method, timeout))

    def session_text(self, session_id):
        """Concatenate assistant text chunks seen for a session."""
        out = []
        for note in self._notifications:
            if note.get("method") != "session/update":
                continue
            params = note.get("params", {})
            if params.get("sessionId") != session_id:
                continue
            upd = params.get("update", {})
            kind = upd.get("sessionUpdate")
            if kind in ("agent_message_chunk", "agent_thought_chunk"):
                content = upd.get("content", {})
                if content.get("type") == "text" and kind == "agent_message_chunk":
                    out.append(content.get("text", ""))
        return "".join(out)

    def close(self):
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def prompt(client, session_id, text, timeout=240):
    return client.request(
        "session/prompt",
        {"sessionId": session_id,
         "prompt": [{"type": "text", "text": text}]},
        timeout=timeout,
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cmd", required=True, help="engine command, shell-split")
    ap.add_argument("--label", required=True)
    ap.add_argument("--cwd", default="/tmp/acp-probe-ws")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    os.makedirs(args.cwd, exist_ok=True)
    cmd = args.cmd.split()
    findings = {"engine": args.label, "cmd": cmd, "started": time.time()}
    log = []
    client = AcpClient(cmd, args.cwd, log=log)
    try:
        init = client.request("initialize", {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {"readTextFile": True, "writeTextFile": True},
                "terminal": False,
            },
        }, timeout=60)
        findings["initialize"] = init.get("result") or init.get("error")

        a = client.request("session/new", {"cwd": args.cwd, "mcpServers": []},
                           timeout=120)
        findings["session_new_a"] = a.get("result") or a.get("error")
        if "error" in a:
            findings["verdict"] = "BLOCKED: session/new failed"
            return finish(findings, log, client, args.out)
        sid_a = a["result"]["sessionId"]

        b = client.request("session/new", {"cwd": args.cwd, "mcpServers": []},
                           timeout=120)
        findings["session_new_b"] = b.get("result") or b.get("error")
        sid_b = b["result"]["sessionId"]
        findings["same_process_pid"] = client.proc.pid
        findings["session_ids"] = {"a": sid_a, "b": sid_b}
        findings["distinct_session_ids"] = sid_a != sid_b

        # Q1: context isolation.
        r1 = prompt(client, sid_a,
                    "Remember this exact code word for later: %s. "
                    "Reply with just: STORED" % SECRET)
        findings["a_turn1_stop"] = r1.get("result") or r1.get("error")
        findings["a_turn1_text"] = client.session_text(sid_a)

        r2 = prompt(client, sid_b,
                    "What code word were you asked to remember earlier in "
                    "this conversation? If you were never given one in THIS "
                    "conversation, reply exactly: NONE")
        findings["b_turn1_stop"] = r2.get("result") or r2.get("error")
        b_text = client.session_text(sid_b)
        findings["b_turn1_text"] = b_text
        findings["q1_leaked"] = SECRET in b_text

        # Control: session A still remembers it (proves the probe is valid).
        r3 = prompt(client, sid_a,
                    "Repeat the code word you were asked to remember. "
                    "Reply with just the code word.")
        findings["a_turn2_stop"] = r3.get("result") or r3.get("error")
        a_text2 = client.session_text(sid_a)
        findings["a_turn2_text"] = a_text2
        findings["q1_control_a_remembers"] = SECRET in a_text2

        # Q2: session-addressed config / permission floor.
        for method, params in (
            ("session/set_config_option",
             {"sessionId": sid_b, "key": "sandbox_mode",
              "value": "read-only"}),
            ("session/set_mode", {"sessionId": sid_b, "modeId": "plan"}),
        ):
            try:
                resp = client.request(method, params, timeout=60)
                findings["q2_" + method.replace("/", "_")] = (
                    resp.get("result") if "result" in resp else resp.get("error")
                )
            except Exception as exc:  # noqa: BLE001 - probe records the failure
                findings["q2_" + method.replace("/", "_")] = {
                    "exception": str(exc)[:400]
                }

        # Does the native-tool floor now differ between the two sessions?
        marker = uuid.uuid4().hex[:8]
        write_prompt = (
            "Create a file named probe-%s.txt in the current directory with "
            "the single line hello. Use your file-writing tool. Then reply "
            "WROTE or DENIED depending on what happened."
        )
        rb = prompt(client, sid_b, write_prompt % ("b-" + marker))
        findings["b_write_stop"] = rb.get("result") or rb.get("error")
        findings["b_write_text"] = client.session_text(sid_b)
        findings["b_file_exists"] = os.path.exists(
            os.path.join(args.cwd, "probe-b-%s.txt" % marker))

        ra = prompt(client, sid_a, write_prompt % ("a-" + marker))
        findings["a_write_stop"] = ra.get("result") or ra.get("error")
        findings["a_write_text"] = client.session_text(sid_a)
        findings["a_file_exists"] = os.path.exists(
            os.path.join(args.cwd, "probe-a-%s.txt" % marker))

        findings["q2_floor_differs"] = (
            findings["a_file_exists"] != findings["b_file_exists"])
    except Exception as exc:  # noqa: BLE001 - probe records the failure
        findings["fatal"] = str(exc)[:2000]
    return finish(findings, log, client, args.out)


def finish(findings, log, client, out_path):
    findings["stderr_tail"] = client.stderr_lines[-40:]
    findings["finished"] = time.time()
    client.close()
    with open(out_path, "w") as fh:
        json.dump({"findings": findings, "wire_log": log}, fh, indent=2)
    print(json.dumps({k: v for k, v in findings.items()
                      if k not in ("stderr_tail",)}, indent=2)[:6000])
    return 0


if __name__ == "__main__":
    sys.exit(main())
