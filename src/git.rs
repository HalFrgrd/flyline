use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Fingerprint of a `.git` directory's reference metadata files.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitDirFingerprint {
    pub head_mtime: Option<SystemTime>,
    pub packed_refs_mtime: Option<SystemTime>,
    pub refs_heads_mtime: Option<SystemTime>,
    pub refs_remotes_mtime: Option<SystemTime>,
    pub refs_tags_mtime: Option<SystemTime>,
    pub stash_log_mtime: Option<SystemTime>,
}

impl GitDirFingerprint {
    /// Compute the fingerprint by inspecting last modification times of git ref files.
    pub fn from_git_dir(git_dir: &Path, common_dir: Option<&Path>) -> Self {
        let refs_dir = common_dir.unwrap_or(git_dir);
        Self {
            head_mtime: git_dir
                .join("HEAD")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            packed_refs_mtime: refs_dir
                .join("packed-refs")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            refs_heads_mtime: refs_dir
                .join("refs/heads")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            refs_remotes_mtime: refs_dir
                .join("refs/remotes")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            refs_tags_mtime: refs_dir
                .join("refs/tags")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            stash_log_mtime: refs_dir
                .join("logs/refs/stash")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
        }
    }
}

/// Snapshot of previous git cache state passed from main thread to the background worker.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitRepoSnapshot {
    pub repo_root: PathBuf,
    pub fingerprint: GitDirFingerprint,
}

/// Background scan payload containing git reference timestamps for a directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GitRepoPayload {
    Unchanged {
        duration: Duration,
    },
    Updated {
        repo_root: PathBuf,
        fingerprint: GitDirFingerprint,
        refs: HashMap<String, u64>,
        duration: Duration,
    },
}

impl GitRepoPayload {
    pub fn duration(&self) -> Duration {
        match self {
            GitRepoPayload::Unchanged { duration } => *duration,
            GitRepoPayload::Updated { duration, .. } => *duration,
        }
    }
}

/// Cached git state for the current editing session.
#[derive(Debug, Clone)]
struct CachedRepo {
    repo_root: PathBuf,
    fingerprint: GitDirFingerprint,
    refs: HashMap<String, u64>,
}

#[derive(Default)]
struct GitCacheState {
    /// Cached repository for the active prompt session.
    current: Option<CachedRepo>,
}

static GIT_CACHE: LazyLock<Mutex<GitCacheState>> =
    LazyLock::new(|| Mutex::new(GitCacheState::default()));

/// Retrieve a snapshot of the cached repo (root and fingerprint) if available.
pub fn get_cached_snapshot() -> Option<GitRepoSnapshot> {
    let cache = GIT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let repo = cache.current.as_ref()?;
    Some(GitRepoSnapshot {
        repo_root: repo.repo_root.clone(),
        fingerprint: repo.fingerprint.clone(),
    })
}

/// Retrieve the number of cached git refs if available.
pub fn get_cached_ref_count() -> usize {
    let cache = GIT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.current.as_ref().map(|r| r.refs.len()).unwrap_or(0)
}

/// Scan git repository refs for `cwd` (invoked in the background startup worker subshell).
///
/// Because `fork()` duplicates the parent process address space (including `GIT_CACHE`),
/// the worker reads `get_cached_snapshot()` directly to check if the repository fingerprint
/// is unchanged. If unchanged, skips loading refs and returns `GitRepoPayload::Unchanged`.
pub fn scan_git_repo_payload(cwd: &Path) -> Option<GitRepoPayload> {
    let start = Instant::now();
    let (repo_root, git_dir, common_dir) = find_git_repo_root(cwd)?;
    let fingerprint = GitDirFingerprint::from_git_dir(&git_dir, common_dir.as_deref());

    if let Some(prev) = get_cached_snapshot() {
        if prev.repo_root == repo_root && prev.fingerprint == fingerprint {
            return Some(GitRepoPayload::Unchanged {
                duration: start.elapsed(),
            });
        }
    }

    let refs = load_git_refs(&repo_root, &git_dir, common_dir.as_deref());
    let duration = start.elapsed();
    Some(GitRepoPayload::Updated {
        repo_root,
        fingerprint,
        refs,
        duration,
    })
}

/// Apply background-scanned git repository refs to the main thread's cache.
pub fn apply_git_repo_payload(payload: GitRepoPayload) {
    let mut cache = GIT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    match payload {
        GitRepoPayload::Unchanged { .. } => {
            // Already in cache, nothing to update!
        }
        GitRepoPayload::Updated {
            repo_root,
            fingerprint,
            refs,
            ..
        } => {
            cache.current = Some(CachedRepo {
                repo_root,
                fingerprint,
                refs,
            });
        }
    }
}

/// Locate the Git repository root, `.git` directory, and shared common directory using `git rev-parse`.
pub fn find_git_repo_root(start_dir: &Path) -> Option<(PathBuf, PathBuf, Option<PathBuf>)> {
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ])
        .current_dir(start_dir)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = std::str::from_utf8(&output.stdout).ok()?;
    let mut lines = text.lines();
    let toplevel = lines.next()?.trim();
    let git_dir_raw = lines.next()?.trim();
    let common_dir_raw = lines.next()?.trim();

    let repo_root = PathBuf::from(toplevel);
    let git_dir = if Path::new(git_dir_raw).is_absolute() {
        PathBuf::from(git_dir_raw)
    } else {
        start_dir.join(git_dir_raw)
    };
    let common_dir = if common_dir_raw == git_dir_raw {
        None
    } else if Path::new(common_dir_raw).is_absolute() {
        Some(PathBuf::from(common_dir_raw))
    } else {
        Some(start_dir.join(common_dir_raw))
    };

    Some((repo_root, git_dir, common_dir))
}

/// Parse output of `git for-each-ref` into the refs map.
fn parse_for_each_ref_output(output: &str, map: &mut HashMap<String, u64>) {
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let refname = parts[0].trim();
            let short_name = parts[1].trim();
            let date_str = parts[2].trim();

            if let Some(ts) = date_str
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
            {
                if !short_name.is_empty() {
                    map.insert(short_name.to_string(), ts);
                }
                if !refname.is_empty() {
                    map.insert(refname.to_string(), ts);
                    if let Some(without_refs) = refname.strip_prefix("refs/") {
                        map.insert(without_refs.to_string(), ts);
                    }
                    if let Some(without_remotes) = refname.strip_prefix("refs/remotes/") {
                        map.insert(without_remotes.to_string(), ts);
                    }
                    if let Some(without_heads) = refname.strip_prefix("refs/heads/") {
                        map.insert(without_heads.to_string(), ts);
                    }
                    if let Some(without_tags) = refname.strip_prefix("refs/tags/") {
                        map.insert(without_tags.to_string(), ts);
                    }
                }
            }
        }
    }
}

/// Parse output of `git log -g --format=%gd\t%ct refs/stash` into the refs map.
fn parse_stash_log_output(output: &str, map: &mut HashMap<String, u64>) {
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let stash_name = parts[0].trim();
            let ts_str = parts[1].trim();
            if let Ok(ts) = ts_str.parse::<u64>() {
                if !stash_name.is_empty() {
                    map.insert(stash_name.to_string(), ts);
                }
            }
        }
    }
}

/// Query git commands to load all branch, tag, and stash timestamps for `repo_root`.
fn load_git_refs(
    repo_root: &Path,
    git_dir: &Path,
    common_dir: Option<&Path>,
) -> HashMap<String, u64> {
    let mut map = HashMap::new();

    // Query branches, remotes, and tags in a single batch
    let for_each_ref_res = Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname)\t%(refname:short)\t%(creatordate:raw)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])
        .current_dir(repo_root)
        .output();

    if let Ok(output) = for_each_ref_res {
        if output.status.success() {
            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                parse_for_each_ref_output(text, &mut map);
            }
        }
    }

    // Query HEAD commit timestamp
    let head_res = Command::new("git")
        .args(["log", "-1", "--format=%ct", "HEAD"])
        .current_dir(repo_root)
        .output();
    if let Ok(output) = head_res {
        if output.status.success() {
            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                if let Ok(ts) = text.trim().parse::<u64>() {
                    map.insert("HEAD".to_string(), ts);
                }
            }
        }
    }

    // Query stashes if stash log exists
    let refs_dir = common_dir.unwrap_or(git_dir);
    let stash_log = refs_dir.join("logs/refs/stash");
    if stash_log.exists() {
        let stash_res = Command::new("git")
            .args(["log", "-g", "--format=%gd\t%ct", "refs/stash"])
            .current_dir(repo_root)
            .output();

        if let Ok(output) = stash_res {
            if output.status.success() {
                if let Ok(text) = std::str::from_utf8(&output.stdout) {
                    parse_stash_log_output(text, &mut map);
                }
            }
        }
    }

    map
}

/// Retrieve the last modification timestamp for a Git reference from the cached git state.
pub fn get_ref_mtime_in_dir(dir: &Path, ref_name: &str) -> Option<u64> {
    let cache = GIT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let repo = cache.current.as_ref()?;
    if !dir.starts_with(&repo.repo_root) {
        return None;
    }
    let trimmed = ref_name.trim().trim_end_matches('/');
    repo.refs
        .get(ref_name)
        .or_else(|| repo.refs.get(trimmed))
        .or_else(|| repo.refs.get(ref_name.trim()))
        .copied()
}

/// Retrieve the last modification timestamp for a Git reference in the current working directory's repository.
pub fn get_ref_mtime(ref_name: &str) -> Option<u64> {
    let cwd = crate::shell::backend().cwd();
    if cwd.is_empty() {
        return None;
    }
    let current_dir = PathBuf::from(cwd);
    get_ref_mtime_in_dir(&current_dir, ref_name)
}

/// Clear the cached git repository state for the current editing session.
pub fn reset_cache() {
    let mut cache = GIT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.current = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_for_each_ref_output() {
        let sample = "refs/heads/master\tmaster\t1786801281 +0100\n\
                      refs/remotes/origin/feat/foo\torigin/feat/foo\t1786796315 +0100\n\
                      refs/tags/v1.0.0\tv1.0.0\t1779212784 +0000\n";
        let mut map = HashMap::new();
        parse_for_each_ref_output(sample, &mut map);

        assert_eq!(map.get("master"), Some(&1786801281));
        assert_eq!(map.get("refs/heads/master"), Some(&1786801281));
        assert_eq!(map.get("heads/master"), Some(&1786801281));

        assert_eq!(map.get("origin/feat/foo"), Some(&1786796315));
        assert_eq!(map.get("remotes/origin/feat/foo"), Some(&1786796315));
        assert_eq!(map.get("refs/remotes/origin/feat/foo"), Some(&1786796315));

        assert_eq!(map.get("v1.0.0"), Some(&1779212784));
        assert_eq!(map.get("tags/v1.0.0"), Some(&1779212784));
        assert_eq!(map.get("refs/tags/v1.0.0"), Some(&1779212784));
    }

    #[test]
    fn test_parse_stash_log_output() {
        let sample = "stash@{0}\t1785795310\nstash@{1}\t1784996381\n";
        let mut map = HashMap::new();
        parse_stash_log_output(sample, &mut map);

        assert_eq!(map.get("stash@{0}"), Some(&1785795310));
        assert_eq!(map.get("stash@{1}"), Some(&1784996381));
    }

    #[test]
    fn test_find_git_repo_root() {
        let current_dir = std::env::current_dir().unwrap();
        let found = find_git_repo_root(&current_dir);
        assert!(found.is_some());
        let (root, git_dir, _common_dir) = found.unwrap();
        assert!(git_dir.exists());
        assert!(root.exists());
    }

    #[test]
    fn test_scan_and_apply_git_repo_payload() {
        reset_cache();
        let current_dir = std::env::current_dir().unwrap();
        let payload = scan_git_repo_payload(&current_dir);
        assert!(payload.is_some());
        let payload = payload.unwrap();

        assert!(matches!(payload, GitRepoPayload::Updated { .. }));

        apply_git_repo_payload(payload);

        let mtime = get_ref_mtime_in_dir(&current_dir, "master");
        assert!(mtime.is_some() || get_ref_mtime_in_dir(&current_dir, "origin/master").is_some());

        // Second scan with cached state present should return Unchanged
        let unchanged_payload = scan_git_repo_payload(&current_dir);
        assert!(matches!(
            unchanged_payload,
            Some(GitRepoPayload::Unchanged { .. })
        ));

        // Test with trailing space (as returned by some completion functions)
        let mtime_space = get_ref_mtime_in_dir(&current_dir, "master ");
        assert_eq!(mtime, mtime_space);

        // Test HEAD lookup
        let head_mtime = get_ref_mtime_in_dir(&current_dir, "HEAD");
        assert!(head_mtime.is_some());
        let head_space_mtime = get_ref_mtime_in_dir(&current_dir, "HEAD ");
        assert_eq!(head_mtime, head_space_mtime);

        // Test non-existent branch
        assert_eq!(
            get_ref_mtime_in_dir(&current_dir, "non_existent_branch_12345"),
            None
        );

        // Test reset_cache clears state
        reset_cache();
        {
            let cache = GIT_CACHE.lock().unwrap();
            assert!(cache.current.is_none());
        }
    }
}
