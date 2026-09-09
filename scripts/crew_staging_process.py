"""Bounded subprocess I/O with no shell and no raw credential diagnostics."""

import json
import os
import selectors
import signal
import subprocess
import time

from crew_staging_config import Refused


def run(argv, limits, *, env=None, output=None, byte_limit=None):
    """Capture bounded output or stream to a protected baseline file."""
    cap = byte_limit or limits["output_bytes"]
    process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               stdin=subprocess.DEVNULL, env=env, start_new_session=True)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, "stdout")
    selector.register(process.stderr, selectors.EVENT_READ, "stderr")
    counts = {"stdout": 0, "stderr": 0}
    captured = bytearray()
    deadline = time.monotonic() + limits["timeout_seconds"]
    try:
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise Refused("command timeout; staging remains unavailable, retry after inspection")
            for key, _ in selector.select(min(remaining, 0.1)):
                block = os.read(key.fileobj.fileno(), 65536)
                if not block:
                    selector.unregister(key.fileobj)
                    continue
                counts[key.data] += len(block)
                bound = cap if key.data == "stdout" else limits["output_bytes"]
                if counts[key.data] > bound:
                    raise Refused("command output limit exceeded; staging remains unavailable")
                if key.data == "stdout":
                    if output:
                        output.write(block)
                    else:
                        captured.extend(block)
        code = process.wait(timeout=max(0.01, deadline - time.monotonic()))
        if code:
            raise Refused("command failed (exit " + str(code) + "); private output suppressed")
        return bytes(captured)
    finally:
        selector.close()
        # Descendants retaining pipes also receive the containment signal.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        process.stdout.close()
        process.stderr.close()


def run_json(argv, config, env=None):
    try:
        return json.loads(run(argv, config["limits"], env=env))
    except (ValueError, TypeError):
        raise Refused("inspection returned invalid JSON") from None
