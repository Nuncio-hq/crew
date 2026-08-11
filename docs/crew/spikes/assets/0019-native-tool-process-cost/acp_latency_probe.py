#!/usr/bin/env python3
"""Spike 0019 latency + turn-RSS measurement over a real authenticated engine.

Measures, per engine:
  - cold: spawn -> initialize -> session/new -> first agent_message_chunk
  - warm: new session on the already-running process -> first chunk
  - RSS idle after initialize, and peak RSS during one turn
"""

import argparse
import json
import os
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from acp_probe import AcpClient  # noqa: E402

PROMPT = "Reply with exactly: PONG"


def rss_kib(pid):
    """Resident set size of pid plus its descendants, in KiB."""
    total = 0
    try:
        pids = [pid]
        # one level of children is enough for these adapters
        with open("/proc/%d/task/%d/children" % (pid, pid)) as fh:
            pids += [int(p) for p in fh.read().split()]
    except OSError:
        pids = [pid]
    for p in pids:
        try:
            with open("/proc/%d/status" % p) as fh:
                for line in fh:
                    if line.startswith("VmRSS:"):
                        total += int(line.split()[1])
                        break
        except OSError:
            continue
    return total


def first_chunk_latency(client, session_id, peak_holder=None, stop_evt=None):
    start = time.monotonic()
    seen = {"t": None}

    def watcher():
        while True:
            for note in list(client._notifications):
                params = note.get("params", {})
                if (note.get("method") == "session/update"
                        and params.get("sessionId") == session_id
                        and params.get("update", {}).get("sessionUpdate")
                        == "agent_message_chunk"):
                    seen["t"] = time.monotonic() - start
                    return
            if stop_evt and stop_evt.is_set():
                return
            time.sleep(0.005)

    t = threading.Thread(target=watcher, daemon=True)
    t.start()
    resp = client.request("session/prompt", {
        "sessionId": session_id,
        "prompt": [{"type": "text", "text": PROMPT}],
    }, timeout=300)
    total = time.monotonic() - start
    t.join(timeout=1)
    return seen["t"], total, resp.get("result", resp.get("error"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cmd", required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument("--cwd", default="/tmp/acp-latency-ws")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    os.makedirs(args.cwd, exist_ok=True)
    out = {"engine": args.label, "cmd": args.cmd}

    t0 = time.monotonic()
    client = AcpClient(args.cmd.split(), args.cwd, log=[])
    client.request("initialize", {
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": {"readTextFile": True, "writeTextFile": True},
            "terminal": False,
        },
    }, timeout=90)
    out["spawn_to_initialized_ms"] = round((time.monotonic() - t0) * 1000, 1)
    time.sleep(1.0)
    out["idle_rss_kib"] = rss_kib(client.proc.pid)

    a = client.request("session/new", {"cwd": args.cwd, "mcpServers": []},
                       timeout=120)["result"]
    out["cold_spawn_to_session_ready_ms"] = round(
        (time.monotonic() - t0) * 1000, 1)

    peak = {"v": out["idle_rss_kib"]}
    stop = threading.Event()

    def sampler():
        while not stop.is_set():
            peak["v"] = max(peak["v"], rss_kib(client.proc.pid))
            time.sleep(0.1)

    sampler_t = threading.Thread(target=sampler, daemon=True)
    sampler_t.start()

    ft, total, stop_reason = first_chunk_latency(client, a["sessionId"])
    out["cold_first_token_ms"] = round(ft * 1000, 1) if ft else None
    out["cold_turn_total_ms"] = round(total * 1000, 1)
    out["cold_turn_stop"] = stop_reason
    out["cold_process_to_first_token_ms"] = (
        round((time.monotonic() - t0) * 1000 - (total - (ft or total)) * 1000, 1))

    # warm: new session on the same, already-running process
    tw = time.monotonic()
    b = client.request("session/new", {"cwd": args.cwd, "mcpServers": []},
                       timeout=120)["result"]
    out["warm_session_new_ms"] = round((time.monotonic() - tw) * 1000, 1)
    ftw, totalw, stopw = first_chunk_latency(client, b["sessionId"])
    out["warm_first_token_ms"] = round(ftw * 1000, 1) if ftw else None
    out["warm_session_new_to_first_token_ms"] = round(
        (time.monotonic() - tw) * 1000 - (totalw - (ftw or totalw)) * 1000, 1)
    out["warm_turn_total_ms"] = round(totalw * 1000, 1)
    out["warm_turn_stop"] = stopw

    stop.set()
    sampler_t.join(timeout=1)
    out["peak_rss_kib_two_turns"] = peak["v"]
    out["rss_growth_kib"] = peak["v"] - out["idle_rss_kib"]
    out["stderr_tail"] = client.stderr_lines[-15:]
    client.close()
    with open(args.out, "w") as fh:
        json.dump(out, fh, indent=2)
    print(json.dumps({k: v for k, v in out.items() if k != "stderr_tail"},
                     indent=2))


if __name__ == "__main__":
    main()
