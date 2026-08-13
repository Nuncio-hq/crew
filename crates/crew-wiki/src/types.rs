//! Shared plan / page types.

use serde::{Deserialize, Serialize};

/// Two-level wiki plan (TOC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPlan {
    /// Output language (`en` default).
    pub language: String,
    /// Ordered sections.
    pub sections: Vec<PlannedSection>,
}

/// One TOC section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedSection {
    /// Stable section id (slug).
    pub id: String,
    /// Display title.
    pub title: String,
    /// Pages in display order.
    pub pages: Vec<PlannedPage>,
}

/// One planned page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedPage {
    /// Page slug (`d` suffix).
    pub slug: String,
    /// Display title.
    pub title: String,
    /// Parent section id.
    pub section: String,
    /// Source files this page is generated from.
    pub source_files: Vec<String>,
}
