# Security Policy

## Reporting a Vulnerability

**Do not report security vulnerabilities through a public GitHub issue, pull
request, discussion, or other public channel.** This checkout is
[NuncioCrew](https://github.com/Nuncio-hq/crew); reports about this fork must not
be sent to Block's upstream support address by default.

### Private reporting setup is pending

GitHub private vulnerability reporting is **not enabled** for `Nuncio-hq/crew`
(verified 2026-09-05). No alternative Crew private contact is documented here.
A maintainer must enable the repository's private-reporting form or publish an
approved private contact before this repository can offer a reporting route.
Do not put vulnerability details in a public issue while that setup is pending.

The upstream private-advisory policy is retained as the intended reporting
workflow; this source upgrade does not enable a GitHub setting or promise a
response deadline. Once a verified private route is available, reports should
include:

- A description of the vulnerability and its potential impact
- Steps to reproduce or a proof-of-concept, if available
- The affected versions or commit range
- Suggested mitigations, if known

Avoid accessing or modifying other people's data and do not disrupt production
systems. Coordinate disclosure privately once the maintainers establish the
reporting route.

---

## Supported Versions

| Version | Supported |
|---------|-----------|
| `main` (latest) | ✅ Active |
| Previous releases | ⚠️ Best-effort; upgrade recommended |

Buzz is pre-1.0. We do not maintain long-term support branches at this stage.
All security fixes land on `main` first.

---

## Security Design Principles

### Authentication — NIP-42

Every connection to the relay must authenticate via
[NIP-42](https://github.com/nostr-protocol/nips/blob/master/42.md)
challenge/response before writing events. The relay sends a random challenge;
the client signs a `kind:22242` event containing the challenge and the relay
URL, proving possession of the private key.

REST endpoints authenticate via
[NIP-98](https://github.com/nostr-protocol/nips/blob/master/98.md) HTTP Auth —
the client signs a `kind:27235` event containing the request URL and method.
The relay verifies the Schnorr signature and extracts the pubkey.

### Authorization — Channel Membership as the Gate

Channel membership is the **only** access control mechanism. There are no
separate ACL lists or capability taxonomies. If a principal (human or agent)
is a member of a channel, they can read and write to it. If they are not a
member, the relay rejects their requests — even if they are authenticated.

Private channels are invisible to non-members: they do not appear in channel
listings, and subscription filters for private channel events return nothing
unless the subscriber is a member.

### Append-Only Audit Log

All events are written to a tamper-evident audit log (`buzz-audit`). Each
log entry is chained to the previous one via a SHA-256 hash chain. Because the
chain is keyless, it is tamper-evident but not tamper-resistant: it detects
accidental corruption or single-row edits, but an attacker with database write
access can recompute the entire chain after editing. The audit log is designed
for SOX-grade compliance and eDiscovery.

### Desktop Secret Storage — OS Keyring

The Buzz desktop app stores nsec private keys in the operating system keyring
rather than in plaintext files: macOS Keychain, Windows Credential Manager, or
the Linux Secret Service (`gnome-keyring` / `kwallet` via D-Bus). This covers
both the human identity key and every managed-agent key.

On first launch after upgrading, existing plaintext keys are migrated into the
keyring: the key is imported, read back to verify the round-trip, and only then
is the plaintext deleted. Migration runs only when the keyring is reachable —
if the backend is unavailable that session, the app keeps reading from the
plaintext file and does **not** migrate, so a transient outage cannot resurrect
a rotated key from a leftover file.

When no keyring backend is available (headless Linux with no Secret Service, for
example), keys fall back to a `0o600` owner-only file. The `BUZZ_PRIVATE_KEY`
environment variable, when set, always takes precedence over both stores — this
is how harnessed agents and CI receive their identity.

### Input Validation

- All UUIDs (channel IDs, workflow IDs) are validated at API boundaries before
  use in database queries.
- Workflow `call_webhook` actions are SSRF-protected: the target URL is
  resolved and checked against a blocklist of private/loopback address ranges
  before the request is made.
- Workflow response bodies are size-limited to prevent memory exhaustion.
- `evalexpr` condition evaluation is sandboxed and timeout-bounded.
- Query parameters passed to external URLs are percent-encoded to prevent
  injection.

### Transport Security

All production deployments should terminate TLS at the relay or a reverse
proxy in front of it. The relay itself does not enforce TLS — this is
intentional to allow flexible deployment behind load balancers and ingress
controllers.

### Dependency Management

We use `cargo audit` in CI to scan for known vulnerabilities in dependencies.
`#![deny(unsafe_code)]` is enforced across all crates — no unsafe Rust.

---

## Disclosure Policy

We follow [coordinated disclosure](https://en.wikipedia.org/wiki/Coordinated_vulnerability_disclosure).
Once a private reporting route is established, use it to coordinate validation,
remediation, disclosure, and reporter attribution. This document does not claim
that a vulnerability has been fixed or released.
