//! Stable snapshot refs `e1..eN` and digest.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::protocol::ControlError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotNode {
    #[serde(rename = "ref")]
    pub r#ref: String,
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub actionable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SnapshotNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub source: String,
    pub snapshot_digest: String,
    pub nodes: Vec<SnapshotNode>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Frame untrusted page content so engines treat it as data, not instructions.
pub fn frame_source(origin: &str) -> String {
    format!("[content from {origin}]")
}

pub fn mint_refs(mut nodes: Vec<SnapshotNode>) -> Vec<SnapshotNode> {
    let mut n = 1u32;
    assign_refs(&mut nodes, &mut n);
    nodes
}

fn assign_refs(nodes: &mut [SnapshotNode], n: &mut u32) {
    for node in nodes {
        node.r#ref = format!("e{n}");
        *n += 1;
        assign_refs(&mut node.children, n);
    }
}

pub fn digest_of(nodes: &[SnapshotNode]) -> String {
    let mut hasher = Sha256::new();
    walk_digest(nodes, &mut hasher);
    hex::encode(hasher.finalize())
}

fn walk_digest(nodes: &[SnapshotNode], hasher: &mut Sha256) {
    for node in nodes {
        hasher.update(node.r#ref.as_bytes());
        hasher.update(0u8.to_be_bytes());
        hasher.update(node.role.as_bytes());
        hasher.update(1u8.to_be_bytes());
        hasher.update(node.name.as_bytes());
        hasher.update(2u8.to_be_bytes());
        walk_digest(&node.children, hasher);
    }
}

pub fn build_snapshot(origin: &str, nodes: Vec<SnapshotNode>) -> Snapshot {
    let nodes = mint_refs(nodes);
    let snapshot_digest = digest_of(&nodes);
    Snapshot {
        source: frame_source(origin),
        snapshot_digest,
        nodes,
        truncated: false,
        cursor: None,
    }
}

pub fn find_node<'a>(nodes: &'a [SnapshotNode], r#ref: &str) -> Option<&'a SnapshotNode> {
    for node in nodes {
        if node.r#ref == r#ref {
            return Some(node);
        }
        if let Some(found) = find_node(&node.children, r#ref) {
            return Some(found);
        }
    }
    None
}

pub fn require_digest(current: &str, provided: Option<&str>) -> Result<(), ControlError> {
    match provided {
        None => Ok(()),
        Some(got) if got == current => Ok(()),
        Some(_) => Err(ControlError::stale_ref(current)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<SnapshotNode> {
        vec![SnapshotNode {
            r#ref: String::new(),
            role: "button".into(),
            name: "Save".into(),
            value: None,
            actionable: true,
            bounds: Some(Bounds {
                x: 10.0,
                y: 20.0,
                w: 80.0,
                h: 24.0,
            }),
            children: vec![],
        }]
    }

    #[test]
    fn two_snapshots_match() {
        let a = build_snapshot("https://app.local", tree());
        let b = build_snapshot("https://app.local", tree());
        assert_eq!(a.snapshot_digest, b.snapshot_digest);
        assert_eq!(a.nodes[0].r#ref, "e1");
        assert_eq!(b.nodes[0].r#ref, "e1");
        assert!(a.source.contains("https://app.local"));
    }

    #[test]
    fn mutation_changes_digest() {
        let a = build_snapshot("https://app.local", tree());
        let mut next = tree();
        next.push(SnapshotNode {
            r#ref: String::new(),
            role: "link".into(),
            name: "Docs".into(),
            value: None,
            actionable: true,
            bounds: None,
            children: vec![],
        });
        let b = build_snapshot("https://app.local", next);
        assert_ne!(a.snapshot_digest, b.snapshot_digest);
        assert!(require_digest(&b.snapshot_digest, Some(&a.snapshot_digest)).is_err());
    }
}
