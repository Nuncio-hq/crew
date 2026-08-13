//! Dev-server helpers on top of labeled Buzz Term sessions.

use super::port::{expand_command, output_matches_ready};

pub const MAX_CRASH_RESTARTS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedServer {
    pub expanded_command: String,
    pub port: u16,
    pub port_note: Option<String>,
}

pub fn prepare_command(command: &str, port: u16, port_note: Option<String>) -> PreparedServer {
    PreparedServer {
        expanded_command: expand_command(command, port),
        port,
        port_note,
    }
}

pub fn should_restart(crash_count: u32) -> bool {
    crash_count < MAX_CRASH_RESTARTS
}

pub fn is_ready(log: &str, ready_pattern: &str) -> bool {
    output_matches_ready(log, ready_pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_up_to_three() {
        assert!(should_restart(0));
        assert!(should_restart(2));
        assert!(!should_restart(3));
    }

    #[test]
    fn ready_from_pattern() {
        assert!(is_ready("Local: http://localhost:5173", "Local:"));
    }
}
