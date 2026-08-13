//! `.crew/wiki.json` steering — mirrors `.devin/wiki.json`.

use crate::error::WikiError;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Optional steering file. All keys optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct WikiSteering {
    /// Free-form notes prepended to the overview prompt.
    #[serde(default)]
    pub repo_notes: Option<String>,
    /// Explicit page list — bypasses clustering when non-empty.
    #[serde(default)]
    pub pages: Option<Vec<SteeringPage>>,
    /// Output language. Default English.
    #[serde(default)]
    pub language: Option<String>,
}

/// One steered page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SteeringPage {
    /// Page slug.
    pub slug: String,
    /// Title.
    #[serde(default)]
    pub title: Option<String>,
    /// Section id.
    #[serde(default)]
    pub section: Option<String>,
    /// Source files (optional; planner fills from glob if empty).
    #[serde(default)]
    pub source_files: Option<Vec<String>>,
}

/// Load `.crew/wiki.json` if present. Missing file is `None`, not an error.
pub fn load_steering(repo_root: &Path) -> Option<WikiSteering> {
    let path = repo_root.join(".crew").join("wiki.json");
    let bytes = fs::read_to_string(path).ok()?;
    parse_steering(&bytes).ok()
}

/// Parse steering JSON.
pub fn parse_steering(raw: &str) -> Result<WikiSteering, WikiError> {
    serde_json::from_str(raw).map_err(|e| WikiError::InvalidSteering(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_object_is_valid() {
        let s = parse_steering("{}").expect("empty");
        assert!(s.pages.is_none());
        assert!(s.language.is_none());
    }

    #[test]
    fn language_and_pages_honored() {
        let s = parse_steering(r#"{"language":"ja","pages":[{"slug":"overview","title":"概要"}]}"#)
            .expect("ok");
        assert_eq!(s.language.as_deref(), Some("ja"));
        assert_eq!(s.pages.as_ref().map(|p| p.len()), Some(1));
    }
}
