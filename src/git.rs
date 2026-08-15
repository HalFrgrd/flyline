use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Fingerprint of a git repository's reference database to detect changes quickly.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitDirFingerprint {
    head_mtime: Option<SystemTime>,
    packed_refs_mtime: Option<SystemTime>,
    refs_heads_mtime: Option<SystemTime>,
    refs_remotes_mtime: Option<SystemTime>,
    refs_tags_mtime: Option<SystemTime>,
    stash_log_mtime: Option<SystemTime>,
}

impl GitDirFingerprint {
    fn from_git_dir(git_dir: &Path) -> Self {
        Self {
            head_mtime: git_dir
                .join("HEAD")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            packed_refs_mtime: git_dir
                .join("packed-refs")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            refs_heads_mtime: git_dir
                .join("refs/heads")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            refs_remotes_mtime: git_dir
                .join("refs/remotes")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            refs_tags_mtime: git_dir
                .join("refs/tags")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
            stash_log_mtime: git_dir
                .join("logs/refs/stash")
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok()),
        }
    }
}

/// Cached reference timestamps for a specific git repository.
#[derive(Debug, Clone)]
struct CachedRepo {
    git_dir: PathBuf,
    fingerprint: GitDirFingerprint,
    last_checked: Instant,
    refs: HashMap<String, u64>,
}

const CACHE_TTL: Duration = Duration::from_secs(2);

static GIT_CACHE: LazyLock<Mutex<HashMap<PathBuf, CachedRepo>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Locate the Git repository root and the `.git` directory starting from `start_dir`.
/// Handles standard `.git` directories as well as `.git` pointer files (for worktrees and submodules).
pub fn find_git_repo_root(start_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    for ancestor in start_dir.ancestors() {
        let dot_git = ancestor.join(".git");
        if dot_git.is_dir() {
            return Some((ancestor.to_path_buf(), dot_git));
        } else if dot_git.is_file() {
            if let Ok(content) = std::fs::read_to_string(&dot_git) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(gitdir_rel) = trimmed.strip_prefix("gitdir:") {
                        let gitdir_path = gitdir_rel.trim();
                        let resolved = if Path::new(gitdir_path).is_absolute() {
                            PathBuf::from(gitdir_path)
                        } else {
                            ancestor.join(gitdir_path)
                        };
                        if resolved.exists() {
                            return Some((ancestor.to_path_buf(), resolved));
                        }
                    }
                }
            }
        }
    }
    None
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
fn load_git_refs(repo_root: &Path, git_dir: &Path) -> HashMap<String, u64> {
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
    let stash_log = git_dir.join("logs/refs/stash");
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

/// Retrieve the last modification timestamp for a Git reference in the repository
/// containing `dir`.
pub fn get_ref_mtime_in_dir(dir: &Path, ref_name: &str) -> Option<u64> {
    let (repo_root, git_dir) = find_git_repo_root(dir)?;

    let mut cache = GIT_CACHE.lock().unwrap_or_else(|e| e.into_inner());

    let now = Instant::now();
    let current_fingerprint = GitDirFingerprint::from_git_dir(&git_dir);

    let needs_refresh = match cache.get(&repo_root) {
        Some(entry) => {
            if entry.git_dir != git_dir {
                true
            } else if now.duration_since(entry.last_checked) < CACHE_TTL {
                false
            } else {
                entry.fingerprint != current_fingerprint
            }
        }
        None => true,
    };

    if needs_refresh {
        let refs = load_git_refs(&repo_root, &git_dir);
        cache.insert(
            repo_root.clone(),
            CachedRepo {
                git_dir,
                fingerprint: current_fingerprint,
                last_checked: now,
                refs,
            },
        );
    }

    let entry = cache.get(&repo_root)?;
    let trimmed = ref_name.trim().trim_end_matches('/');
    entry
        .refs
        .get(ref_name)
        .or_else(|| entry.refs.get(trimmed))
        .or_else(|| entry.refs.get(ref_name.trim()))
        .copied()
}

/// Retrieve the last modification timestamp for a Git reference in the current working directory's repository.
pub fn get_ref_mtime(ref_name: &str) -> Option<u64> {
    let current_dir = std::env::current_dir().ok()?;
    get_ref_mtime_in_dir(&current_dir, ref_name)
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
        let (root, git_dir) = found.unwrap();
        assert!(git_dir.exists());
        assert!(root.exists());
    }

    #[test]
    fn test_get_ref_mtime_in_this_repo() {
        let current_dir = std::env::current_dir().unwrap();
        let mtime = get_ref_mtime_in_dir(&current_dir, "master");
        assert!(mtime.is_some() || get_ref_mtime_in_dir(&current_dir, "origin/master").is_some());

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
    }
}
