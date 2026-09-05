"""Loopback-only signed Nostr HTTP and disconnectable TCP proxy helpers."""
import asyncio
import base64
import hashlib
import json
import time
import uuid
from coincurve import PrivateKey

class Identity:
    def __init__(self):
        self.key = PrivateKey()
        self.pubkey = self.key.public_key_xonly.format().hex()
        self.auth = None

    def event(self, kind, content="", tags=None):
        tags = list(tags or [])
        if self.auth:
            tags.append(self.auth)
        created = int(time.time())
        preimage = [0, self.pubkey, created, kind, tags, content]
        digest = hashlib.sha256(json.dumps(preimage, separators=(",", ":"), ensure_ascii=False).encode()).digest()
        return dict(id=digest.hex(), pubkey=self.pubkey, created_at=created, kind=kind,
                    tags=tags, content=content, sig=self.key.sign_schnorr(digest).hex())

    def authorize(self, agent):
        digest = hashlib.sha256(f"nostr:agent-auth:{agent.pubkey}:".encode()).digest()
        agent.auth = ["auth", self.pubkey, "", self.key.sign_schnorr(digest).hex()]

    async def request(self, session, base, path, payload):
        body = json.dumps(payload, separators=(",", ":")).encode()
        url = base + path
        auth = self.event(27235, tags=[["u", url], ["method", "POST"], ["nonce", str(uuid.uuid4())],
                         ["payload", hashlib.sha256(body).hexdigest()]])
        token = base64.b64encode(json.dumps(auth).encode()).decode()
        headers = {"Authorization": "Nostr " + token, "Content-Type": "application/json"}
        if self.auth:
            headers["x-auth-tag"] = json.dumps(self.auth)
        async with session.post(url, data=body, headers=headers) as response:
            text = await response.text()
            if response.status != 200:
                raise RuntimeError(f"{path}: HTTP {response.status}: {text[:1000]}")
            return json.loads(text)

class Proxy:
    def __init__(self, target_port):
        self.target_port = target_port
        self.websockets = set()
        self.upgrades = 0
        self.drops = 0

    async def accept(self, reader, writer):
        upstream = None
        try:
            first = await reader.readuntil(b"\r\n\r\n")
            source, upstream = await asyncio.open_connection("127.0.0.1", self.target_port)
            websocket = b"upgrade: websocket" in first.lower()
            if websocket:
                self.websockets.add(writer)
                self.upgrades += 1
            upstream.write(first)
            await upstream.drain()
            async def pump(src, dst):
                while chunk := await src.read(65536):
                    dst.write(chunk)
                    await dst.drain()
            tasks = [asyncio.create_task(pump(reader, upstream)), asyncio.create_task(pump(source, writer))]
            _, pending = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
            for task in pending:
                task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
        except (ConnectionError, asyncio.IncompleteReadError):
            pass
        finally:
            self.websockets.discard(writer)
            writer.close()
            if upstream:
                upstream.close()

    def drop(self):
        assert self.websockets, "no actual WebSocket exists to interrupt"
        self.drops += len(self.websockets)
        for writer in list(self.websockets):
            writer.transport.abort()

async def wait_for(predicate, label, timeout=45):
    end = asyncio.get_running_loop().time() + timeout
    while asyncio.get_running_loop().time() < end:
        value = predicate()
        if asyncio.iscoroutine(value):
            value = await value
        if value:
            return value
        await asyncio.sleep(0.05)
    raise AssertionError("timed out waiting for " + label)
