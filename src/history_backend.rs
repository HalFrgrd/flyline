use std::collections::HashSet;
use std::fs::{DirBuilder, File, OpenOptions, Permissions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};

use crate::history::{HistoryEntry, HistoryManager, HistoryTag, TimestampNanos};

fn is_normal_tag(tag: &HistoryTag) -> bool {
    *tag == HistoryTag::Normal
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HistoryJsonlEvent {
    Start {
        id: String,
        timestamp: TimestampNanos,
        command: String,
        #[serde(default, skip_serializing_if = "is_normal_tag")]
        tag: HistoryTag,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hostname: Option<String>,
        session: String,
    },
    End {
        id: String,
        timestamp: TimestampNanos,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ns: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_status: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pipestatus: Option<String>,
    },
}

pub fn default_jsonl_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("flyline")
        .join("history.jsonl")
}

pub fn is_file_empty_or_missing(path: &Path) -> bool {
    !path.exists()
        || std::fs::metadata(path)
            .map(|m| m.len() == 0)
            .unwrap_or(true)
}

pub fn create_jsonl_path(path: &Path) {
    if let Some(parent) = path.parent() {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            builder.mode(0o700);
        }
        let _ = builder.create(parent);
        #[cfg(unix)]
        {
            let _ = std::fs::set_permissions(parent, Permissions::from_mode(0o700));
        }
    }
}

#[cfg(unix)]
struct FlockGuard(RawFd);

#[cfg(not(unix))]
struct FlockGuard(i32);

impl Drop for FlockGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::flock(self.0, libc::LOCK_UN);
        }
    }
}

impl FlockGuard {
    #[cfg(unix)]
    fn lock_exclusive(fd: RawFd) -> Self {
        unsafe {
            libc::flock(fd, libc::LOCK_EX);
        }
        FlockGuard(fd)
    }

    #[cfg(not(unix))]
    fn lock_exclusive(fd: i32) -> Self {
        FlockGuard(fd)
    }

    #[cfg(unix)]
    fn lock_shared(fd: RawFd) -> Self {
        unsafe {
            libc::flock(fd, libc::LOCK_SH);
        }
        FlockGuard(fd)
    }

    #[cfg(not(unix))]
    fn lock_shared(fd: i32) -> Self {
        FlockGuard(fd)
    }
}

pub fn append_jsonl_history_events(
    events: &[HistoryJsonlEvent],
    path: &Path,
) -> anyhow::Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    create_jsonl_path(path);

    let mut open_options = OpenOptions::new();
    open_options.create(true).append(true);
    #[cfg(unix)]
    {
        open_options.mode(0o600);
    }
    let file = open_options.open(path)?;

    let _lock_guard = FlockGuard::lock_exclusive(file.as_raw_fd());

    let mut writer = std::io::BufWriter::new(&file);
    for event in events {
        serde_json::to_writer(&mut writer, event)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;

    Ok(())
}

pub fn append_jsonl_history_event(event: &HistoryJsonlEvent, path: &Path) -> anyhow::Result<()> {
    append_jsonl_history_events(std::slice::from_ref(event), path)
}

#[derive(Debug, Clone, Default)]
pub struct JsonlFetchResult {
    pub new_entries: Vec<HistoryEntry>,
    pub end_updates: Vec<(String, Option<u64>, Option<i32>, Option<String>)>,
    pub new_offset: u64,
    pub last_seen_event_id: Option<String>,
}

pub fn fetch_flyline_jsonl_history_from_offset(
    path: &Path,
    start_offset: u64,
    last_seen_event_id: Option<&str>,
) -> anyhow::Result<JsonlFetchResult> {
    if is_file_empty_or_missing(path) {
        return Ok(JsonlFetchResult::default());
    }

    let mut file = File::open(path)?;
    let _lock_guard = FlockGuard::lock_shared(file.as_raw_fd());

    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut actual_offset = start_offset;
    let mut needs_recovery = start_offset > file_len;

    if !needs_recovery && start_offset > 0 {
        if file.seek(SeekFrom::Start(start_offset)).is_ok() {
            let mut check_buf = String::new();
            let mut check_reader = BufReader::new(&file);
            if let Ok(bytes) = check_reader.read_line(&mut check_buf) {
                if bytes > 0 {
                    let trimmed = check_buf.trim();
                    if !trimmed.is_empty()
                        && serde_json::from_str::<HistoryJsonlEvent>(trimmed).is_err()
                    {
                        needs_recovery = true;
                    }
                }
            }
        } else {
            needs_recovery = true;
        }
    }

    if needs_recovery {
        log::warn!(
            "Flyline JSONL offset {} is invalid or file tampered; attempting recovery via last_seen_event_id {:?}",
            start_offset,
            last_seen_event_id
        );
        actual_offset = 0;
        if let Some(target_id) = last_seen_event_id {
            if file.seek(SeekFrom::Start(0)).is_ok() {
                let mut rec_reader = BufReader::new(&file);
                let mut rec_buf = String::new();
                let mut pos = 0u64;
                let mut found_pos = None;

                while let Ok(bytes) = rec_reader.read_line(&mut rec_buf) {
                    if bytes == 0 {
                        break;
                    }
                    pos += bytes as u64;
                    let trimmed = rec_buf.trim();
                    if !trimmed.is_empty() {
                        if let Ok(event) = serde_json::from_str::<HistoryJsonlEvent>(trimmed) {
                            let event_id = match &event {
                                HistoryJsonlEvent::Start { id, .. } => id,
                                HistoryJsonlEvent::End { id, .. } => id,
                            };
                            if event_id == target_id {
                                found_pos = Some(pos);
                            }
                        }
                    }
                    rec_buf.clear();
                }
                if let Some(recovered_pos) = found_pos {
                    log::info!(
                        "Flyline JSONL recovered valid offset at byte {}",
                        recovered_pos
                    );
                    actual_offset = recovered_pos;
                }
            }
        }
    }

    let _ = file.seek(SeekFrom::Start(actual_offset));
    let mut reader = BufReader::new(&file);
    let mut entries = Vec::new();
    let mut end_updates = Vec::new();
    let mut entry_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut current_offset = actual_offset;
    let mut line_buf = String::new();
    let mut line_idx = 0;
    let mut last_seen_id = last_seen_event_id.map(String::from);

    while let Ok(bytes_read) = reader.read_line(&mut line_buf) {
        if bytes_read == 0 {
            break;
        }
        current_offset += bytes_read as u64;

        let line = line_buf.trim();
        if !line.is_empty() {
            if let Ok(event) = serde_json::from_str::<HistoryJsonlEvent>(line) {
                match event {
                    HistoryJsonlEvent::Start { ref id, .. } => {
                        last_seen_id = Some(id.clone());
                        let event_id = id.clone();
                        if let Ok(mut entry) = HistoryEntry::try_from(event) {
                            entry.index = line_idx;
                            entry_map.insert(event_id, entries.len());
                            entries.push(entry);
                            line_idx += 1;
                        }
                    }
                    HistoryJsonlEvent::End {
                        id,
                        duration_ns,
                        exit_status,
                        pipestatus,
                        ..
                    } => {
                        last_seen_id = Some(id.clone());
                        if let Some(&idx) = entry_map.get(&id) {
                            if let Some(entry) = entries.get_mut(idx) {
                                entry.apply_end_metadata(
                                    duration_ns,
                                    exit_status,
                                    pipestatus.as_deref(),
                                );
                            }
                        }
                        end_updates.push((id, duration_ns, exit_status, pipestatus));
                    }
                }
            }
        }
        line_buf.clear();
    }

    Ok(JsonlFetchResult {
        new_entries: entries,
        end_updates,
        new_offset: current_offset,
        last_seen_event_id: last_seen_id,
    })
}

pub fn repopulate_jsonl_from_entries(
    entries: &[HistoryEntry],
    session_id: &str,
    target_path: &Path,
) -> anyhow::Result<u64> {
    let default_hostname = Some(crate::bash_funcs::get_hostname()).filter(|h| !h.is_empty());

    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    let mut events = Vec::with_capacity(sorted_entries.len() * 2);
    for entry in &sorted_entries {
        if entry.command.trim().is_empty() {
            continue;
        }
        events.push(entry.to_jsonl_start_event(session_id, default_hostname.as_deref()));
        if let Some(end_event) = entry.to_jsonl_end_event() {
            events.push(end_event);
        }
    }
    append_jsonl_history_events(&events, target_path)?;

    let file_len = std::fs::metadata(target_path).map(|m| m.len()).unwrap_or(0);
    Ok(file_len)
}
