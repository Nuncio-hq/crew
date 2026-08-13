use super::types::ForgeCheckLogTail;

const TAIL_LINES: usize = 50;
const MAX_TAILS: usize = 12;

/// Parse `gh run view --log-failed` TSV into bounded per-step tails.
pub fn parse_log_failed(output: &str) -> Vec<ForgeCheckLogTail> {
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    for line in output.lines() {
        let (job, step, rest) = split_tsv(line);
        if rest.trim() == "…" {
            continue;
        }
        match groups.last_mut() {
            Some((current_job, current_step, lines))
                if current_job == &job && current_step == &step =>
            {
                lines.push(strip_timestamp(rest).to_string());
            }
            _ => groups.push((job, step, vec![strip_timestamp(rest).to_string()])),
        }
    }
    let mut tails: Vec<ForgeCheckLogTail> = groups
        .into_iter()
        .map(|(job, step, lines)| {
            let truncated = lines.len() > TAIL_LINES;
            let start = lines.len().saturating_sub(TAIL_LINES);
            ForgeCheckLogTail {
                job,
                step,
                lines: lines[start..].to_vec(),
                truncated,
            }
        })
        .collect();
    // Prefer groups that contain an error marker when we have too many.
    if tails.len() > MAX_TAILS {
        tails.sort_by_key(|tail| {
            !tail.lines.iter().any(|line| {
                line.contains("##[error]") || line.to_ascii_lowercase().contains("error")
            })
        });
        tails.truncate(MAX_TAILS);
    }
    tails
}

fn split_tsv(line: &str) -> (String, String, &str) {
    let Some((job, rest)) = line.split_once('\t') else {
        return ("unknown".to_string(), "unknown".to_string(), line);
    };
    match rest.split_once('\t') {
        Some((step, body)) => (job.to_string(), step.to_string(), body),
        None => (job.to_string(), "unknown".to_string(), rest),
    }
}

fn strip_timestamp(line: &str) -> &str {
    let line = line.trim_start_matches('\u{feff}');
    // `2026-08-13T11:52:40.1194845Z message`
    if line.len() > 20
        && line.as_bytes().get(4) == Some(&b'-')
        && line.as_bytes().get(10) == Some(&b'T')
    {
        if let Some(rest) = line.split_once('Z') {
            return rest.1.trim();
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tails_excerpt_keeps_error_line() {
        let raw = include_str!("fixtures/log-failed.txt");
        let tails = parse_log_failed(raw);
        assert!(!tails.is_empty());
        assert!(tails
            .iter()
            .any(|tail| { tail.lines.iter().any(|line| line.contains("##[error]")) }));
        for tail in &tails {
            assert!(tail.lines.len() <= TAIL_LINES);
        }
    }

    #[test]
    fn bounds_last_n_lines() {
        let mut blob = String::new();
        for i in 0..80 {
            blob.push_str(&format!("job\tstep\t2026-01-01T00:00:00Z line {i}\n"));
        }
        let tails = parse_log_failed(&blob);
        assert_eq!(tails.len(), 1);
        assert!(tails[0].truncated);
        assert_eq!(tails[0].lines.len(), TAIL_LINES);
        assert!(tails[0].lines[0].contains("line 30"));
    }
}
