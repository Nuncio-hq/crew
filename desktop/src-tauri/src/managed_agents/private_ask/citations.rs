//! The answer-citation fence.
//!
//! An answer's citations used to be the request's grounding, copied verbatim.
//! That said nothing about the answer: a runtime that cited a file it was never
//! given — or invented one — produced exactly the same citation list as one that
//! stayed inside the snapshot. This module reads the citations the *answer*
//! claims and admits only the ones the verified snapshot can account for.
//!
//! The format is fixed by [`CITATION_PREFIX`] and stated in the prompt, so the
//! runtime and the parser cannot drift: a line beginning with the prefix names
//! exactly one source path. Anything else in the markdown is prose.

use super::{GroundedSource, PrivateAskFailure};

/// The one line shape a citation may take. It is a Markdown footnote
/// definition, so an answer rendered anywhere else still reads correctly.
pub(super) const CITATION_PREFIX: &str = "[^cite]: ";

/// Upper bound on citation lines read from one answer.
///
/// Grounding is already bounded, and a duplicate-heavy answer must not make the
/// parser do unbounded work before it refuses. It is deliberately larger than
/// any plausible grounding list so a legitimate answer is never truncated into
/// a refusal.
const MAX_CITATION_LINES: usize = 1024;

/// Resolve an answer's own citations against the grounding it was given.
///
/// Every cited path must be a path in `grounding`; a citation to anything else
/// is [`PrivateAskFailure::InvalidOutput`] and the whole answer is refused,
/// because an answer that cites a file it was not given has either invented it
/// or reached one, and neither is an answer this feature returns.
///
/// An answer with no citation line is accepted with no citations: not every
/// answer cites, and refusing silence would make an honest "the source does not
/// say" look hostile.
///
/// Duplicates collapse. The result follows the grounding order rather than the
/// order the runtime happened to print, so the citation list is a property of
/// the verified snapshot and not of the answer's formatting.
pub(super) fn resolve(
    markdown: &str,
    grounding: &[GroundedSource],
) -> Result<Vec<GroundedSource>, PrivateAskFailure> {
    let mut cited: Vec<&str> = Vec::new();
    for line in markdown.lines() {
        let Some(rest) = line.trim_start().strip_prefix(CITATION_PREFIX) else {
            continue;
        };
        if cited.len() >= MAX_CITATION_LINES {
            return Err(PrivateAskFailure::InvalidOutput);
        }
        let path = rest.trim();
        if path.is_empty() {
            return Err(PrivateAskFailure::InvalidOutput);
        }
        cited.push(path);
    }
    // Exact equality against the grounded path. A prefix or suffix match would
    // make `src/a` account for a citation of `src/ab`, which is precisely the
    // foreign citation this fence exists to refuse.
    if let Some(foreign) = cited
        .iter()
        .find(|path| !grounding.iter().any(|source| source.path == **path))
    {
        let _ = foreign;
        return Err(PrivateAskFailure::InvalidOutput);
    }
    Ok(grounding
        .iter()
        .filter(|source| cited.iter().any(|path| *path == source.path))
        .cloned()
        .collect())
}

/// The instruction that makes an answer's citations parseable.
///
/// It lives beside the parser so the two cannot describe different formats.
pub(super) fn citation_instruction() -> String {
    format!(
        "Cite every source you used. Put each citation on its own line at the end, exactly as `{CITATION_PREFIX}<path>`, using a path from the grounding above and nothing else. Cite nothing if you used nothing.\n\n"
    )
}

#[cfg(test)]
#[path = "citation_tests.rs"]
mod tests;
