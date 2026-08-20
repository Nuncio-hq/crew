#!/usr/bin/env python3
"""Spike 0056 probe: short ACP handshake for home-profile argv.

Handshake only: initialize -> session/new -> exit. Never session/prompt.
If ~/.hermes/profiles/default appears, kill that argv immediately.
"""
from __future__ import annotations

import json
import os
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

HOME = Path.home() / ".hermes"
PROFILES = HOME / "profiles"
DEFAULT_DIR = PROFILES / "default"
EVIDENCE_DIR = Path(__file__).resolve().parent / "0056-home-profile-acp"
TIMEOUT_S = 180
CWD = "/tmp/issue243-spike-cwd"

ARGV_CASES = [
    ("no_p", ["hermes", "acp"]),
    ("p_default", ["hermes", "-p", "default", "acp"]),
]


def snapshot() -> dict:
    profiles_listing = []
    if PROFILES.exists():
        profiles_listing = sorted(p.name for p in PROFILES.iterdir())
    return {
        "ts": time.time(),
        "profiles_dir_exists": PROFILES.exists(),
        "profiles_listing": profiles_listing,
        "default_dir_exists": DEFAULT_DIR.exists(),
        "default_dir_is_dir": DEFAULT_DIR.is_dir(),
        "home_state_db_exists": (HOME / "state.db").exists(),
        "home_state_db_mtime": (
            (HOME / "state.db").stat().st_mtime if (HOME / "state.db").exists() else None
        ),
        "home_state_db_wal_mtime": (
            (HOME / "state.db-wal").stat().st_mtime
            if (HOME / "state.db-wal").exists()
            else None
        ),
    }


def child_env_sample(pid: int) -> dict:
    out = {"pid": pid, "ps_eww_hermes_keys": {}, "lsof_state_db": []}
    try:
        ps = subprocess.check_output(
            ["ps", "eww", "-p", str(pid)],
            text=True,
            errors="replace",
            timeout=5,
        )
        keys = {}
        for token in ps.replace("\n", " ").split():
            if token.startswith("HERMES_") and "=" in token:
                k, _, v = token.partition("=")
                keys[k] = v
        out["ps_eww_hermes_keys"] = keys
    except Exception as exc:
        out["ps_error"] = str(exc)
    try:
        lsof = subprocess.check_output(
            ["lsof", "-p", str(pid)],
            text=True,
            errors="replace",
            timeout=10,
        )
        hits = []
        for line in lsof.splitlines():
            if "state.db" in line or "/.hermes" in line and (
                "state.db" in line or "/profiles/" in line
            ):
                hits.append(line.strip())
        out["lsof_state_db"] = hits[:40]
    except Exception as exc:
        out["lsof_error"] = str(exc)
    return out


def default_dir_created() -> bool:
    return DEFAULT_DIR.exists()


class AcpClient:
    def __init__(self, cmd: list[str], env: dict[str, str]):
        self.cmd = cmd
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            text=True,
            bufsize=1,
        )
        self.stderr_lines: list[str] = []
        self.responses: dict = {}
        self.notifications: list = []
        self.lock = threading.Condition()
        threading.Thread(target=self._drain_stderr, daemon=True).start()
        threading.Thread(target=self._reader, daemon=True).start()

    def _drain_stderr(self) -> None:
        assert self.proc.stderr is not None
        for line in self.proc.stderr:
            self.stderr_lines.append(line.rstrip())

    def _reader(self) -> None:
        assert self.proc.stdout is not None
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
                else:
                    self.notifications.append(msg)

    def send(self, id_: int, method: str, params: dict) -> None:
        assert self.proc.stdin is not None
        req = {"jsonrpc": "2.0", "id": id_, "method": method, "params": params}
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()

    def wait(self, id_: int, timeout: float) -> dict | None:
        deadline = time.time() + timeout
        with self.lock:
            while id_ not in self.responses:
                remaining = deadline - time.time()
                if remaining <= 0:
                    return None
                self.lock.wait(remaining)
            return self.responses[id_]

    def kill(self) -> int | None:
        if self.proc.poll() is None:
            try:
                if self.proc.stdin:
                    self.proc.stdin.close()
            except Exception:
                pass
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=5)
        return self.proc.returncode


def slim_result(msg: dict | None, limit: int = 4000) -> object:
    if msg is None:
        return None
    text = json.dumps(msg, default=str)
    if len(text) > limit:
        return {"truncated": True, "chars": len(text), "head": text[:limit]}
    return msg


def run_case(label: str, cmd: list[str]) -> dict:
    result = {
        "label": label,
        "argv": cmd,
        "stopped_for_default_dir": False,
        "ok_initialize": False,
        "ok_session_new": False,
        "against_home_profile": False,
        "created_profiles_default": False,
    }
    result["before"] = snapshot()
    if result["before"]["default_dir_exists"]:
        result["stopped_for_default_dir"] = True
        result["stop_reason"] = "profiles/default already existed before this argv"
        return result

    env = os.environ.copy()
    env.pop("HERMES_HOME", None)
    env.pop("HERMES_PROFILE", None)
    env["HERMES_ACP_SKIP_CONFIGURED_MCP"] = "1"

    Path(CWD).mkdir(parents=True, exist_ok=True)
    t0 = time.time()
    client = AcpClient(cmd, env)
    result["pid"] = client.proc.pid
    time.sleep(0.4)
    if default_dir_created():
        result["stopped_for_default_dir"] = True
        result["stop_reason"] = "profiles/default appeared right after spawn"
        result["created_profiles_default"] = True
        result["child_env"] = child_env_sample(client.proc.pid)
        client.kill()
        result["after"] = snapshot()
        result["elapsed_s"] = round(time.time() - t0, 2)
        result["stderr_tail"] = client.stderr_lines[-20:]
        return result

    client.send(
        1,
        "initialize",
        {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {"readTextFile": False, "writeTextFile": False}
            },
        },
    )
    init = client.wait(1, TIMEOUT_S)
    result["initialize"] = slim_result(init)
    if default_dir_created():
        result["stopped_for_default_dir"] = True
        result["stop_reason"] = "profiles/default appeared during initialize"
        result["created_profiles_default"] = True
        result["child_env"] = child_env_sample(client.proc.pid)
        client.kill()
        result["after"] = snapshot()
        result["elapsed_s"] = round(time.time() - t0, 2)
        result["stderr_tail"] = client.stderr_lines[-20:]
        return result
    if not init or "error" in init or "result" not in init:
        result["initialize_failed"] = True
        result["child_env"] = child_env_sample(client.proc.pid)
        result["stderr_tail"] = client.stderr_lines[-30:]
        client.kill()
        result["after"] = snapshot()
        result["elapsed_s"] = round(time.time() - t0, 2)
        result["created_profiles_default"] = default_dir_created()
        return result
    result["ok_initialize"] = True
    agent = (init.get("result") or {}).get("agentInfo") or {}
    result["agentInfo"] = agent

    client.send(2, "session/new", {"cwd": CWD, "mcpServers": []})
    new = client.wait(2, TIMEOUT_S)
    result["session_new"] = slim_result(new, limit=8000)
    result["child_env"] = child_env_sample(client.proc.pid)
    if default_dir_created():
        result["stopped_for_default_dir"] = True
        result["stop_reason"] = "profiles/default appeared during session/new"
        result["created_profiles_default"] = True
        client.kill()
        result["after"] = snapshot()
        result["elapsed_s"] = round(time.time() - t0, 2)
        result["stderr_tail"] = client.stderr_lines[-20:]
        return result
    if not new or "error" in new or "result" not in new:
        result["session_new_failed"] = True
        result["stderr_tail"] = client.stderr_lines[-30:]
        client.kill()
        result["after"] = snapshot()
        result["elapsed_s"] = round(time.time() - t0, 2)
        result["created_profiles_default"] = default_dir_created()
        return result
    result["ok_session_new"] = True
    res = new["result"]
    result["sessionId"] = res.get("sessionId")
    meta = res.get("_meta") or {}
    result["session_meta"] = slim_result({"_meta": meta}, limit=4000)
    result["notification_count"] = len(client.notifications)

    hermes_home = (result["child_env"].get("ps_eww_hermes_keys") or {}).get(
        "HERMES_HOME"
    )
    lsof_hits = result["child_env"].get("lsof_state_db") or []
    lsof_text = "\n".join(lsof_hits)
    home_str = str(HOME)
    default_str = str(DEFAULT_DIR)
    opened_home_db = home_str in lsof_text and "state.db" in lsof_text
    opened_named_default = default_str in lsof_text
    result["hermes_home_from_ps"] = hermes_home
    result["opened_home_state_db"] = opened_home_db
    result["opened_profiles_default"] = opened_named_default
    result["against_home_profile"] = (
        result["ok_initialize"]
        and result["ok_session_new"]
        and not default_dir_created()
        and not opened_named_default
        and (
            hermes_home in (None, "", home_str)
            or hermes_home == home_str
            or opened_home_db
        )
        and (hermes_home == home_str or opened_home_db or hermes_home is None)
    )
    # Tighten: home profile means HERMES_HOME is ~/.hermes OR lsof has ~/.hermes/state.db
    # and never profiles/default.
    result["against_home_profile"] = (
        result["ok_initialize"]
        and result["ok_session_new"]
        and not default_dir_created()
        and not opened_named_default
        and (
            (hermes_home == home_str)
            or (opened_home_db and hermes_home not in (None, "") and "profiles/" not in (hermes_home or ""))
            or (opened_home_db and hermes_home in (None, "", home_str))
        )
    )

    result["exit"] = client.kill()
    result["after"] = snapshot()
    result["elapsed_s"] = round(time.time() - t0, 2)
    result["stderr_tail"] = client.stderr_lines[-20:]
    result["created_profiles_default"] = result["after"]["default_dir_exists"]
    if result["created_profiles_default"]:
        result["against_home_profile"] = False
    return result


def main() -> int:
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)
    preflight = {
        "which_hermes": subprocess.check_output(["which", "hermes"], text=True).strip(),
        "hermes_version": subprocess.check_output(
            ["hermes", "version"], text=True, errors="replace"
        ).strip(),
        "acp_version": subprocess.check_output(
            ["hermes", "acp", "--version"], text=True, errors="replace"
        ).strip(),
        "parent_HERMES_HOME": os.environ.get("HERMES_HOME"),
        "parent_HERMES_PROFILE": os.environ.get("HERMES_PROFILE"),
        "live_snapshot_before_any_acp": snapshot(),
    }
    try:
        check = subprocess.run(
            ["hermes", "acp", "--check"],
            capture_output=True,
            text=True,
            timeout=60,
        )
        preflight["acp_check_exit"] = check.returncode
        preflight["acp_check_stdout"] = check.stdout.strip()[-500:]
        preflight["acp_check_stderr"] = check.stderr.strip()[-500:]
    except Exception as exc:
        preflight["acp_check_error"] = str(exc)

    cases = []
    for label, cmd in ARGV_CASES:
        print(f"RUN {label}: {cmd}", flush=True)
        case = run_case(label, cmd)
        cases.append(case)
        print(
            json.dumps(
                {
                    "label": label,
                    "ok_initialize": case.get("ok_initialize"),
                    "ok_session_new": case.get("ok_session_new"),
                    "against_home_profile": case.get("against_home_profile"),
                    "created_profiles_default": case.get("created_profiles_default"),
                    "stopped_for_default_dir": case.get("stopped_for_default_dir"),
                    "hermes_home_from_ps": case.get("hermes_home_from_ps"),
                    "sessionId": case.get("sessionId"),
                    "elapsed_s": case.get("elapsed_s"),
                    "stop_reason": case.get("stop_reason"),
                },
                default=str,
            ),
            flush=True,
        )
        if case.get("created_profiles_default"):
            print("STOP: profiles/default created; not continuing remaining argv", flush=True)
            break

    report = {
        "preflight": preflight,
        "cases": cases,
        "final_snapshot": snapshot(),
    }
    out = EVIDENCE_DIR / "evidence.json"
    out.write_text(json.dumps(report, indent=2, default=str) + "\n")
    print(f"WROTE {out}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
