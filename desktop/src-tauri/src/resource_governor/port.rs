//! Free-port allocation and `$PORT` expansion for Crew-owned dev servers.

use std::net::TcpListener;

/// Pick a free localhost TCP port. `preferred` is tried first; if it is busy
/// the OS assigns the next free port and the caller should surface a conflict
/// note.
pub fn allocate_port(preferred: Option<u16>) -> Result<(u16, Option<String>), String> {
    if let Some(port) = preferred {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            let chosen = listener.local_addr().map_err(|e| e.to_string())?.port();
            drop(listener);
            return Ok((chosen, None));
        }
        let (fallback, _) = allocate_port(None)?;
        let note = format!("port {port} busy → using {fallback}");
        return Ok((fallback, Some(note)));
    }
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("allocate port: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    Ok((port, None))
}

/// Substitute `$PORT` in a canvas `tooling.devServer.command`.
pub fn expand_command(command: &str, port: u16) -> String {
    command.replace("$PORT", &port.to_string())
}

/// True when PTY output matches the owner-authored ready pattern, or when the
/// pattern is empty and the port already accepts connections.
pub fn output_matches_ready(output: &str, ready_pattern: &str) -> bool {
    let pattern = ready_pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    output.contains(pattern)
}

pub fn port_is_open(port: u16) -> bool {
    std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_port_placeholder() {
        assert_eq!(
            expand_command("pnpm dev --port $PORT", 5173),
            "pnpm dev --port 5173"
        );
    }

    #[test]
    fn ready_pattern_matches_vite_banner() {
        assert!(output_matches_ready(
            "  ➜  Local:   http://localhost:5173/\n",
            "Local:"
        ));
        assert!(!output_matches_ready("starting…", "Local:"));
    }

    #[test]
    fn allocates_a_bindable_port() {
        let (port, note) = allocate_port(None).expect("port");
        assert!(port > 0);
        assert!(note.is_none());
    }

    #[test]
    fn busy_preferred_port_falls_back_with_note() {
        let hold = TcpListener::bind(("127.0.0.1", 0)).expect("hold");
        let busy = hold.local_addr().expect("addr").port();
        let (fallback, note) = allocate_port(Some(busy)).expect("fallback");
        assert_ne!(fallback, busy);
        let expected = format!("port {busy} busy → using {fallback}");
        assert_eq!(note.as_deref(), Some(expected.as_str()));
    }
}
