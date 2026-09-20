//! Hostile-input and evidence tests for the loopback CONNECT proxy.
//!
//! These are platform-independent: the parser and the observation record are
//! the parts that decide what is allowed and what may be certified, and both
//! must behave identically on every lane. The real-process proof that the
//! Seatbelt policy admits only this proxy's port lives in the macOS-only
//! containment tests.

use super::*;

fn head(line: &str) -> Vec<u8> {
    format!("{line}\r\n\r\n").into_bytes()
}

#[test]
fn only_connect_to_the_provider_on_443_parses() {
    let target = parse_connect_target(&head("CONNECT api.anthropic.com:443 HTTP/1.1"))
        .expect("a well-formed CONNECT");
    assert_eq!(target.host, "api.anthropic.com");
    assert_eq!(target.port, 443);

    // Case and one trailing dot are the same host.
    let target = parse_connect_target(&head("CONNECT API.Anthropic.Com.:443 HTTP/1.1"))
        .expect("case-insensitive host");
    assert_eq!(target.host, "api.anthropic.com");
}

/// Removing the method/version check in `parse_connect_target` turns the proxy
/// into an open forwarder for anything that opens a socket.
#[test]
fn anything_that_is_not_a_connect_request_is_refused() {
    for line in [
        "GET http://api.anthropic.com/ HTTP/1.1",
        "connect api.anthropic.com:443 HTTP/1.1",
        "CONNECT api.anthropic.com:443",
        "CONNECT api.anthropic.com:443 HTTP/1.1 extra",
        "CONNECT  HTTP/1.1",
        "",
        "POST / HTTP/1.1",
    ] {
        assert_eq!(
            parse_connect_target(&head(line)).unwrap_err(),
            RefusalReason::NotConnect,
            "accepted {line:?}"
        );
    }
}

/// Removing the `host.parse::<IpAddr>()` refusal lets a child skip the host
/// check entirely by dialling the provider's address, or anyone else's.
#[test]
fn ip_literal_and_bracketed_targets_are_refused() {
    assert_eq!(
        parse_connect_target(&head("CONNECT 127.0.0.1:443 HTTP/1.1")).unwrap_err(),
        RefusalReason::ForeignHost
    );
    assert_eq!(
        parse_connect_target(&head("CONNECT [::1]:443 HTTP/1.1")).unwrap_err(),
        RefusalReason::ForeignHost
    );
    // `host:443:443` parses as the host `api.anthropic.com:443`, which is not a
    // host at all — the refusal is about the host, not the port.
    assert_eq!(
        parse_connect_target(&head("CONNECT api.anthropic.com:443:443 HTTP/1.1")).unwrap_err(),
        RefusalReason::ForeignHost
    );
}

#[test]
fn a_target_without_a_port_or_with_a_bad_port_is_refused() {
    for line in [
        "CONNECT api.anthropic.com HTTP/1.1",
        "CONNECT api.anthropic.com: HTTP/1.1",
        "CONNECT api.anthropic.com:abc HTTP/1.1",
        "CONNECT api.anthropic.com:99999 HTTP/1.1",
    ] {
        assert_eq!(
            parse_connect_target(&head(line)).unwrap_err(),
            RefusalReason::ForeignPort,
            "accepted {line:?}"
        );
    }
}

/// A non-ASCII request line is refused before any UTF-8 decision is made.
#[test]
fn a_non_ascii_request_line_is_refused_without_decoding() {
    let mut raw = b"CONNECT api.anthropic.com".to_vec();
    raw.extend_from_slice(&[0xff, 0xfe]);
    raw.extend_from_slice(b":443 HTTP/1.1\r\n\r\n");
    assert_eq!(
        parse_connect_target(&raw).unwrap_err(),
        RefusalReason::NotConnect
    );
}

/// `ProviderHost::parse` is the only way to name a destination, and it refuses
/// everything that would make the host check vacuous.
#[test]
fn a_provider_host_must_be_a_plain_dns_name() {
    assert_eq!(
        ProviderHost::parse("api.anthropic.com").unwrap().as_str(),
        "api.anthropic.com"
    );
    assert_eq!(
        ProviderHost::parse(" API.ANTHROPIC.COM. ")
            .unwrap()
            .as_str(),
        "api.anthropic.com"
    );
    for value in [
        "",
        "localhost",
        "127.0.0.1",
        "::1",
        "api.anthropic.com:443",
        "api.anthropic.com/path",
        "https://api.anthropic.com",
        "api..anthropic.com",
        ".anthropic.com",
        "api.anthropic.com\u{0}",
    ] {
        assert_eq!(
            ProviderHost::parse(value).unwrap_err(),
            PrivateAskFailure::EgressBoundUnverified,
            "accepted {value:?}"
        );
    }
}

fn observation(
    accepted: Vec<&str>,
    refused: Vec<(&str, RefusalReason)>,
    direct: u32,
) -> EgressObservation {
    EgressObservation::from_parts(
        "api.anthropic.com".into(),
        41234,
        accepted.into_iter().map(str::to_owned).collect(),
        refused
            .into_iter()
            .map(|(target, reason)| (target.to_owned(), reason))
            .collect(),
        0,
        false,
        direct,
    )
}

/// The evidence rule, stated as a test. Removing the `!self.refused.is_empty()`
/// clause in `bounds_egress` makes an untested proxy indistinguishable from a
/// proxy that refused a real hostile target.
#[test]
fn an_observation_bounds_egress_only_with_a_real_refusal_and_no_leak() {
    assert!(observation(
        vec!["api.anthropic.com"],
        vec![("127.0.0.1:41999", RefusalReason::ForeignHost)],
        0,
    )
    .bounds_egress());

    // Nothing was ever refused: the proxy was never shown to say no.
    assert!(!observation(vec!["api.anthropic.com"], vec![], 0).bounds_egress());

    // A foreign host was accepted.
    assert!(!observation(
        vec!["api.anthropic.com", "evil.test"],
        vec![("x", RefusalReason::ForeignHost)],
        0,
    )
    .bounds_egress());

    // The child reached a listener directly, bypassing the proxy.
    assert!(!observation(
        vec!["api.anthropic.com"],
        vec![("x", RefusalReason::ForeignHost)],
        1,
    )
    .bounds_egress());

    // A malformed request is not a destination refusal. Anything that opens a
    // socket produces one, so accepting it as proof would let a port scan
    // certify a proxy that was never asked for a foreign host.
    assert!(!observation(
        vec!["api.anthropic.com"],
        vec![("GET / HTTP/1.1", RefusalReason::NotConnect)],
        0,
    )
    .bounds_egress());

    // Neither is a busy proxy.
    assert!(!observation(
        vec!["api.anthropic.com"],
        vec![("api.anthropic.com:443", RefusalReason::TunnelCap)],
        0,
    )
    .bounds_egress());

    // A wrong port is a destination refusal and does count.
    assert!(observation(
        vec!["api.anthropic.com"],
        vec![("api.anthropic.com:80", RefusalReason::ForeignPort)],
        0,
    )
    .bounds_egress());

    // A truncated record is not a complete set of destinations.
    let truncated = EgressObservation::from_parts(
        "api.anthropic.com".into(),
        41234,
        vec!["api.anthropic.com".into()],
        vec![("x".into(), RefusalReason::ForeignHost)],
        0,
        true,
        0,
    );
    assert!(!truncated.bounds_egress());

    // A port of zero was never bound.
    let unbound = EgressObservation::from_parts(
        "api.anthropic.com".into(),
        0,
        vec![],
        vec![("x".into(), RefusalReason::ForeignHost)],
        0,
        false,
        0,
    );
    assert!(!unbound.bounds_egress());
}

/// A near-miss host is a different host. Suffix matching would accept it.
#[test]
fn a_lookalike_host_is_not_the_provider() {
    assert!(!observation(
        vec!["api.anthropic.com.evil.test"],
        vec![("x", RefusalReason::ForeignHost)],
        0,
    )
    .bounds_egress());
}

/// The record is bounded: a hostile child that opens thousands of connections
/// cannot grow this process, and the resulting truncation is refused rather
/// than silently accepted.
#[test]
fn the_refusal_record_is_bounded_and_marks_truncation() {
    let mut record = ProxyRecord::default();
    for index in 0..(RECORDED_TARGET_LIMIT + 5) {
        record.refuse(format!("evil-{index}.test:443"), RefusalReason::ForeignHost);
    }
    assert_eq!(record.refused.len(), RECORDED_TARGET_LIMIT);
    assert!(record.truncated);
}

/// Control bytes in an attacker-chosen target never reach the evidence.
#[test]
fn a_recorded_target_is_stripped_of_control_bytes() {
    let mut record = ProxyRecord::default();
    record.refuse("evil\u{0}\n.test:443".into(), RefusalReason::ForeignHost);
    assert_eq!(record.refused[0].target(), "evil.test:443");
    assert_eq!(record.refused[0].reason(), RefusalReason::ForeignHost);
}

/// The child's proxy environment is numeric and leaves no bypass list. A named
/// proxy URL would need a resolver the sandbox does not grant, and an inherited
/// `NO_PROXY` would route around the only egress the run has.
#[test]
fn the_child_environment_names_a_numeric_loopback_proxy_and_no_bypass() {
    let proxy = EgressProxy::start(ProviderHost::parse("api.anthropic.com").unwrap())
        .expect("loopback proxy");
    let port = proxy.port();
    assert!(port != 0);
    let env = proxy.child_env();
    let url = format!("http://127.0.0.1:{port}");
    for name in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY"] {
        assert_eq!(
            env.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str()),
            Some(url.as_str())
        );
    }
    assert_eq!(
        env.iter()
            .find(|(key, _)| key == "NO_PROXY")
            .map(|(_, value)| value.as_str()),
        Some("")
    );
    assert!(proxy.is_listening());
}

/// A live proxy refuses a foreign CONNECT, records it with its reason, and
/// keeps serving afterwards. Removing the host check in `handle_client` makes
/// the refusal list empty, which `bounds_egress` then refuses to certify.
#[test]
fn a_live_proxy_refuses_a_foreign_host_and_keeps_serving() {
    use std::io::{Read, Write};

    let proxy = EgressProxy::start(ProviderHost::parse("api.anthropic.com").unwrap())
        .expect("loopback proxy");
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, proxy.port()));

    let mut response = String::new();
    let mut client =
        std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).expect("connect");
    client
        .write_all(b"CONNECT evil.test:443 HTTP/1.1\r\n\r\n")
        .expect("write");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let _ = client.read_to_string(&mut response);
    assert!(response.starts_with("HTTP/1.1 403"), "saw {response:?}");

    // Still serving: a second foreign attempt is refused the same way.
    let mut second =
        std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).expect("connect");
    second
        .write_all(b"GET http://evil.test/ HTTP/1.1\r\n\r\n")
        .expect("write");
    second
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut second_response = String::new();
    let _ = second.read_to_string(&mut second_response);
    assert!(second_response.starts_with("HTTP/1.1 403"));

    let observation = proxy.observe(0);
    assert!(observation.accepted().is_empty(), "nothing may be accepted");
    let reasons: Vec<RefusalReason> = observation
        .refused()
        .iter()
        .map(RefusedTarget::reason)
        .collect();
    assert!(reasons.contains(&RefusalReason::ForeignHost));
    assert!(reasons.contains(&RefusalReason::NotConnect));
    // Nothing ever reached the provider, so refusals alone certify nothing:
    // a proxy that blocks everything must not look like a bounded one.
    assert!(!observation.bounds_egress());
}

/// The proxy stops with its owner. Nothing is left listening on the loopback
/// port after the run, so a later process cannot inherit this run's egress.
#[test]
fn the_proxy_stops_when_its_owner_is_dropped() {
    let proxy = EgressProxy::start(ProviderHost::parse("api.anthropic.com").unwrap())
        .expect("loopback proxy");
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, proxy.port()));
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).is_ok());
    drop(proxy);

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the proxy port is still accepting after its owner was dropped");
}

/// A connection that sends nothing asks for nothing, and must leave no trace in
/// the record. Removing the `head.is_empty()` return in `handle_client` lets a
/// liveness probe — including the proxy's own shutdown self-connect —
/// manufacture the refusal that `bounds_egress` looks for.
#[test]
fn a_connection_that_sends_nothing_records_nothing() {
    let proxy = EgressProxy::start(ProviderHost::parse("api.anthropic.com").unwrap())
        .expect("loopback proxy");
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, proxy.port()));
    for _ in 0..3 {
        let client = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5))
            .expect("connect");
        drop(client);
    }
    // Give the accept loop a moment to have handled all three.
    std::thread::sleep(Duration::from_millis(300));
    let observation = proxy.observe(0);
    assert!(
        observation.refused().is_empty(),
        "an empty request must record nothing: {:?}",
        observation.refused()
    );
    assert!(observation.accepted().is_empty());
}

/// The smallest well-formed ClientHello carrying one SNI host-name entry. The
/// probe program builds the same bytes in Perl; both are read by the same
/// parser, which is what keeps them from drifting.
fn client_hello_bytes(host: &str) -> Vec<u8> {
    let mut entry = vec![0_u8];
    entry.extend_from_slice(&(host.len() as u16).to_be_bytes());
    entry.extend_from_slice(host.as_bytes());
    let mut list = (entry.len() as u16).to_be_bytes().to_vec();
    list.extend_from_slice(&entry);
    let mut extension = 0_u16.to_be_bytes().to_vec();
    extension.extend_from_slice(&(list.len() as u16).to_be_bytes());
    extension.extend_from_slice(&list);

    let mut body = 0x0303_u16.to_be_bytes().to_vec();
    body.extend_from_slice(&[0_u8; 32]);
    body.push(0);
    body.extend_from_slice(&2_u16.to_be_bytes());
    body.extend_from_slice(&[0x13, 0x01]);
    body.push(1);
    body.push(0);
    body.extend_from_slice(&(extension.len() as u16).to_be_bytes());
    body.extend_from_slice(&extension);

    let mut handshake = vec![0x01];
    handshake.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    handshake.extend_from_slice(&body);

    let mut record = vec![0x16, 0x03, 0x01];
    record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
    record.extend_from_slice(&handshake);
    record
}

/// The parser reads the outer server name, normalizes it, and refuses anything
/// it cannot walk without leaving the buffer.
///
/// Production line: `client_hello_sni`. Every length in a ClientHello is
/// attacker-chosen, so a truncation at any of them must return `None` rather
/// than panic — which is what the truncation sweep below asserts.
#[test]
fn a_client_hello_yields_its_normalized_server_name_or_nothing() {
    let hello = client_hello_bytes("API.Anthropic.Com.");
    assert_eq!(
        client_hello_sni(&hello).as_deref(),
        Some("api.anthropic.com"),
        "case and a trailing dot are the same host"
    );

    // Not TLS at all, and a handshake that is not a ClientHello.
    assert_eq!(client_hello_sni(b"CONNECT api.anthropic.com:443"), None);
    assert_eq!(client_hello_sni(&[]), None);
    let mut not_hello = client_hello_bytes("api.anthropic.com");
    not_hello[5] = 0x02;
    assert_eq!(client_hello_sni(&not_hello), None);

    // A hostile length at any offset must be refused, never walked off the end.
    let hello = client_hello_bytes("api.anthropic.com");
    for cut in 0..hello.len() {
        assert_eq!(
            client_hello_sni(&hello[..cut]),
            None,
            "a record truncated at {cut} must not parse"
        );
    }

    // A host that is not a DNS name at all is not a name this proxy can match.
    assert_eq!(client_hello_sni(&client_hello_bytes("evil..test")), None);
    assert_eq!(client_hello_sni(&client_hello_bytes("")), None);
}

/// A CONNECT to the provider whose handshake asks for somebody else is
/// refused, and nothing is forwarded.
///
/// Production line: the `client_hello_sni` match in `handle_client`. Remove it
/// and a shared front end that terminates the provider's name serves any site
/// the child names in its handshake, with the CONNECT line still saying the
/// provider.
#[test]
fn a_handshake_that_fronts_a_foreign_server_is_refused() {
    use std::io::{Read, Write};

    let proxy = EgressProxy::start(ProviderHost::parse("api.anthropic.com").unwrap())
        .expect("loopback proxy");
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, proxy.port()));

    let mut client =
        std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).expect("connect");
    client
        .write_all(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n")
        .expect("write");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut status = [0_u8; 39];
    client.read_exact(&mut status).expect("status line");
    assert!(String::from_utf8_lossy(&status).starts_with("HTTP/1.1 200"));

    client
        .write_all(&client_hello_bytes("evil.test"))
        .expect("fronting handshake");
    // The proxy closes the tunnel rather than forwarding a byte.
    let mut forwarded = Vec::new();
    let _ = client.read_to_end(&mut forwarded);
    assert!(
        forwarded.is_empty(),
        "nothing may come back through a refused tunnel"
    );

    let observation = proxy.observe(0);
    assert!(
        observation
            .refused()
            .iter()
            .any(|refused| refused.reason() == RefusalReason::SniMismatch),
        "the refusal must be recorded as evidence, with its own reason"
    );
}

/// A connection past the concurrency cap is dropped at accept, before a task
/// exists to hold a descriptor through the head-read timeout.
///
/// Production line: the `tasks.len() >= MAX_CONCURRENT_TUNNELS` check in
/// `accept_loop`. With the cap applied only after the request head is parsed —
/// where it used to be — every one of these idle clients stays connected for
/// the full ten-second head timeout and this read does not return.
#[test]
fn a_connection_past_the_cap_is_dropped_before_it_can_hold_a_descriptor() {
    use std::io::Read;

    let proxy = EgressProxy::start(ProviderHost::parse("api.anthropic.com").unwrap())
        .expect("loopback proxy");
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, proxy.port()));

    // Exactly the cap, all silent: each occupies a task sitting in the
    // head-read timeout.
    let idle: Vec<std::net::TcpStream> = (0..MAX_CONCURRENT_TUNNELS)
        .map(|_| {
            std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).expect("connect")
        })
        .collect();

    let mut extra =
        std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).expect("connect");
    extra
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut bytes = Vec::new();
    let started = std::time::Instant::now();
    let read = extra.read_to_end(&mut bytes);
    assert!(
        read.is_ok(),
        "the extra connection must be closed, not held"
    );
    assert!(bytes.is_empty(), "a dropped connection is told nothing");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the cap must be applied at accept, not after the head-read timeout"
    );

    // Nothing was manufactured into the evidence: these clients asked for
    // nothing, so there is nothing to refuse.
    assert!(proxy.observe(0).refused().is_empty());
    drop(idle);
}
