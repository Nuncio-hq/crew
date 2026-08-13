//! Child-webview probe + WebviewWindow fallback (huddle precedent).
//!
//! Spike 0027: this Linux VM cannot exercise macOS `Window::add_child`.
//! The probe reports true only on macOS; commands fall back to `WebviewWindow`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserBackend {
    Child,
    Window,
}

/// Runtime probe: child webviews are compiled for macOS only. Linux/CI and
/// a failed AppKit probe use the window fallback behind the same TS API.
pub fn probe_child_webview() -> bool {
    cfg!(target_os = "macos")
}

pub fn backend() -> BrowserBackend {
    if probe_child_webview() {
        BrowserBackend::Child
    } else {
        BrowserBackend::Window
    }
}

pub fn window_label(channel_id: &str) -> String {
    let compact: String = channel_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    format!("crew-browser-{compact}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_ci_uses_window_fallback() {
        if cfg!(target_os = "macos") {
            assert!(probe_child_webview());
            assert_eq!(backend(), BrowserBackend::Child);
        } else {
            assert!(!probe_child_webview());
            assert_eq!(backend(), BrowserBackend::Window);
        }
    }

    #[test]
    fn label_is_stable_per_channel() {
        let a = window_label("9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50");
        let b = window_label("9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50");
        assert_eq!(a, b);
        assert!(a.starts_with("crew-browser-"));
    }
}
