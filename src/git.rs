//! Git operations via libgit2 — replaces `Command::new("git")` exec calls.
//!
//! Provides `Git2Repo` for local repo queries (tags, branch, SHA, repo root)
//! and `fetch_tags` for authenticated remote tag fetching via `auth-git2`.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Wrapper around `git2::Repository` for common git operations.
pub struct Git2Repo {
    repo: git2::Repository,
}

impl Git2Repo {
    /// Open a repository by discovering it from the given path upward.
    pub fn open(path: &Path) -> Result<Self> {
        let repo =
            git2::Repository::discover(path).context("Not a git repository (or any parent)")?;
        Ok(Self { repo })
    }

    /// Open a repository from the current working directory.
    pub fn open_cwd() -> Result<Self> {
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        Self::open(&cwd)
    }

    /// Get the tag reachable from HEAD, equivalent to `git describe --tags --abbrev=0`.
    pub fn describe_tags(&self) -> Result<String> {
        let mut opts = git2::DescribeOptions::new();
        opts.describe_tags();

        let describe = self
            .repo
            .describe(&opts)
            .context("No tags found reachable from HEAD")?;

        let tag = describe
            .format(None)
            .context("Failed to format tag description")?;

        // Strip the "-N-gHASH" suffix to match `git describe --tags --abbrev=0`.
        // libgit2's abbreviated_size(0) means "default 7", not "no suffix".
        // Guard: only strip if the candidate base is a real tag — prevents
        // corrupting tag names that happen to end with "-N-g<hex>" (e.g. "v1.0-0-gcafe").
        let tag = if let Some(g_pos) = tag.rfind("-g") {
            let after_g = &tag[g_pos + 2..];
            if after_g.chars().all(|c| c.is_ascii_hexdigit()) && !after_g.is_empty() {
                let before_g = &tag[..g_pos];
                if let Some(dash_pos) = before_g.rfind('-') {
                    let distance = &before_g[dash_pos + 1..];
                    if !distance.is_empty() && distance.chars().all(|c| c.is_ascii_digit()) {
                        let candidate = &before_g[..dash_pos];
                        // Only accept the stripped result if it exists as a tag
                        let tag_ref = format!("refs/tags/{candidate}");
                        if self.repo.find_reference(&tag_ref).is_ok() {
                            candidate.to_string()
                        } else {
                            tag
                        }
                    } else {
                        tag
                    }
                } else {
                    tag
                }
            } else {
                tag
            }
        } else {
            tag
        };

        Ok(tag)
    }

    /// List all tags sorted by semver descending, equivalent to
    /// `git tag -l --sort=-v:refname`.
    pub fn list_tags_sorted(&self) -> Result<Vec<String>> {
        let tags = self.repo.tag_names(None).context("Failed to list tags")?;
        let mut tag_list: Vec<String> = tags.iter().flatten().map(|s| s.to_string()).collect();
        tag_list.sort_by(|a, b| compare_semver_desc(a, b));
        Ok(tag_list)
    }

    /// Get the current branch name, equivalent to `git rev-parse --abbrev-ref HEAD`.
    /// Returns "HEAD" in detached HEAD state (matching git CLI behavior).
    pub fn branch_name(&self) -> Result<String> {
        let head = self.repo.head().context("HEAD not found")?;
        if head.is_branch() {
            let name = head
                .shorthand()
                .ok_or_else(|| anyhow::anyhow!("HEAD is not a symbolic ref"))?;
            Ok(name.to_string())
        } else {
            Ok("HEAD".to_string())
        }
    }

    /// Get the short SHA (7 chars) of HEAD, equivalent to `git rev-parse --short=7 HEAD`.
    pub fn short_sha(&self) -> Result<String> {
        let head = self.repo.head().context("HEAD not found")?;
        let commit = head
            .peel_to_commit()
            .context("HEAD does not point to a commit")?;
        let id = commit.id();
        let hex = id.to_string();
        Ok(hex[..7.min(hex.len())].to_string())
    }

    /// Get the repository working directory root, equivalent to
    /// `git rev-parse --show-toplevel`.
    pub fn repo_root(&self) -> Result<PathBuf> {
        self.repo
            .workdir()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("Bare repository has no working directory"))
    }

    /// Fetch tags from origin with authentication and a timeout.
    /// Uses `auth-git2` for credential callbacks (SSH agent, keys, HTTPS tokens).
    /// The fetch runs in a background thread; if it exceeds `timeout`, the
    /// thread is abandoned and an error is returned.
    pub fn fetch_tags(&self, timeout: Duration) -> Result<()> {
        let repo_path = self
            .repo
            .workdir()
            .ok_or_else(|| anyhow::anyhow!("Bare repository"))?
            .to_path_buf();

        let (sender, receiver) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let repo = git2::Repository::open(&repo_path)
                    .context("Failed to reopen repository for fetch")?;

                let auth = auth_git2::GitAuthenticator::default();
                let git_config = repo
                    .config()
                    .context("Failed to open repository git config for credentials")?;

                let mut fetch_options = git2::FetchOptions::new();
                let mut remote_callbacks = git2::RemoteCallbacks::new();
                remote_callbacks.credentials(auth.credentials(&git_config));
                fetch_options.remote_callbacks(remote_callbacks);

                let mut remote = repo
                    .find_remote("origin")
                    .context("No 'origin' remote found")?;

                remote
                    .fetch(
                        &["+refs/tags/*:refs/tags/*"],
                        Some(&mut fetch_options),
                        None,
                    )
                    .context("Failed to fetch tags from origin")?;

                Ok(())
            })();
            let _ = sender.send(result);
        });

        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                tracing::warn!(
                    "fetch_tags: timed out after {:?}, background thread may still be running",
                    timeout
                );
                bail!("Fetch tags timed out after {:?}", timeout)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                bail!("Fetch thread terminated unexpectedly (possible panic)")
            }
        }
    }
}

/// Compare two tag strings in semver-descending order.
/// Tags that parse as semver sort numerically; non-semver tags sort lexicographically after.
/// Stable releases sort above pre-releases at the same version (e.g. v1.0.0 > v1.0.0-rc1).
fn compare_semver_desc(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_semver(a), parse_semver(b)) {
        (Some((a_maj, a_min, a_pat, a_pre)), Some((b_maj, b_min, b_pat, b_pre))) => {
            // Descending by version, then stable before pre-release
            match (b_maj, b_min, b_pat).cmp(&(a_maj, a_min, a_pat)) {
                std::cmp::Ordering::Equal => a_pre.cmp(&b_pre), // false < true, so stable first
                other => other,
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.cmp(a),
    }
}

/// Parse a tag like "v1.2.3", "1.2.3", or "v2.0" into (major, minor, patch, is_prerelease).
/// 2-component tags like "v2.0" are treated as patch 0.
/// Pre-release tags (e.g. "v1.0.0-rc1") sort after stable at the same version.
fn parse_semver(tag: &str) -> Option<(u32, u32, u32, bool)> {
    let s = tag.strip_prefix('v').unwrap_or(tag);
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor_str = parts.next()?;
    let has_minor_pre = minor_str.contains(|c: char| !c.is_ascii_digit());
    let minor: u32 = minor_str
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    match parts.next() {
        Some(patch_str) => {
            let has_prerelease = has_minor_pre || patch_str.contains(|c: char| !c.is_ascii_digit());
            let patch: u32 = patch_str
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()?;
            // Reject 4+ component versions (e.g. "1.2.3.4") — not semver
            if !has_prerelease && parts.next().is_some() {
                return None;
            }
            Some((major, minor, patch, has_prerelease))
        }
        None => Some((major, minor, 0, has_minor_pre)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git_ok(repo_dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo_dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Create a temp git repo with an initial commit.
    fn init_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        run_git_ok(p, &["init"]);
        run_git_ok(p, &["config", "user.email", "test@test.com"]);
        run_git_ok(p, &["config", "user.name", "Test"]);
        std::fs::write(p.join("file.txt"), "hello").unwrap();
        run_git_ok(p, &["add", "file.txt"]);
        run_git_ok(p, &["commit", "-m", "init", "--no-gpg-sign"]);
        dir
    }

    #[test]
    fn test_open_non_repo() {
        let dir = TempDir::new().unwrap();
        let result = Git2Repo::open(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_open_valid_repo() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path());
        assert!(repo.is_ok());
    }

    #[test]
    fn test_branch_name() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let branch = repo.branch_name().unwrap();
        // Default branch is typically "master" or "main"
        assert!(!branch.is_empty());
    }

    #[test]
    fn test_short_sha() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let sha = repo.short_sha().unwrap();
        assert_eq!(sha.len(), 7);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_repo_root() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let root = repo.repo_root().unwrap();
        // Canonicalize both to handle macOS /private/var/... symlinks
        let expected = dir.path().canonicalize().unwrap();
        let actual = root.canonicalize().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_describe_tags_no_tags() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let result = repo.describe_tags();
        assert!(result.is_err());
    }

    #[test]
    fn test_describe_tags_with_tag() {
        let dir = init_test_repo();
        run_git_ok(dir.path(), &["tag", "v1.0.0"]);
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tag = repo.describe_tags().unwrap();
        assert_eq!(tag, "v1.0.0");
    }

    #[test]
    fn test_describe_tags_head_past_tag() {
        let dir = init_test_repo();
        run_git_ok(dir.path(), &["tag", "v1.0.0"]);
        // Make another commit so HEAD is past the tag
        std::fs::write(dir.path().join("extra.txt"), "extra").unwrap();
        run_git_ok(dir.path(), &["add", "extra.txt"]);
        run_git_ok(dir.path(), &["commit", "-m", "extra", "--no-gpg-sign"]);
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tag = repo.describe_tags().unwrap();
        // Should return just the tag, not "v1.0.0-1-gabcdef"
        assert_eq!(tag, "v1.0.0");
    }

    #[test]
    fn test_branch_name_detached_head() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let sha = repo.short_sha().unwrap();
        // Detach HEAD
        run_git_ok(dir.path(), &["checkout", "--detach"]);
        let repo = Git2Repo::open(dir.path()).unwrap();
        let branch = repo.branch_name().unwrap();
        assert_eq!(branch, "HEAD");
        // Verify short_sha still works in detached state
        let sha2 = repo.short_sha().unwrap();
        assert_eq!(sha, sha2);
    }

    #[test]
    fn test_list_tags_sorted() {
        let dir = init_test_repo();
        for tag in &["v0.1.0", "v1.0.0", "v0.2.0", "v0.10.0"] {
            run_git_ok(dir.path(), &["tag", tag]);
        }
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tags = repo.list_tags_sorted().unwrap();
        assert_eq!(tags, vec!["v1.0.0", "v0.10.0", "v0.2.0", "v0.1.0"]);
    }

    #[test]
    fn test_list_tags_empty() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tags = repo.list_tags_sorted().unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3, false)));
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3, false)));
        assert_eq!(parse_semver("v0.10.0"), Some((0, 10, 0, false)));
        assert_eq!(parse_semver("v1.0.0-rc1"), Some((1, 0, 0, true)));
        assert_eq!(parse_semver("v2.0"), Some((2, 0, 0, false)));
        assert_eq!(parse_semver("1.5"), Some((1, 5, 0, false)));
        assert_eq!(parse_semver("v2.0-beta"), Some((2, 0, 0, true)));
        assert_eq!(parse_semver("v1.5-rc1"), Some((1, 5, 0, true)));
        assert_eq!(parse_semver("not-a-version"), None);
        assert_eq!(parse_semver(""), None);
        // 4+ component versions are not semver
        assert_eq!(parse_semver("v1.2.3.4"), None);
        assert_eq!(parse_semver("1.2.3.4.5"), None);
    }

    #[test]
    fn test_compare_semver_desc() {
        assert_eq!(
            compare_semver_desc("v1.0.0", "v0.1.0"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_semver_desc("v0.1.0", "v1.0.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_semver_desc("v1.0.0", "v1.0.0"),
            std::cmp::Ordering::Equal
        );
        // non-semver sorts after semver
        assert_eq!(
            compare_semver_desc("v1.0.0", "latest"),
            std::cmp::Ordering::Less
        );
        // stable sorts before pre-release at same version
        assert_eq!(
            compare_semver_desc("v1.0.0", "v1.0.0-rc1"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_semver_desc("v1.0.0-rc1", "v1.0.0"),
            std::cmp::Ordering::Greater
        );
        // 2-component tags treated as patch 0
        assert_eq!(
            compare_semver_desc("v2.0", "v1.9.9"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_semver_desc("v2.0", "v2.0.0"),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn test_list_tags_stable_before_prerelease() {
        let dir = init_test_repo();
        for tag in &["v1.0.0-rc1", "v1.0.0", "v1.0.0-beta"] {
            run_git_ok(dir.path(), &["tag", tag]);
        }
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tags = repo.list_tags_sorted().unwrap();
        // Stable v1.0.0 should come first, then pre-releases
        assert_eq!(tags[0], "v1.0.0");
    }

    #[test]
    fn test_describe_tags_ambiguous_tag_name() {
        // Tag name that looks like a git-describe suffix: "v1.0-0-gcafe"
        // HEAD on this tag should return the full name, not strip it to "v1.0"
        let dir = init_test_repo();
        run_git_ok(dir.path(), &["tag", "v1.0-0-gcafe"]);
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tag = repo.describe_tags().unwrap();
        assert_eq!(tag, "v1.0-0-gcafe");
    }

    #[test]
    fn test_describe_tags_ambiguous_tag_past_head() {
        // When HEAD is past a tag with an ambiguous name, stripping should
        // still work because the real base tag exists.
        let dir = init_test_repo();
        run_git_ok(dir.path(), &["tag", "v1.0-0-gcafe"]);
        // Make another commit past the tag
        std::fs::write(dir.path().join("extra.txt"), "extra").unwrap();
        run_git_ok(dir.path(), &["add", "extra.txt"]);
        run_git_ok(dir.path(), &["commit", "-m", "extra", "--no-gpg-sign"]);
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tag = repo.describe_tags().unwrap();
        // Should return the full tag name since it's the only tag
        assert_eq!(tag, "v1.0-0-gcafe");
    }

    #[test]
    fn test_fetch_tags_no_remote() {
        let dir = init_test_repo();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let result = repo.fetch_tags(Duration::from_secs(5));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No 'origin' remote"));
    }

    #[test]
    #[ignore] // requires network access — flaky in CI and pre-push hooks
    fn test_fetch_tags_real_repo() {
        // Use the actual skilldo repo — it has an origin remote and tags.
        let repo = Git2Repo::open_cwd().unwrap();
        let result = repo.fetch_tags(Duration::from_secs(30));
        assert!(result.is_ok(), "fetch_tags failed: {:?}", result);
        // After fetch, tags should be present
        let tags = repo.list_tags_sorted().unwrap();
        assert!(!tags.is_empty(), "Expected tags after fetch");
    }
}
