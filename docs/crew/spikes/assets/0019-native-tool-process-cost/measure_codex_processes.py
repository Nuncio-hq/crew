#!/usr/bin/env python3
"""Credential-free Codex ACP process RSS and startup measurement."""

from __future__ import annotations

import json
import os
import signal
import subprocess
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parent
OUTPUT = ROOT / "codex-process-measurements.json"


def proc_stat(pid: int) -> tuple[int, int, int]:
    status = Path(f"/proc/{pid}/status").read_text()
    rss = next(
        int(line.split()[1]) for line in status.splitlines() if line.startswith("VmRSS:")
    )
    fds = len(list(Path(f"/proc/{pid}/fd").iterdir()))
    cpu = os.times()
    return rss, fds, int(cpu.user * 1000)


def measure(count: int) -> dict[str, object]:
    processes: list[subprocess.Popen[bytes]] = []
    start = time.monotonic_ns()
    for _ in range(count):
        processes.append(
            subprocess.Popen(
                ["codex-acp"],
                stdin=subprocess.PIPE,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        )
    spawn_ms = (time.monotonic_ns() - start) / 1_000_000
    time.sleep(1)
    samples = []
    for process in processes:
        try:
            rss_kib, fds, cpu_ms = proc_stat(process.pid)
        except (FileNotFoundError, StopIteration):
            continue
        samples.append({"pid": process.pid, "rss_kib": rss_kib, "fds": fds, "cpu_ms": cpu_ms})
    for process in processes:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
    for process in processes:
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    rss_values = [sample["rss_kib"] for sample in samples]
    return {
        "processes_requested": count,
        "processes_sampled": len(samples),
        "launch_ms_all": round(spawn_ms, 2),
        "rss_kib_total": sum(rss_values),
        "rss_kib_mean": round(sum(rss_values) / len(rss_values), 2) if rss_values else None,
        "rss_kib_max": max(rss_values) if rss_values else None,
        "fd_total": sum(sample["fds"] for sample in samples),
        "fd_mean": round(sum(sample["fds"] for sample in samples) / len(samples), 2)
        if samples
        else None,
        "samples": samples,
    }


def main() -> None:
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    result = {
        "engine": "codex-acp",
        "version": "1.1.14",
        "host": {
            "kernel": os.uname().release,
            "cpu_count": os.cpu_count(),
            "fd_limit": 65536,
            "memory_kib": int(
                next(
                    line.split()[1]
                    for line in Path("/proc/meminfo").read_text().splitlines()
                    if line.startswith("MemTotal:")
                )
            ),
            "swap": "0 B",
        },
        "sampling": "one second after launch; stdin remains open; own child PIDs only",
        "runs": [measure(count) for count in (1, 8, 24, 48)],
    }
    OUTPUT.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: value for key, value in result.items() if key != "runs"}, indent=2))
    for run in result["runs"]:
        print(
            f"{run['processes_requested']}: sampled={run['processes_sampled']} "
            f"launch_ms={run['launch_ms_all']} rss_mean_kib={run['rss_kib_mean']} "
            f"rss_total_kib={run['rss_kib_total']} fd_mean={run['fd_mean']}"
        )


if __name__ == "__main__":
    main()
