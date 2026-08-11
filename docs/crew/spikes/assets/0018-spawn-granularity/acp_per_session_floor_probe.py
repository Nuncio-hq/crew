#!/usr/bin/env python3
"""Spike 0018 probe 2: per-session native-tool floor on ONE engine process.

Session A keeps the default mode; session B is switched to the engine's
read-only mode via session-addressed `session/set_config_option` (or
`session/set_mode`).

Permission handling is a probe *variable*, because it changes what a run can
prove:

* `--deny-permissions` (default): the client rejects every request. A session
  whose floor is genuinely read-only cannot write, but an engine that asks
  before writing (Grok) has its unrestricted session denied too, so the run
  cannot separate engine floor from client policy.
* `--approve-permissions`: the client approves every request, so a refusal is
  the engine's own floor talking. This is the only variant that is evidence
  about the engine on engines that ask.
"""

import argparse
import json
import os
import sys
import time
import uuid

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from acp_two_session_probe import AcpClient, prompt  # noqa: E402


def deny_permissions(client):
    def _auto_reply(msg):
        method = msg.get("method")
        if method == "session/request_permission":
            opts = msg.get("params", {}).get("options", [])
            reject = next(
                (o for o in opts if o.get("kind", "").startswith("reject")), None)
            client.denied_requests.append({
                "sessionId": msg.get("params", {}).get("sessionId"),
                "toolCall": msg.get("params", {}).get("toolCall", {}).get("title"),
            })
            client._send({
                "jsonrpc": "2.0", "id": msg["id"],
                "result": {"outcome": {
                    "outcome": "selected",
                    "optionId": (reject or {}).get("optionId", "reject"),
                }},
            })
            return
        if method in ("fs/read_text_file", "fs/write_text_file"):
            # Deny client-side filesystem proxying too; we are measuring the
            # engine's own native-tool floor, not our proxy's generosity.
            client._send({"jsonrpc": "2.0", "id": msg["id"],
                          "error": {"code": -32000, "message": "denied by probe"}})
            return
        client._send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})

    client.denied_requests = []
    client._auto_reply = _auto_reply


def approve_permissions(client):
    def _auto_reply(msg):
        method = msg.get("method")
        if method == "session/request_permission":
            opts = msg.get("params", {}).get("options", [])
            allow = next(
                (o for o in opts if o.get("kind", "").startswith("allow")), None)
            client.approved_requests.append({
                "sessionId": msg.get("params", {}).get("sessionId"),
                "toolCall": msg.get("params", {}).get("toolCall", {}).get("title"),
            })
            client._send({
                "jsonrpc": "2.0", "id": msg["id"],
                "result": {"outcome": {
                    "outcome": "selected",
                    "optionId": (allow or {}).get("optionId", "allow"),
                }},
            })
            return
        client._send({"jsonrpc": "2.0", "id": msg["id"], "result": {}})

    client.approved_requests = []
    client.denied_requests = []
    client._auto_reply = _auto_reply


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cmd", required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument("--cwd", default="/tmp/acp-probe2-ws")
    ap.add_argument("--out", required=True)
    ap.add_argument("--readonly-value", default="read-only")
    ap.add_argument("--config-id", default="mode")
    ap.add_argument("--use-set-mode", action="store_true",
                    help="use session/set_mode instead of set_config_option")
    ap.add_argument("--approve-permissions", action="store_true",
                    help="approve every permission request, so a refusal is the "
                         "engine's own floor rather than the client's policy")
    ap.add_argument("--no-fs-capability", action="store_true",
                    help="advertise no client fs capability, forcing the engine "
                         "to use its own native file tools instead of proxying "
                         "writes back through the client")
    args = ap.parse_args()

    os.makedirs(args.cwd, exist_ok=True)
    findings = {"engine": args.label, "cmd": args.cmd, "started": time.time()}
    log = []
    client = AcpClient(args.cmd.split(), args.cwd, log=log)
    findings["permission_policy"] = (
        "approve" if args.approve_permissions else "deny")
    if args.approve_permissions:
        approve_permissions(client)
    else:
        deny_permissions(client)
    fs_capability = (
        {"readTextFile": False, "writeTextFile": False}
        if args.no_fs_capability
        else {"readTextFile": True, "writeTextFile": True}
    )
    findings["client_fs_capability"] = fs_capability
    try:
        client.request("initialize", {
            "protocolVersion": 1,
            "clientCapabilities": {"fs": fs_capability, "terminal": False},
        }, timeout=60)
        a = client.request("session/new", {"cwd": args.cwd, "mcpServers": []},
                           timeout=120)["result"]
        b = client.request("session/new", {"cwd": args.cwd, "mcpServers": []},
                           timeout=120)["result"]
        sid_a, sid_b = a["sessionId"], b["sessionId"]
        findings["pid"] = client.proc.pid
        findings["session_ids"] = {"a": sid_a, "b": sid_b}
        findings["config_options_advertised"] = [
            {"id": o.get("id"), "currentValue": o.get("currentValue"),
             "options": [v.get("value") for v in o.get("options", [])]}
            for o in a.get("configOptions", [])
        ]

        if args.use_set_mode:
            resp = client.request("session/set_mode", {
                "sessionId": sid_b,
                "modeId": args.readonly_value,
            }, timeout=60)
        else:
            resp = client.request("session/set_config_option", {
                "sessionId": sid_b,
                "configId": args.config_id,
                "value": args.readonly_value,
            }, timeout=60)
        findings["set_config_option_b"] = resp.get("result", resp.get("error"))

        marker = uuid.uuid4().hex[:8]
        write_prompt = (
            "Create a file named probe-%s.txt in the current working directory "
            "containing the single line hello, using your file editing tool. "
            "Then reply WROTE if the file now exists, or DENIED if you were "
            "not permitted."
        )

        rb = prompt(client, sid_b, write_prompt % ("b-" + marker))
        findings["b_stop"] = rb.get("result", rb.get("error"))
        findings["b_text"] = client.session_text(sid_b)
        findings["b_file_exists"] = os.path.exists(
            os.path.join(args.cwd, "probe-b-%s.txt" % marker))

        ra = prompt(client, sid_a, write_prompt % ("a-" + marker))
        findings["a_stop"] = ra.get("result", ra.get("error"))
        findings["a_text"] = client.session_text(sid_a)
        findings["a_file_exists"] = os.path.exists(
            os.path.join(args.cwd, "probe-a-%s.txt" % marker))

        findings["denied_requests"] = client.denied_requests
        findings["approved_requests"] = getattr(client, "approved_requests", [])
        findings["floor_differs_per_session"] = (
            findings["a_file_exists"] and not findings["b_file_exists"])
        findings["ls_cwd"] = sorted(os.listdir(args.cwd))
    except Exception as exc:  # noqa: BLE001
        findings["fatal"] = str(exc)[:2000]

    # Engine stderr carries auth prefixes and bearer fragments, so it is never
    # written to a committed asset.
    findings["stderr_tail"] = [
        "<redacted: engine stderr contained auth prefixes/bearer fragments>"
    ]
    client.close()
    with open(args.out, "w") as fh:
        json.dump({"findings": findings, "wire_log": log}, fh, indent=2)
    print(json.dumps({k: v for k, v in findings.items()
                      if k not in ("stderr_tail", "config_options_advertised")},
                     indent=2)[:5000])


if __name__ == "__main__":
    main()
