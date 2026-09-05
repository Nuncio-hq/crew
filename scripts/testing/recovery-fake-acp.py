#!/usr/bin/env python3
"""Controlled ACP peer for isolated real-relay recovery tests; no model/network calls."""
import json
import os
from pathlib import Path
import sys
import subprocess
import threading
import time
import uuid

root = Path(os.environ["RECOVERY_FIXTURE_DIR"])
lock = threading.Lock()

def emit(frame):
    with lock:
        with (root / "frames.jsonl").open("a") as stream:
            stream.write(json.dumps({"direction": "out", "frame": frame}) + "\n")
        print(json.dumps(frame), flush=True)

def result(frame, value):
    emit({"jsonrpc": "2.0", "id": frame["id"], "result": value})

def respond(frame):
    method = frame.get("method")
    if method == "initialize":
        result(frame, {"protocolVersion": 1, "agentCapabilities": {},
                       "agentInfo": {"name": "recovery-wire-fixture", "version": "1"}})
    elif method == "session/new":
        while not (root / "release-new").exists():
            time.sleep(0.025)
        result(frame, {"sessionId": str(uuid.uuid4())})
    elif method == "session/prompt":
        while not (root / "release-prompt").exists():
            time.sleep(0.025)
        if (root / "cancel-seen").exists():
            result(frame, {"stopReason": "cancelled"})
            (root / "cancel-completed").touch()
            return
        target = json.loads((root / "reply-target.json").read_text())
        sent = subprocess.run([os.environ["RECOVERY_BUZZ_CLI"], "messages", "send",
            "--channel", target["channel"], "--reply-to", target["trigger"],
            "--content", "Recovery wire fixture completed successfully."],
            capture_output=True, text=True, check=True)
        with (root / "published-replies.jsonl").open("a") as output:
            output.write(sent.stdout.strip() + "\n")
        emit({"jsonrpc": "2.0", "method": "session/update", "params": {
            "sessionId": frame["params"]["sessionId"], "update": {
                "sessionUpdate": "agent_message_chunk", "content": {
                    "type": "text", "text": "Recovery wire fixture completed successfully."}}}})
        result(frame, {"stopReason": "end_turn"})
    elif method == "session/cancel":
        # Deliberately wait for release: allows disconnect during cancel-drain.
        (root / "cancel-seen").touch()
    elif "id" in frame:
        result(frame, {})

for line in sys.stdin:
    frame = json.loads(line)
    with lock:
        with (root / "frames.jsonl").open("a") as stream:
            stream.write(json.dumps({"direction": "in", "frame": frame}) + "\n")
    threading.Thread(target=respond, args=(frame,), daemon=True).start()
