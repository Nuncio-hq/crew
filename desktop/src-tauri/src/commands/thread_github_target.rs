use std::path::Path;

use super::thread_workspace_git::git_output_at;

/// Resolve the `origin` remote of a project checkout into a `gh --repo` target.
///
/// Without an explicit target, `gh` resolves the repository from the checkout's
/// remotes and prefers one named `upstream` over `origin`. A fork checkout
/// therefore reads and mutates pull requests on the parent repository. Pinning
/// `origin` matches the push target already used when deleting a thread branch.
///
/// Returns `None` when the checkout has no `origin` remote or its URL is not a
/// recognizable `owner/name` location; callers then fall back to the `gh`
/// default resolution, which is correct for a single-remote checkout.
pub(crate) async fn origin_repo_target(repository_path: &Path) -> Option<String> {
    let url = git_output_at(repository_path, ["remote", "get-url", "origin"])
        .await
        .ok()?;
    repo_target_from_remote_url(url.trim())
}

/// Parse a git remote URL into the `[HOST/]OWNER/REPO` form `gh --repo` accepts.
///
/// The host is kept for non-`github.com` remotes so an enterprise checkout is
/// not silently redirected to github.com.
fn repo_target_from_remote_url(url: &str) -> Option<String> {
    let url = url.trim();
    let scheme_relative = url.split_once("://").map(|(_, rest)| rest);
    let without_scheme = scheme_relative.unwrap_or(url);
    // Drop any `user[:password]@` credential prefix from either remote form.
    let authority = without_scheme
        .rsplit_once('@')
        .map_or(without_scheme, |(_, rest)| rest);
    // `scp`-like remotes separate host and path with `:`; URL forms use `/`.
    let (host, path) = if scheme_relative.is_some() {
        authority.split_once('/')?
    } else {
        authority.split_once(':')?
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    let owner = segments.next()?;
    let name = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    // A URL authority may carry a port; `gh` expects the bare host.
    let host = host.split(':').next().filter(|host| !host.is_empty())?;
    if host.eq_ignore_ascii_case("github.com") {
        Some(format!("{owner}/{name}"))
    } else {
        Some(format!("{host}/{owner}/{name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::repo_target_from_remote_url;

    #[test]
    fn parses_github_remote_url_forms() {
        for url in [
            "https://github.com/Nuncio-hq/crew.git",
            "https://github.com/Nuncio-hq/crew",
            "https://github.com/Nuncio-hq/crew/",
            "https://oscar@github.com/Nuncio-hq/crew.git",
            "git@github.com:Nuncio-hq/crew.git",
            "ssh://git@github.com/Nuncio-hq/crew.git",
            "ssh://git@github.com:22/Nuncio-hq/crew.git",
            "git://github.com/Nuncio-hq/crew.git",
        ] {
            assert_eq!(
                repo_target_from_remote_url(url).as_deref(),
                Some("Nuncio-hq/crew"),
                "failed to parse {url}"
            );
        }
    }

    #[test]
    fn keeps_host_for_non_github_remotes() {
        assert_eq!(
            repo_target_from_remote_url("https://ghe.example.com/Nuncio-hq/crew.git").as_deref(),
            Some("ghe.example.com/Nuncio-hq/crew")
        );
        assert_eq!(
            repo_target_from_remote_url("git@ghe.example.com:Nuncio-hq/crew.git").as_deref(),
            Some("ghe.example.com/Nuncio-hq/crew")
        );
    }

    #[test]
    fn rejects_urls_without_an_owner_and_name() {
        for url in [
            "",
            "   ",
            "/Users/oscar/mirrors/crew.git",
            "../crew",
            "https://github.com/Nuncio-hq",
            "https://github.com/Nuncio-hq/crew/extra",
            "git@github.com:crew.git",
        ] {
            assert_eq!(
                repo_target_from_remote_url(url),
                None,
                "unexpectedly parsed {url}"
            );
        }
    }
}
