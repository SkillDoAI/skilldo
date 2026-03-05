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
        let tag = if let Some(pos) = tag.rfind("-g") {
            // Verify the part after "-g" looks like a hex hash
            let after = &tag[pos + 2..];
            if after.chars().all(|c| c.is_ascii_hexdigit()) && !after.is_empty() {
                // Also strip the "-N" distance count before "-g"
                let before_g = &tag[..pos];
                if let Some(dash) = before_g.rfind('-') {
                    let distance = &before_g[dash + 1..pos];
                    if distance.chars().all(|c| c.is_ascii_digit()) {
                        before_g[..dash].to_string()
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
fn compare_semver_desc(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_semver(a), parse_semver(b)) {
        (Some(va), Some(vb)) => vb.cmp(&va), // descending
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.cmp(a),
    }
}

/// Parse a tag like "v1.2.3" or "1.2.3" into (major, minor, patch).
fn parse_semver(tag: &str) -> Option<(u32, u32, u32)> {
    let s = tag.strip_prefix('v').unwrap_or(tag);
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    // Patch may have pre-release suffix like "3-rc1"
    let patch_str = parts.next()?;
    let patch: u32 = patch_str
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a temp git repo with an initial commit.
    fn init_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        Command::new("git")
            .args(["init"])
            .current_dir(p)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(p)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(p)
            .output()
            .unwrap();
        std::fs::write(p.join("file.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(p)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init", "--no-gpg-sign"])
            .current_dir(p)
            .output()
            .unwrap();
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
        Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let repo = Git2Repo::open(dir.path()).unwrap();
        let tag = repo.describe_tags().unwrap();
        assert_eq!(tag, "v1.0.0");
    }

    #[test]
    fn test_describe_tags_head_past_tag() {
        let dir = init_test_repo();
        Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        // Make another commit so HEAD is past the tag
        std::fs::write(dir.path().join("extra.txt"), "extra").unwrap();
        Command::new("git")
            .args(["add", "extra.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "extra", "--no-gpg-sign"])
            .current_dir(dir.path())
            .output()
            .unwrap();
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
        Command::new("git")
            .args(["checkout", "--detach"])
            .current_dir(dir.path())
            .output()
            .unwrap();
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
            Command::new("git")
                .args(["tag", tag])
                .current_dir(dir.path())
                .output()
                .unwrap();
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
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("v0.10.0"), Some((0, 10, 0)));
        assert_eq!(parse_semver("v1.0.0-rc1"), Some((1, 0, 0)));
        assert_eq!(parse_semver("not-a-version"), None);
        assert_eq!(parse_semver(""), None);
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
}
