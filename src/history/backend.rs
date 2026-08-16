use std::fs::{DirBuilder, File, OpenOptions, Permissions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};

use super::{HistoryEntry, TimestampNanos};
use crate::shell;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryJsonlEvent {
    Start {
        id: String,
        #[serde(rename = "ts")]
        timestamp: TimestampNanos,
        #[serde(rename = "cmd")]
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(rename = "host", skip_serializing_if = "Option::is_none")]
        hostname: Option<String>,
        #[serde(rename = "sesh")]
        session: String,
    },
    End {
        id: String,
        #[serde(rename = "ts")]
        timestamp: TimestampNanos,
        #[serde(rename = "es", skip_serializing_if = "Option::is_none")]
        exit_status: Option<i32>,
        #[serde(rename = "ps", skip_serializing_if = "Option::is_none")]
        pipestatus: Option<String>,
    },
}

pub fn default_jsonl_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("flyline")
        .join("history.jsonl")
}

pub(super) fn is_file_empty_or_missing(path: &Path) -> bool {
    !path.exists()
        || std::fs::metadata(path)
            .map(|m| m.len() == 0)
            .unwrap_or(true)
}

fn create_jsonl_path(path: &Path) {
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

pub(super) fn append_jsonl_history_events(
    events: &[HistoryJsonlEvent],
    path: &Path,
) -> anyhow::Result<u64> {
    if events.is_empty() {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        return Ok(len);
    }
    create_jsonl_path(path);

    let mut open_options = OpenOptions::new();
    open_options.create(true).append(true);
    #[cfg(unix)]
    {
        open_options.mode(0o600);
    }
    let mut file = open_options.open(path)?;

    let _lock_guard = FlockGuard::lock_exclusive(file.as_raw_fd());

    let start_offset = file.seek(SeekFrom::End(0))?;
    let mut last_event_start = start_offset;
    let mut current_offset = start_offset;

    let mut writer = std::io::BufWriter::new(&file);
    for event in events {
        last_event_start = current_offset;
        let serialized = serde_json::to_string(event)?;
        writer.write_all(serialized.as_bytes())?;
        writer.write_all(b"\n")?;
        current_offset += serialized.len() as u64 + 1;
    }
    writer.flush()?;

    Ok(last_event_start)
}

pub(super) fn append_jsonl_history_event(
    event: &HistoryJsonlEvent,
    path: &Path,
) -> anyhow::Result<u64> {
    append_jsonl_history_events(std::slice::from_ref(event), path)
}

impl HistoryJsonlEvent {
    pub fn id(&self) -> &str {
        match self {
            HistoryJsonlEvent::Start { id, .. } => id,
            HistoryJsonlEvent::End { id, .. } => id,
        }
    }

    #[allow(dead_code)]
    pub fn timestamp(&self) -> TimestampNanos {
        match self {
            HistoryJsonlEvent::Start { timestamp, .. } => *timestamp,
            HistoryJsonlEvent::End { timestamp, .. } => *timestamp,
        }
    }
}

fn read_event_from_reader<R: BufRead>(reader: &mut R) -> Option<(HistoryJsonlEvent, u64)> {
    let mut buf = String::new();
    while let Ok(bytes_read) = reader.read_line(&mut buf) {
        if bytes_read == 0 {
            return None;
        }
        let trimmed = buf.trim();
        if !trimmed.is_empty() {
            if let Ok(event) = serde_json::from_str::<HistoryJsonlEvent>(trimmed) {
                return Some((event, bytes_read as u64));
            }
        }
        buf.clear();
    }
    None
}

fn read_event_at_offset(file: &mut File, offset: u64) -> Option<(HistoryJsonlEvent, u64)> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut reader = BufReader::new(file);
    read_event_from_reader(&mut reader)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastJsonlReadOffset {
    pub byte_offset: u64,
    pub event_id: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct JsonlFetchResult {
    pub(super) events: Vec<HistoryJsonlEvent>,
    pub(super) last_read_offset: Option<LastJsonlReadOffset>,
}

pub(super) fn fetch_flyline_jsonl_history_from_offset(
    path: &Path,
    last_offset: Option<&LastJsonlReadOffset>,
) -> anyhow::Result<JsonlFetchResult> {
    if is_file_empty_or_missing(path) {
        log::warn!(
            "Flyline JSONL history file {:?} is empty or missing; returning empty result",
            path
        );
        return Ok(JsonlFetchResult::default());
    }

    let mut file = File::open(path)?;
    let _lock_guard = FlockGuard::lock_shared(file.as_raw_fd());

    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let (start_offset, last_seen_event_id) = match last_offset {
        Some(offset_info) => (offset_info.byte_offset, Some(offset_info.event_id.as_str())),
        None => (0, None),
    };

    let mut actual_offset = start_offset;
    let mut needs_recovery = start_offset > file_len;

    if !needs_recovery && start_offset > 0 {
        match (
            read_event_at_offset(&mut file, start_offset),
            last_seen_event_id,
        ) {
            (Some((event, bytes_read)), Some(expected_id)) => {
                if event.id() == expected_id {
                    actual_offset = start_offset + bytes_read;
                } else {
                    needs_recovery = true;
                }
            }
            (Some(_), None) => {}
            (None, _) => needs_recovery = true,
        }
    }

    let mut recovered = false;
    let mut target_start_pos = 0u64;

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
                let mut line_start_pos = 0u64;

                while let Some((event, bytes_read)) = read_event_from_reader(&mut rec_reader) {
                    let next_pos = line_start_pos + bytes_read;
                    if event.id() == target_id {
                        target_start_pos = line_start_pos;
                        actual_offset = next_pos;
                        recovered = true;
                    }
                    line_start_pos = next_pos;
                }
            }
        }
        if recovered {
            log::info!(
                "Flyline JSONL recovery successful: found last_seen_event_id {:?} at start_pos {} (end offset {})",
                last_seen_event_id,
                target_start_pos,
                actual_offset
            );
        } else {
            log::warn!(
                "Flyline JSONL recovery: could not find last_seen_event_id {:?}; resetting offset to 0",
                last_seen_event_id
            );
        }
    }

    let mut last_seen_start_offset = if recovered {
        Some(target_start_pos)
    } else if start_offset > 0 {
        Some(start_offset)
    } else {
        None
    };

    let _ = file.seek(SeekFrom::Start(actual_offset));
    let mut reader = BufReader::new(&file);
    let mut events = Vec::new();
    let mut line_start_pos = actual_offset;
    let mut last_seen_id = last_seen_event_id.map(String::from);

    while let Some((event, bytes_read)) = read_event_from_reader(&mut reader) {
        last_seen_id = Some(event.id().to_string());
        last_seen_start_offset = Some(line_start_pos);
        line_start_pos += bytes_read;
        events.push(event);
    }

    let result_last_offset = match (last_seen_start_offset, last_seen_id) {
        (Some(byte_offset), Some(event_id)) => Some(LastJsonlReadOffset {
            byte_offset,
            event_id,
        }),
        _ => None,
    };

    log::info!(
        "Fetched {} events from Flyline JSONL history starting at offset {:?}",
        events.len(),
        result_last_offset,
    );

    Ok(JsonlFetchResult {
        events,
        last_read_offset: result_last_offset,
    })
}

pub(super) fn repopulate_jsonl_from_entries(
    entries: &[HistoryEntry],
    session_id: &str,
    target_path: &Path,
) -> anyhow::Result<u64> {
    let default_hostname = Some(shell::backend().hostname()).filter(|h| !h.is_empty());

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
