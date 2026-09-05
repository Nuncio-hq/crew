#!/usr/bin/env python3
"""Real loopback relay proof. Requires aiohttp/coincurve; owns only its child process.

Seed the isolated community host localhost:3031, backend localhost:3030.
Run with --allow-isolated-local-relay --bin-dir /absolute/target/debug.
No existing identities are read. Test keys remain memory/environment-only.
"""
import argparse
import asyncio
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import uuid
import aiohttp

spec = importlib.util.spec_from_file_location("fixture", Path(__file__).with_name("recovery-relay-fixture.py"))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
Identity, Proxy, wait_for = fixture.Identity, fixture.Proxy, fixture.wait_for
BASE = "http://localhost:3031"

async def run(args):
    root = Path(tempfile.mkdtemp(prefix="crew-recovery-wire-")).resolve()
    root.chmod(0o700)
    print(f"Evidence: {root}", flush=True)
    proxy = Proxy(3030)
    server = await asyncio.start_server(proxy.accept, "127.0.0.1", 3031)
    clean_env = {key: value for key, value in os.environ.items() if not key.startswith("BUZZ_")}
    owner, agent = Identity(), Identity()
    owner.authorize(agent)
    evidence = {"owner": owner.pubkey, "agent": agent.pubkey, "scenarios": []}
    process = None
    async with server, aiohttp.ClientSession() as session:
        async def publish(identity, event):
            answer = await identity.request(session, BASE, "/events", event)
            assert answer.get("accepted") is True, answer
            assert answer["event_id"] == event["id"], answer
            return event["id"]
        async def cli(*parts):
            env = {**clean_env, "BUZZ_PRIVATE_KEY": owner.key.secret.hex(), "BUZZ_RELAY_URL": BASE}
            env.pop("BUZZ_AUTH_TAG", None)
            child = await asyncio.create_subprocess_exec(str(Path(args.bin_dir) / "buzz"), *parts,
                env=env, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
            out, err = await child.communicate()
            assert child.returncode == 0, err.decode()
            return json.loads(out)
        try:
            await publish(owner, owner.event(0, json.dumps({"name": "Recovery test owner"})))
            await publish(agent, agent.event(0, json.dumps({"name": "Recovery wire agent", "bot": True})))
            channel = await cli("channels", "create", "--name", "recovery-" + uuid.uuid4().hex[:8],
                                "--type", "stream", "--visibility", "open")
            evidence["channel_result"] = channel
            channel_id = channel.get("channel_id") or channel.get("id")
            assert channel_id, channel
            evidence["channel"] = channel_id
            for dirname in ["receipts", "resolution", "ledger"]:
                (root / dirname).mkdir(mode=0o700)
            (root / "release-new").touch()
            env = {**clean_env, "BUZZ_PRIVATE_KEY": agent.key.secret.hex(), "BUZZ_AUTH_TAG": json.dumps(agent.auth),
                "RUST_LOG": "buzz_acp=info", "BUZZ_RELAY_URL": BASE.replace("http:", "ws:"), "RECOVERY_FIXTURE_DIR": str(root),
                "RECOVERY_BUZZ_CLI": str(Path(args.bin_dir) / "buzz"), "BUZZ_ACP_RESOLUTION_OUTBOX_DIR": str(root / "resolution"),
                "BUZZ_ACP_RECEIPT_OUTBOX_DIR": str(root / "receipts"),
                "BUZZ_ACP_SESSION_LEDGER_DIR": str(root / "ledger")}
            log = (root / "harness.log").open("w")
            process = await asyncio.create_subprocess_exec(str(Path(args.bin_dir) / "buzz-acp"),
                "--agent-owner", owner.pubkey, "--agent-command", sys.executable,
                "--agent-args", str(Path(__file__).with_name("recovery-fake-acp.py").resolve()),
                "--idle-timeout", "90", "--max-turn-duration", "180", "--dispatch-hold-ms", "0",
                "--agent-receipts", env=env, stdout=log, stderr=log, cwd=root)
            def logs():
                text = (root / "harness.log").read_text()
                assert process.returncode is None, text[-5000:]
                return text
            def frames(method):
                path = root / "frames.jsonl"
                rows = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
                return [r for r in rows if r["direction"] == "in" and r["frame"].get("method") == method]
            await wait_for(lambda: "discovered 0 channel(s)" in logs(), "zero-channel startup")
            await cli("channels", "add-member", "--channel", channel_id, "--pubkey", agent.pubkey)
            await wait_for(lambda: "subscribing to new channel" in logs(), "dynamic membership")
            async def query_receipt(trigger):
                events = await owner.request(session, BASE, "/query", [
                    {"kinds": [46043], "#h": [channel_id], "#e": [trigger]}])
                if not isinstance(events, list) or not events:
                    return None
                assert len(events) == 1, "duplicate successful receipt"
                assert events[0]["pubkey"] == agent.pubkey, "receipt author mismatch"
                assert ["h", channel_id] in events[0]["tags"], "receipt routing mismatch"
                return events
            async def trigger(label):
                event = owner.event(9, label, [["h", channel_id], ["p", agent.pubkey]])
                (root / "reply-target.json").write_text(json.dumps({"channel": channel_id, "trigger": event["id"]}))
                return await publish(owner, event)
            first = await trigger("Complete the recovery active-prompt check")
            await wait_for(lambda: len(frames("session/prompt")) == 1, "first active prompt")
            before = proxy.upgrades
            proxy.drop()
            await wait_for(lambda: proxy.upgrades > before, "actual WebSocket reconnect")
            assert len(frames("session/cancel")) == 0
            (root / "release-prompt").touch()
            receipts = await wait_for(lambda: query_receipt(first), "accepted exact trigger receipt")
            assert len(frames("session/prompt")) == 1, "duplicate dispatch"
            replies = await owner.request(session, BASE, "/query", [
                {"kinds": [9], "authors": [agent.pubkey], "#h": [channel_id], "#e": [first]}])
            assert replies, "missing published agent reply"
            evidence["scenarios"].append({"name": "zero-add-active-reconnect", "trigger": first,
                "receipt_ids": [e["id"] for e in receipts], "reply_ids": [e["id"] for e in replies],
                "prompt_count": len(frames("session/prompt")), "cancel_count": len(frames("session/cancel"))})
            # A fresh thread forces session/new; hold it before dropping the socket.
            (root / "release-new").unlink()
            before_new = len(frames("session/new"))
            second = await trigger("Complete the recovery session-new check")
            await wait_for(lambda: len(frames("session/new")) > before_new, "blocked session/new")
            before = proxy.upgrades
            proxy.drop()
            await wait_for(lambda: proxy.upgrades > before, "session/new reconnect")
            (root / "release-new").touch()
            receipts = await wait_for(lambda: query_receipt(second), "session/new receipt")
            assert len(frames("session/prompt")) == 2
            assert not frames("session/cancel")
            evidence["scenarios"].append({"name": "session-new-reconnect", "trigger": second,
                "receipt_ids": [e["id"] for e in receipts]})
            (root / "release-prompt").unlink()
            third = await trigger("Wait for an explicit owner cancel")
            await wait_for(lambda: len(frames("session/prompt")) == 3, "third active prompt")
            cancel = owner.event(9, "!cancel", [["h", channel_id], ["p", agent.pubkey],
                ["e", third, "", "root"], ["e", third, "", "reply"]])
            await publish(owner, cancel)
            await wait_for(lambda: len(frames("session/cancel")) == 1, "explicit cancel reaches ACP")
            before = proxy.upgrades
            proxy.drop()
            await wait_for(lambda: proxy.upgrades > before, "cancel-drain reconnect")
            (root / "release-prompt").touch()
            await wait_for(lambda: (root / "cancel-completed").exists(), "cancelled response sent")
            (root / "cancel-seen").unlink()
            fourth = await trigger("Complete one fresh turn after cancellation")
            receipts = await wait_for(lambda: query_receipt(fourth), "post-cancel healthy turn")
            assert len(frames("session/prompt")) == 4, "cancelled turn was duplicated"
            assert len(frames("session/cancel")) == 1, "reconnect synthesized cancellation"
            assert not await query_receipt(third), "cancelled turn published successful receipt"
            evidence["scenarios"].append({"name": "explicit-cancel-drain-reconnect",
                "cancelled_trigger": third, "cancel_event": cancel["id"], "recovery_trigger": fourth,
                "receipt_ids": [e["id"] for e in receipts], "prompt_count": 4, "cancel_count": 1})
            evidence.update(websocket_upgrades=proxy.upgrades, forced_disconnects=proxy.drops)
            (root / "evidence.json").write_text(json.dumps(evidence, indent=2))
            print(json.dumps(evidence, indent=2), flush=True)
        finally:
            if process and process.returncode is None:
                process.terminate()
                try:
                    await asyncio.wait_for(process.wait(), timeout=10)
                except asyncio.TimeoutError:
                    process.kill()
                    await process.wait()

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-isolated-local-relay", action="store_true", required=True)
    parser.add_argument("--bin-dir", required=True)
    asyncio.run(run(parser.parse_args()))
