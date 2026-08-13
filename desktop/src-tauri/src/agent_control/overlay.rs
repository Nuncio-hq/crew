//! Overlay payloads (ghost cursor / tap ripple). Kind 24201 family.

use serde::{Deserialize, Serialize};

use super::protocol::Instrument;
use super::snapshot::Bounds;

pub const KIND_AGENT_INSTRUMENT_OVERLAY: u32 = 24201;
pub const RIPPLE_MS: u64 = 800;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayFrame {
    pub kind: u32,
    pub ripple_ms: u64,
    pub instrument: String,
    pub tool: String,
    pub channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<OverlayTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<OverlayPoint>,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayTarget {
    #[serde(rename = "ref")]
    pub r#ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPoint {
    pub x: f64,
    pub y: f64,
}

impl OverlayFrame {
    pub fn click(
        instrument: Instrument,
        tool: &str,
        channel_id: &str,
        r#ref: Option<&str>,
        point: Option<(f64, f64)>,
        bounds: Option<Bounds>,
        at_ms: u64,
    ) -> Self {
        Self {
            kind: KIND_AGENT_INSTRUMENT_OVERLAY,
            ripple_ms: RIPPLE_MS,
            instrument: match instrument {
                Instrument::Browser => "browser".into(),
                Instrument::Sim => "sim".into(),
            },
            tool: tool.to_string(),
            channel_id: channel_id.to_string(),
            target: r#ref.map(|r| OverlayTarget {
                r#ref: r.to_string(),
                bounds,
            }),
            point: point.map(|(x, y)| OverlayPoint { x, y }),
            at_ms,
        }
    }
}

#[derive(Clone, Default)]
pub struct OverlayLog {
    pub frames: Vec<OverlayFrame>,
}

impl OverlayLog {
    pub fn push(&mut self, frame: OverlayFrame) {
        self.frames.push(frame);
        if self.frames.len() > 32 {
            self.frames.remove(0);
        }
    }

    #[allow(dead_code)]
    pub fn latest_for(&self, channel_id: &str) -> Option<&OverlayFrame> {
        self.frames
            .iter()
            .rev()
            .find(|f| f.channel_id == channel_id)
    }
}
