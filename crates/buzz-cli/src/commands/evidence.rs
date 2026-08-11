use std::{fmt, str::FromStr};

use nostr::{EventBuilder, Tag};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceKind {
    TestRun,
    Metrics,
    BeforeAfterVisual,
    DiffStat,
}

impl EvidenceKind {
    pub const ALL: [Self; 4] = [
        Self::TestRun,
        Self::Metrics,
        Self::BeforeAfterVisual,
        Self::DiffStat,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TestRun => "test-run",
            Self::Metrics => "metrics",
            Self::BeforeAfterVisual => "before-after-visual",
            Self::DiffStat => "diff-stat",
        }
    }

    pub fn append_tag(self, builder: EventBuilder) -> Result<EventBuilder, String> {
        let tag = Tag::parse(["crew-evidence", self.as_str()])
            .map_err(|error| format!("invalid evidence tag: {error}"))?;
        Ok(builder.tag(tag).dedup_tags())
    }
}

impl fmt::Display for EvidenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EvidenceKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "test-run" => Ok(Self::TestRun),
            "metrics" => Ok(Self::Metrics),
            "before-after-visual" => Ok(Self::BeforeAfterVisual),
            "diff-stat" => Ok(Self::DiffStat),
            _ => Err(format!(
                "invalid evidence kind '{value}'; expected one of: {}",
                Self::ALL
                    .iter()
                    .map(|kind| kind.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};

    #[test]
    fn parses_only_canonical_wire_values() {
        for kind in EvidenceKind::ALL {
            assert_eq!(kind.as_str().parse::<EvidenceKind>(), Ok(kind));
        }
        for value in ["TEST-RUN", "test_run", " Test-run", "unknown"] {
            assert!(value.parse::<EvidenceKind>().is_err(), "{value}");
        }
    }

    #[test]
    fn appends_exactly_one_evidence_tag_to_built_event() {
        let keys = Keys::generate();
        for kind in EvidenceKind::ALL {
            let event = kind
                .append_tag(EventBuilder::new(Kind::TextNote, "evidence"))
                .expect("canonical evidence tag")
                .sign_with_keys(&keys)
                .expect("static builder signs");
            let matching = event
                .tags
                .iter()
                .filter(|tag| {
                    tag.as_slice().first().map(|value| value.as_str()) == Some("crew-evidence")
                })
                .collect::<Vec<_>>();
            assert_eq!(matching.len(), 1);
            assert_eq!(matching[0].as_slice()[1].as_str(), kind.as_str());
        }
    }
}
