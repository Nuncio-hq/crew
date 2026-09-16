//! Normalize installed runtime page envelopes and bind source navigation.

use super::wiki_runtime::{WikiRuntimeFailure, WIKI_RUNTIME_OUTPUT_LIMIT};
use crew_wiki::{git_snapshot::RepoSnapshot, types::PlannedPage};
use std::ops::Range;

/// Preserve generated Markdown while resolving exact captured source links.
/// All other navigation still passes the existing strict source-range check.
pub(super) fn normalize_generated_page(
    page: &PlannedPage,
    snapshot: &RepoSnapshot,
    output: &str,
) -> Result<String, WikiRuntimeFailure> {
    let markdown = unwrap_markdown_envelope(output);
    if markdown.trim().is_empty() {
        return Err(WikiRuntimeFailure::InvalidOutput);
    }
    let mut normalized = String::with_capacity(markdown.len());
    let code_ranges = markdown_code_ranges(markdown);
    let mut cursor = 0;
    while let Some(link) = next_markdown_link(markdown, cursor, &code_ranges)? {
        normalized.push_str(&markdown[cursor..link.link_start]);
        let target = &markdown[link.target_start..link.target_end];
        normalized.push_str("](");
        if page.source_files.iter().any(|path| path == target) {
            let content = snapshot
                .contents
                .get(target)
                .ok_or(WikiRuntimeFailure::InvalidOutput)?;
            let lines = content.split_terminator('\n').count();
            if lines == 0 {
                return Err(WikiRuntimeFailure::InvalidOutput);
            }
            let mut url =
                url::Url::parse("buzz://file").map_err(|_| WikiRuntimeFailure::InvalidOutput)?;
            url.query_pairs_mut()
                .append_pair("path", target)
                .append_pair("lines", &format!("1-{lines}"));
            normalized.push_str(url.as_str());
        } else {
            normalized.push_str(target);
        }
        normalized.push(')');
        if normalized.len() as u64 > WIKI_RUNTIME_OUTPUT_LIMIT {
            return Err(WikiRuntimeFailure::InvalidOutput);
        }
        cursor = link.target_end + 1;
    }
    normalized.push_str(&markdown[cursor..]);
    if normalized.len() as u64 > WIKI_RUNTIME_OUTPUT_LIMIT {
        return Err(WikiRuntimeFailure::InvalidOutput);
    }
    validate_generated_links(page, snapshot, &normalized)?;
    Ok(normalized)
}

// Some installed CLIs wrap the complete page in a Markdown-labelled fence.
// Remove only that transport envelope; keep prose and embedded code intact.
fn unwrap_markdown_envelope(output: &str) -> &str {
    let Some((opening, body)) = output.trim().split_once('\n') else {
        return output;
    };
    if matches!(opening.trim_end(), "```markdown" | "```md") {
        return body.strip_suffix("\n```").unwrap_or(output);
    }
    output
}

/// Reject model-authored navigation outside the captured source revision.
/// The signed snapshot already carries complete source references, so a page
/// may link to an exact `buzz://file` range from its planned files; arbitrary
/// external, filesystem, or command links are never accepted from runtime
/// output. Plain prose and code blocks may still contain URL-looking text.
pub(super) fn validate_generated_links(
    page: &PlannedPage,
    snapshot: &RepoSnapshot,
    markdown: &str,
) -> Result<(), WikiRuntimeFailure> {
    let code_ranges = markdown_code_ranges(markdown);
    let mut cursor = 0;
    while let Some(link) = next_markdown_link(markdown, cursor, &code_ranges)? {
        let target = &markdown[link.target_start..link.target_end];
        let target = target
            .trim()
            .strip_prefix('<')
            .and_then(|target| target.strip_suffix('>'))
            .unwrap_or(target);
        if target.starts_with('#') {
            cursor = link.target_end + 1;
            continue;
        }
        let parsed = url::Url::parse(target).map_err(|_| WikiRuntimeFailure::InvalidOutput)?;
        if parsed.scheme() != "buzz" || parsed.host_str() != Some("file") {
            return Err(WikiRuntimeFailure::InvalidOutput);
        }
        let mut path = None;
        let mut lines = None;
        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "path" if path.is_none() => path = Some(value.into_owned()),
                "lines" if lines.is_none() => lines = Some(value.into_owned()),
                _ => return Err(WikiRuntimeFailure::InvalidOutput),
            }
        }
        let path = path.ok_or(WikiRuntimeFailure::InvalidOutput)?;
        if !page.source_files.iter().any(|source| source == &path) {
            return Err(WikiRuntimeFailure::InvalidOutput);
        }
        let content = snapshot
            .contents
            .get(&path)
            .ok_or(WikiRuntimeFailure::InvalidOutput)?;
        let line_count = if content.is_empty() {
            0
        } else {
            content.split_terminator('\n').count() as u64
        };
        let lines = lines.ok_or(WikiRuntimeFailure::InvalidOutput)?;
        let (start, end) = lines
            .split_once('-')
            .ok_or(WikiRuntimeFailure::InvalidOutput)
            .and_then(|(start, end)| {
                Ok((
                    start
                        .parse::<u64>()
                        .map_err(|_| WikiRuntimeFailure::InvalidOutput)?,
                    end.parse::<u64>()
                        .map_err(|_| WikiRuntimeFailure::InvalidOutput)?,
                ))
            })?;
        if start == 0 || end < start || end > line_count {
            return Err(WikiRuntimeFailure::InvalidOutput);
        }
        cursor = link.target_end + 1;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct MarkdownLink {
    link_start: usize,
    target_start: usize,
    target_end: usize,
}

/// Return source ranges occupied by Markdown code spans and code blocks.
/// `pulldown-cmark` handles escaped delimiters, indentation, and fence
/// boundaries; link handling below remains deliberately limited to the shape
/// already understood by the validator.
fn markdown_code_ranges(markdown: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut code_block_start = None;
    for (event, range) in pulldown_cmark::Parser::new(markdown).into_offset_iter() {
        match event {
            pulldown_cmark::Event::Code(_) => ranges.push(range),
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_)) => {
                code_block_start = Some(range.start);
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::CodeBlock) => {
                if let Some(start) = code_block_start.take() {
                    ranges.push(start..range.end);
                }
            }
            _ => {}
        }
    }
    if let Some(start) = code_block_start {
        ranges.push(start..markdown.len());
    }
    ranges.sort_by_key(|range| range.start);
    ranges
}

/// Find the next Markdown link outside code ranges. Code examples remain
/// byte-for-byte inert while prose links retain the existing strict
/// `buzz://file` validation.
fn next_markdown_link(
    markdown: &str,
    from: usize,
    code_ranges: &[Range<usize>],
) -> Result<Option<MarkdownLink>, WikiRuntimeFailure> {
    let mut cursor = from;
    while let Some(offset) = markdown[cursor..].find("](") {
        let index = cursor + offset;
        let code_index = code_ranges.partition_point(|range| range.end <= index);
        if let Some(range) = code_ranges
            .get(code_index)
            .filter(|range| range.start <= index)
        {
            cursor = range.end;
            continue;
        }
        let target_start = index + 2;
        let target_end = markdown[target_start..]
            .find(')')
            .map(|offset| target_start + offset)
            .ok_or(WikiRuntimeFailure::InvalidOutput)?;
        return Ok(Some(MarkdownLink {
            link_start: index,
            target_start,
            target_end,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crew_wiki::{git_snapshot::RepoSnapshot, types::PlannedPage};
    use std::collections::BTreeMap;

    fn page_snapshot(content: &str) -> (PlannedPage, RepoSnapshot) {
        (
            PlannedPage {
                slug: "overview".into(),
                title: "Overview".into(),
                section: "overview".into(),
                source_files: vec!["src/lib.rs".into()],
            },
            RepoSnapshot {
                commit: "deadbeef".into(),
                branch: "main".into(),
                source_revision: "git:deadbeef".into(),
                files: vec!["src/lib.rs".into()],
                contents: BTreeMap::from([("src/lib.rs".into(), content.into())]),
                ..RepoSnapshot::default()
            },
        )
    }

    #[test]
    fn source_links_are_skipped_in_inline_and_fenced_code() {
        let (page, snapshot) = page_snapshot("first\nsecond\n");
        let markdown = concat!(
            "See [source](src/lib.rs).\n\n",
            "Inline `[literal](src/lib.rs)` and ``[literal-two](src/lib.rs)``.\n\n",
            "Escaped \\` marker [escaped](src/lib.rs) `.\n\n",
            "```rust\n[literal-three](src/lib.rs)\n[literal-outside](../private.txt)\n````\n\n",
            "~~~~markdown\n[literal-four](src/lib.rs)\n~~~~\n",
        );
        let normalized = normalize_generated_page(&page, &snapshot, markdown).expect("page");
        assert_eq!(
            normalized
                .matches("buzz://file?path=src%2Flib.rs&lines=1-2")
                .count(),
            2
        );
        assert!(normalized.contains("`[literal](src/lib.rs)`"));
        assert!(normalized.contains("``[literal-two](src/lib.rs)``"));
        assert!(normalized
            .contains("Escaped \\` marker [escaped](buzz://file?path=src%2Flib.rs&lines=1-2) `."));
        assert!(normalized.contains("```rust\n[literal-three](src/lib.rs)"));
        assert!(normalized.contains("[literal-outside](../private.txt)\n````"));
        assert!(normalized.contains("~~~~markdown\n[literal-four](src/lib.rs)\n~~~~"));
        assert!(normalize_generated_page(
            &page,
            &snapshot,
            "```text\n[private](../private.txt)\n```\n"
        )
        .is_ok());
    }
}
