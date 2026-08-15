use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use crate::bash_funcs::CommandWordInfo;

/// Per-directory executable cache entry: the directory's last-modified time and
/// the list of executable filenames found in that directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirExecutables {
    pub mtime: Option<SystemTime>,
    pub names: Vec<String>,
}

/// Changes to PATH directories computed in a background subshell.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathScanPayload {
    pub updated: HashMap<PathBuf, DirExecutables>,
    pub current_dirs: HashSet<PathBuf>,
}

/// Global cache that maps each directory on `PATH` to its executable names and
/// the directory's last-modified timestamp.  The cache is **never** invalidated
/// on app startup; instead it is updated lazily on every access:
///
/// 1. Directories that have been removed from `PATH` are evicted from the cache.
/// 2. Newly-added directories are scanned and inserted.
/// 3. For each remaining directory the last-modified time is compared to the
///    cached value; if it has changed the directory is re-scanned.
pub struct ExecutablesOnPath {
    cache: HashMap<PathBuf, DirExecutables>,
}

impl Default for ExecutablesOnPath {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutablesOnPath {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Compute changes to PATH directories in a background subshell.
    /// Returns a payload with updated directory scans and current PATH set.
    pub fn scan_path_updates(path_env: Option<String>) -> PathScanPayload {
        let current_dirs: Vec<PathBuf> = path_env
            .map(|p| p.split(':').map(PathBuf::from).collect())
            .unwrap_or_default();

        let current_dir_set: HashSet<PathBuf> = current_dirs.iter().cloned().collect();

        let guard = EXECUTABLES_ON_PATH
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let mut updated = HashMap::new();
        for dir in &current_dirs {
            let current_mtime = dir.metadata().ok().and_then(|m| m.modified().ok());

            let needs_update = match guard.cache.get(dir) {
                Some(entry) if entry.mtime == current_mtime => false,
                _ => true,
            };

            if needs_update {
                let names = if current_mtime.is_some() {
                    Self::scan_dir(dir)
                } else {
                    Vec::new()
                };

                updated.insert(
                    dir.clone(),
                    DirExecutables {
                        mtime: current_mtime,
                        names,
                    },
                );
            }
        }

        PathScanPayload {
            updated,
            current_dirs: current_dir_set,
        }
    }

    /// Apply the subshell's scan results back to the global cache:
    /// evict removed PATH dirs, and insert newly scanned/updated dirs.
    pub fn apply_updates(payload: PathScanPayload) {
        let mut guard = EXECUTABLES_ON_PATH
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // Evict directories that are no longer on PATH.
        guard
            .cache
            .retain(|dir, _| payload.current_dirs.contains(dir));

        // Update only the directories that were modified or newly added.
        for (dir, entry) in payload.updated {
            guard.cache.insert(dir, entry);
        }
    }

    /// Iterate over the names of all cached executables.
    pub fn iter_info(&self) -> impl Iterator<Item = CommandWordInfo> + '_ {
        self.cache.iter().flat_map(|(dir, entry)| {
            entry.names.iter().map(move |name| {
                let path = dir.join(name).to_string_lossy().into_owned();
                CommandWordInfo::File {
                    command: name.clone(),
                    path,
                }
            })
        })
    }

    /// Scan `dir` and return the names of all executable files it contains.
    fn scan_dir(dir: &Path) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = std::fs::metadata(entry.path())
                    && metadata.is_file()
                {
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 != 0 {
                        if let Some(file_name) = entry.file_name().to_str() {
                            names.push(file_name.to_string());
                        }
                    }
                }
            }
        }
        names
    }
}

pub static EXECUTABLES_ON_PATH: LazyLock<Mutex<ExecutablesOnPath>> =
    LazyLock::new(|| Mutex::new(ExecutablesOnPath::new()));
