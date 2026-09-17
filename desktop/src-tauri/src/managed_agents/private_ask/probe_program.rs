//! The program a production capability probe actually runs.
//!
//! # Why this, and not the selected runtime
//!
//! Every dimension [`super::capability::PrivateAskCapability::from_probe`] can
//! mint — tool isolation, read bound, egress bound, process containment, no
//! side effects — is a property of the *envelope*: the Seatbelt policy text
//! plus the desktop's own loopback proxy. The kernel denies a write because of
//! the policy, not because of who attempted it. Asking the real `claude` or
//! `hermes` to attempt the hostile acts would instead be a self-report by the
//! subject under test, which is exactly the shape this feature's evidence rules
//! reject; it would also need a model call and a provider credential *before*
//! either is certified, and it would answer differently on every run.
//!
//! The probe therefore runs a program the desktop ships, under byte-identical
//! policy text to the production launch. That is possible with **zero policy
//! delta** because the containment profile's read allow-list already contains
//! `/usr` and `/bin` unconditionally, for every runtime: `/usr/bin/perl` is
//! present on every macOS and starts under the policy built for any runtime's
//! directory. Nothing is widened to accommodate the probe — if it were, the
//! rebuilt-profile comparison in `from_probe` would no longer match the
//! production text, and the receipt would be worthless.
//!
//! NAMED LIMIT: this proves the envelope denies the effect. It does not prove
//! the runtime has no in-process capability the envelope permits. That is the
//! Seatbelt model rather than a gap introduced here.
//!
//! # What it attempts
//!
//! One write outside the run root, one read outside the run root, one direct
//! TCP connect to a desktop-owned control listener, one `CONNECT` through the
//! proxy to a foreign host, one `CONNECT` through the proxy to the configured
//! provider, one DNS lookup, and one `fork`. Then it prints a single-line
//! marker carrying the desktop's own per-run nonce and exits zero.
//!
//! Only the *attempts* are self-reported. Every *effect* is measured by the
//! desktop: the sentinel digest, the control listener's accept count, the
//! proxy's own record and the descendant lock are all observations this
//! process makes, not claims the probe makes about itself.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Marker prefixing the probe's single result line. A probe that exits zero
/// without it produced no evidence; silence is never read as success.
pub(super) const PROBE_MARKER: &str = "CREW-PRIVATE-ASK-PROBE-V1 ";

/// Environment variable names the probe reads. They are fixed here so the
/// program and its launcher cannot drift apart.
pub(super) const ENV_NONCE: &str = "CREW_PROBE_NONCE";
pub(super) const ENV_SENTINEL: &str = "CREW_PROBE_SENTINEL";
pub(super) const ENV_CONTROL: &str = "CREW_PROBE_CONTROL";
pub(super) const ENV_PROXY: &str = "CREW_PROBE_PROXY";
pub(super) const ENV_PROVIDER: &str = "CREW_PROBE_PROVIDER";
pub(super) const ENV_FOREIGN: &str = "CREW_PROBE_FOREIGN";
pub(super) const ENV_LOCK: &str = "CREW_PROBE_LOCK";

/// The interpreter the probe runs under. It is inside the policy's fixed read
/// allow-list for every runtime, which is the whole reason this design needs no
/// policy delta.
pub(super) const PROBE_INTERPRETER: &str = "/usr/bin/perl";

/// The probe program source.
///
/// Kept as one literal rather than a file on disk beside the binary: the digest
/// of these exact bytes is recorded in the receipt, and a receipt whose probe
/// program is not this one is ignored. A file that shipped alongside the app
/// could be edited without the binary changing.
pub(super) const PROBE_PROGRAM: &str = r#"#!/usr/bin/perl
use strict;
use warnings;
use Socket;
use Fcntl qw(:flock);

my $nonce    = $ENV{CREW_PROBE_NONCE}    // '';
my $sentinel = $ENV{CREW_PROBE_SENTINEL} // '';
my $control  = $ENV{CREW_PROBE_CONTROL}  // '';
my $proxy    = $ENV{CREW_PROBE_PROXY}    // '';
my $provider = $ENV{CREW_PROBE_PROVIDER} // '';
my $foreign  = $ENV{CREW_PROBE_FOREIGN}  // '';
my $lock     = $ENV{CREW_PROBE_LOCK}     // '';

sub jbool { return $_[0] ? 'true' : 'false'; }

# 1. Write outside the run root. The bytes would be appended to a file the
#    desktop owns and digests either side of this run.
my $write_denied = 1;
if ($sentinel ne '') {
    if (open(my $out, '>>', $sentinel)) {
        print $out "probe-escaped\n";
        close $out;
        $write_denied = 0;
    }
}

# 2. Read outside the run root.
my $read_denied = 1;
if ($sentinel ne '') {
    if (open(my $in, '<', $sentinel)) {
        local $/;
        my $bytes = <$in>;
        close $in;
        $read_denied = 0 if defined $bytes && length $bytes > 0;
    }
}

sub dial {
    my ($endpoint) = @_;
    my ($host, $port) = split(/:/, $endpoint, 2);
    return undef unless defined $host && defined $port && $port =~ /^[0-9]+$/;
    my $address = inet_aton($host);
    return undef unless defined $address;
    socket(my $socket, PF_INET, SOCK_STREAM, getprotobyname('tcp')) or return undef;
    unless (connect($socket, pack_sockaddr_in($port, $address))) {
        close $socket;
        return undef;
    }
    return $socket;
}

# 3. Direct TCP to a listener the desktop controls, bypassing the proxy.
my $direct_denied = 1;
if ($control ne '') {
    my $socket = dial($control);
    if (defined $socket) {
        $direct_denied = 0;
        syswrite($socket, "probe-direct\n");
        close $socket;
    }
}

# 4/5. CONNECT through the proxy, to a foreign host and to the provider.
sub connect_through_proxy {
    my ($target) = @_;
    return 'unreachable' if $proxy eq '' || $target eq '';
    my $socket = dial($proxy);
    return 'unreachable' unless defined $socket;
    syswrite($socket, "CONNECT $target:443 HTTP/1.1\r\nHost: $target:443\r\n\r\n");
    my $reply = '';
    my $chunk = '';
    while (length($reply) < 256) {
        my $read = sysread($socket, $chunk, 64);
        last unless defined $read && $read > 0;
        $reply .= $chunk;
        last if $reply =~ /\r\n/;
    }
    close $socket;
    return 'silent' if $reply eq '';
    return $reply =~ m{^HTTP/1\.[01] 200} ? 'accepted' : 'refused';
}

my $foreign_result  = connect_through_proxy($foreign);
my $provider_result = connect_through_proxy($provider);

# 6. Name resolution. The policy leaves no resolver reachable, so this must
#    fail; a probe that resolves a name has found an egress the proxy does not
#    see.
my $dns_denied = defined(gethostbyname('api.anthropic.com')) ? 0 : 1;

# 7. Fork. A descendant that survives holds an exclusive lock on a file inside
#    the run root; the desktop takes that lock after the bounded owner finished,
#    which is a liveness observation that needs no PID.
my $fork_denied = 1;
if ($lock ne '') {
    my $pid = fork();
    if (defined $pid) {
        $fork_denied = 0;
        if ($pid == 0) {
            if (open(my $handle, '>', $lock)) {
                flock($handle, LOCK_EX | LOCK_NB);
                sleep 120;
            }
            exit 0;
        }
    }
}

printf(
    "CREW-PRIVATE-ASK-PROBE-V1 {\"nonce\":\"%s\",\"parentPid\":%d,\"writeOutsideDenied\":%s,\"readOutsideDenied\":%s,\"directConnectDenied\":%s,\"foreignConnect\":\"%s\",\"providerConnect\":\"%s\",\"dnsDenied\":%s,\"forkDenied\":%s}\n",
    $nonce,
    getppid(),
    jbool($write_denied),
    jbool($read_denied),
    jbool($direct_denied),
    $foreign_result,
    $provider_result,
    jbool($dns_denied),
    jbool($fork_denied),
);
exit 0;
"#;

/// Digest of the probe program the running binary ships.
///
/// Recorded in every receipt. A receipt written by a different build — one
/// whose probe attempted fewer things, or attempted them differently — no
/// longer describes the evidence this build requires, and is ignored rather
/// than reinterpreted.
pub(super) fn probe_program_digest() -> String {
    hex::encode(Sha256::digest(PROBE_PROGRAM.as_bytes()))
}

/// Write the probe program into one run root and return its path.
///
/// It goes inside the run root because that is the only tree the policy lets
/// the child read; nothing else on the machine needs to be readable for it.
pub(super) fn write_probe_program(run_root: &Path) -> Result<PathBuf, super::PrivateAskFailure> {
    use std::io::Write;

    let path = run_root.join("probe.pl");
    let mut file =
        std::fs::File::create(&path).map_err(|_| super::PrivateAskFailure::InvalidState)?;
    file.write_all(PROBE_PROGRAM.as_bytes())
        .map_err(|_| super::PrivateAskFailure::InvalidState)?;
    file.sync_all()
        .map_err(|_| super::PrivateAskFailure::InvalidState)?;
    Ok(path)
}

/// What the probe reported about the attempts it made.
///
/// Every field here is an *attempt* the probe is entitled to report on, because
/// only the probe can see the return value of its own syscall. No field here is
/// an effect: effects are measured by the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProbeMarker {
    /// The child's own view of who started it. Read from the child rather than
    /// from `std::process::id()` on this side: a run that had been reparented
    /// onto the employee's harness would still look independent if the desktop
    /// answered that question for it.
    pub(super) parent_pid: u32,
    pub(super) write_outside_denied: bool,
    pub(super) read_outside_denied: bool,
    pub(super) direct_connect_denied: bool,
    pub(super) foreign_connect_refused: bool,
    pub(super) provider_connect_reached_proxy: bool,
    pub(super) dns_denied: bool,
    pub(super) fork_denied: bool,
}

/// Parse the probe's single marker line, requiring this run's own nonce.
///
/// A missing marker, a truncated marker, non-UTF-8 output, or a marker carrying
/// a different nonce all yield `None`. A retained trace that could be replayed
/// from another run is the thing the nonce exists to stop, so the nonce check is
/// not advisory.
pub(super) fn parse_marker(stdout: &[u8], expected_nonce: &str) -> Option<ProbeMarker> {
    let text = std::str::from_utf8(stdout).ok()?;
    let line = text
        .lines()
        .find_map(|line| line.trim().strip_prefix(PROBE_MARKER))?;
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if expected_nonce.is_empty() || value.get("nonce")?.as_str()? != expected_nonce {
        return None;
    }
    let flag = |name: &str| value.get(name).and_then(serde_json::Value::as_bool);
    let text_of = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let parent_pid = u32::try_from(value.get("parentPid")?.as_u64()?).ok()?;
    // PID zero is not a parent. Refusing it here keeps an absent lineage out of
    // the evidence rather than letting it read as a value.
    if parent_pid == 0 {
        return None;
    }
    Some(ProbeMarker {
        parent_pid,
        write_outside_denied: flag("writeOutsideDenied")?,
        read_outside_denied: flag("readOutsideDenied")?,
        direct_connect_denied: flag("directConnectDenied")?,
        // "refused" is the proxy answering something other than 200. A foreign
        // target the proxy *accepted* is an escape; a target the child could not
        // reach the proxy for at all proves nothing and is not a refusal.
        foreign_connect_refused: text_of("foreignConnect")? == "refused",
        // The provider leg must have reached the proxy. Whether the proxy could
        // then dial the provider is a network condition, not a policy fact, so
        // both "accepted" and "refused" count as reaching it — but "unreachable"
        // does not, because a proxy nothing reached records nothing.
        provider_connect_reached_proxy: matches!(
            text_of("providerConnect")?.as_str(),
            "accepted" | "refused" | "silent"
        ),
        dns_denied: flag("dnsDenied")?,
        fork_denied: flag("forkDenied")?,
    })
}

#[cfg(test)]
#[path = "probe_program_tests.rs"]
mod tests;
