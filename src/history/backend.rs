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

    if !needs_recovery && last_seen_event_id.is_some() {
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
    } else if last_seen_event_id.is_some() {
        Some(start_offset)
    } else {
        None
    };

    let _ = file.seek(SeekFrom::Start(actual_offset));
    let mut reader = BufReader::new(&file);
    let mut events = Vec::new();
    let mut line_start_pos = actual_offset;
    let mut last_seen_id = last_seen_event_id.map(String::from);
    let mut unparseable_count = 0usize;
    let mut buf = String::new();

    while let Ok(bytes_read) = reader.read_line(&mut buf) {
        if bytes_read == 0 {
            break;
        }
        let trimmed = buf.trim();
        if !trimmed.is_empty() {
            match serde_json::from_str::<HistoryJsonlEvent>(trimmed) {
                Ok(event) => {
                    last_seen_id = Some(event.id().to_string());
                    last_seen_start_offset = Some(line_start_pos);
                    events.push(event);
                }
                Err(_) => {
                    unparseable_count += 1;
                }
            }
        }
        line_start_pos += bytes_read as u64;
        buf.clear();
    }

    if unparseable_count > 0 {
        log::warn!(
            "Failed to parse {} lines from Flyline JSONL history file {:?}",
            unparseable_count,
            path
        );
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

#[derive(Debug, Clone, Default)]
pub struct JsonlNewEntriesResult {
    pub new_entries: Vec<HistoryEntry>,
    pub unmatched_end_events: Vec<HistoryJsonlEvent>,
    pub last_read_offset: Option<LastJsonlReadOffset>,
}

/// Organizes a raw list of JSONL history events into:
/// - `new_entries`: `HistoryEntry` items (with intra-batch Start and End events resolved) sorted by `sort_key`.
/// - `unmatched_end_events`: Unmatched `End` events whose matching `Start` events occurred in earlier batches.
pub fn organize_jsonl_events(
    events: Vec<HistoryJsonlEvent>,
) -> (Vec<HistoryEntry>, Vec<HistoryJsonlEvent>) {
    let mut new_entries = Vec::with_capacity(events.len());
    let mut id_to_idx: std::collections::HashMap<String, usize> =
        std::collections::HashMap::with_capacity(events.len());
    let mut unmatched_end_events = Vec::new();

    for event in events {
        match event {
            HistoryJsonlEvent::Start {
                id,
                timestamp,
                command,
                cwd,
                hostname,
                session,
            } => {
                if command.trim().is_empty() {
                    continue;
                }
                let mut entry = HistoryEntry::new(Some(timestamp.raw_nanos()), 0, command);
                entry.fill_missing_metadata(Some(id.clone()), cwd, hostname, Some(session));
                let idx = new_entries.len();
                new_entries.push(entry);
                id_to_idx.insert(id, idx);
            }
            HistoryJsonlEvent::End {
                id,
                timestamp,
                exit_status,
                pipestatus,
            } => {
                if let Some(&idx) = id_to_idx.get(&id) {
                    if let Some(entry) = new_entries.get_mut(idx) {
                        let duration_ns = entry.timestamp.map(|start_ts| {
                            timestamp.raw_nanos().saturating_sub(start_ts.raw_nanos())
                        });
                        entry.apply_end_metadata(duration_ns, exit_status, pipestatus.as_deref());
                    }
                } else {
                    unmatched_end_events.push(HistoryJsonlEvent::End {
                        id,
                        timestamp,
                        exit_status,
                        pipestatus,
                    });
                }
            }
        }
    }

    new_entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    (new_entries, unmatched_end_events)
}

/// Reads new JSONL history events starting from `last_offset`, resolves intra-batch
/// Start/End event pairs into `HistoryEntry` items, and returns:
/// - `new_entries`: A `Vec<HistoryEntry>` strictly sorted by `sort_key = (timestamp, command)`.
/// - `unmatched_end_events`: Unmatched `End` events whose matching `Start` events occurred in earlier batches.
/// - `last_read_offset`: The new byte offset position in the JSONL file.
pub fn fetch_jsonl_new_entries_from_offset(
    path: &Path,
    last_offset: Option<&LastJsonlReadOffset>,
) -> anyhow::Result<JsonlNewEntriesResult> {
    let fetch_res = fetch_flyline_jsonl_history_from_offset(path, last_offset)?;
    let (new_entries, unmatched_end_events) = organize_jsonl_events(fetch_res.events);
    Ok(JsonlNewEntriesResult {
        new_entries,
        unmatched_end_events,
        last_read_offset: fetch_res.last_read_offset,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonl_history_serialization_and_locking() {
        let session_uuid = uuid::Uuid::now_v7().to_string();
        let cmd_uuid = uuid::Uuid::now_v7().to_string();
        let start_event = HistoryJsonlEvent::Start {
            id: cmd_uuid.clone(),
            timestamp: TimestampNanos::new(1700000000000000000),
            command: "cargo test --lib".to_string(),
            cwd: Some("/home/user/project".to_string()),
            hostname: Some("test-host".to_string()),
            session: session_uuid.clone(),
        };
        let end_event = HistoryJsonlEvent::End {
            id: cmd_uuid.clone(),
            timestamp: TimestampNanos::new(1700000005000000000),
            exit_status: Some(0),
            pipestatus: Some("0".to_string()),
        };

        let start_json = serde_json::to_string(&start_event).unwrap();
        let end_json = serde_json::to_string(&end_event).unwrap();

        assert!(start_json.contains("\"type\":\"start\""));
        assert!(start_json.contains("\"sesh\":\""));
        assert!(start_json.contains("\"cmd\":\"cargo test --lib\""));
        assert!(end_json.contains("\"type\":\"end\""));
    }

    #[test]
    fn test_history_jsonl_tampering_recovery() {
        let temp_file = std::env::temp_dir().join(format!(
            "flyline_test_tamper_{}.jsonl",
            uuid::Uuid::now_v7()
        ));
        let _ = std::fs::remove_file(&temp_file);

        let event1 = HistoryJsonlEvent::Start {
            id: "event-1".to_string(),
            timestamp: TimestampNanos::new(1700000000000000000),
            command: "echo 1".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        let event2 = HistoryJsonlEvent::Start {
            id: "event-2".to_string(),
            timestamp: TimestampNanos::new(1700000001000000000),
            command: "echo 2".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        let event3 = HistoryJsonlEvent::Start {
            id: "event-3".to_string(),
            timestamp: TimestampNanos::new(1700000002000000000),
            command: "echo 3".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };

        append_jsonl_history_event(&event1, &temp_file).unwrap();
        append_jsonl_history_event(&event2, &temp_file).unwrap();

        let res1 = fetch_flyline_jsonl_history_from_offset(&temp_file, None).unwrap();
        assert_eq!(res1.events.len(), 2);
        assert_eq!(
            res1.last_read_offset.as_ref().map(|o| o.event_id.as_str()),
            Some("event-2")
        );

        // Simulate file modification / truncation (file rewritten with event1, event2, event3)
        std::fs::write(&temp_file, "").unwrap();
        append_jsonl_history_event(&event1, &temp_file).unwrap();
        append_jsonl_history_event(&event2, &temp_file).unwrap();
        append_jsonl_history_event(&event3, &temp_file).unwrap();

        // Pass invalid old offset (e.g. 999999) with last_seen_event_id "event-2"
        let bad_offset = LastJsonlReadOffset {
            byte_offset: 999999,
            event_id: "event-2".to_string(),
        };
        let res2 = fetch_flyline_jsonl_history_from_offset(&temp_file, Some(&bad_offset)).unwrap();
        assert_eq!(res2.events.len(), 1);
        assert_eq!(res2.events[0].id(), "event-3");
        assert_eq!(
            res2.last_read_offset.as_ref().map(|o| o.event_id.as_str()),
            Some("event-3")
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_history_jsonl_middle_deletion_recovery() {
        let temp_file = std::env::temp_dir().join(format!(
            "flyline_test_del_rec_{}.jsonl",
            uuid::Uuid::now_v7()
        ));
        let _ = std::fs::remove_file(&temp_file);

        let event1 = HistoryJsonlEvent::Start {
            id: "event-1".to_string(),
            timestamp: TimestampNanos::new(1700000000000000000),
            command: "echo 1".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        let event2 = HistoryJsonlEvent::Start {
            id: "event-2".to_string(),
            timestamp: TimestampNanos::new(1700000001000000000),
            command: "echo 2".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        let event3 = HistoryJsonlEvent::Start {
            id: "event-3".to_string(),
            timestamp: TimestampNanos::new(1700000002000000000),
            command: "echo 3".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };

        append_jsonl_history_event(&event1, &temp_file).unwrap();
        append_jsonl_history_event(&event2, &temp_file).unwrap();

        let res1 = fetch_flyline_jsonl_history_from_offset(&temp_file, None).unwrap();
        assert_eq!(res1.events.len(), 2);
        assert_eq!(
            res1.last_read_offset.as_ref().map(|o| o.event_id.as_str()),
            Some("event-2")
        );
        let last_offset = res1.last_read_offset.unwrap();

        // Simulate deleting event1 from file (file now contains event2, event3)
        std::fs::write(&temp_file, "").unwrap();
        append_jsonl_history_event(&event2, &temp_file).unwrap();
        append_jsonl_history_event(&event3, &temp_file).unwrap();

        // Calling fetch with last_offset (which pointed to event2 before event1 was deleted)
        let res2 = fetch_flyline_jsonl_history_from_offset(&temp_file, Some(&last_offset)).unwrap();
        // Since offset shifted, verification detects event_id mismatch and recovers, reading event3!
        assert_eq!(res2.events.len(), 1);
        assert_eq!(res2.events[0].id(), "event-3");
        assert_eq!(
            res2.last_read_offset.as_ref().map(|o| o.event_id.as_str()),
            Some("event-3")
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_organize_jsonl_events_pairing_and_unmatched() {
        let events = vec![
            HistoryJsonlEvent::Start {
                id: "cmd-1".to_string(),
                timestamp: TimestampNanos::new(100),
                command: "cargo build".to_string(),
                cwd: Some("/work".to_string()),
                hostname: None,
                session: "sess".to_string(),
            },
            HistoryJsonlEvent::Start {
                id: "cmd-2".to_string(),
                timestamp: TimestampNanos::new(200),
                command: "cargo test".to_string(),
                cwd: None,
                hostname: None,
                session: "sess".to_string(),
            },
            HistoryJsonlEvent::End {
                id: "cmd-1".to_string(),
                timestamp: TimestampNanos::new(150),
                exit_status: Some(0),
                pipestatus: None,
            },
            HistoryJsonlEvent::End {
                id: "cmd-earlier".to_string(),
                timestamp: TimestampNanos::new(50),
                exit_status: Some(1),
                pipestatus: Some("1".to_string()),
            },
        ];

        let (entries, unmatched_ends) = organize_jsonl_events(events);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "cargo build");
        assert_eq!(entries[0].exit_status(), Some(0));
        assert_eq!(entries[0].duration_ns(), Some(50));
        assert_eq!(entries[1].command, "cargo test");
        assert_eq!(entries[1].exit_status(), None);

        assert_eq!(unmatched_ends.len(), 1);
        assert_eq!(unmatched_ends[0].id(), "cmd-earlier");
    }

    #[test]
    fn test_fetch_jsonl_new_entries_from_offset_incremental() {
        let temp_file = std::env::temp_dir().join(format!(
            "flyline_test_fetch_entries_{}.jsonl",
            uuid::Uuid::now_v7()
        ));
        let _ = std::fs::remove_file(&temp_file);

        let event1 = HistoryJsonlEvent::Start {
            id: "cmd-1".to_string(),
            timestamp: TimestampNanos::new(100),
            command: "echo first".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        append_jsonl_history_event(&event1, &temp_file).unwrap();

        let res1 = fetch_jsonl_new_entries_from_offset(&temp_file, None).unwrap();
        assert_eq!(res1.new_entries.len(), 1);
        assert_eq!(res1.new_entries[0].command, "echo first");
        assert!(res1.unmatched_end_events.is_empty());

        let event2_end = HistoryJsonlEvent::End {
            id: "cmd-1".to_string(),
            timestamp: TimestampNanos::new(150),
            exit_status: Some(0),
            pipestatus: None,
        };
        append_jsonl_history_event(&event2_end, &temp_file).unwrap();

        let res2 = fetch_jsonl_new_entries_from_offset(&temp_file, res1.last_read_offset.as_ref())
            .unwrap();
        assert!(res2.new_entries.is_empty());
        assert_eq!(res2.unmatched_end_events.len(), 1);
        assert_eq!(res2.unmatched_end_events[0].id(), "cmd-1");

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_fetch_jsonl_unparseable_lines_counted() {
        use std::io::Write;
        let temp_file = std::env::temp_dir().join(format!(
            "flyline_test_unparseable_{}.jsonl",
            uuid::Uuid::now_v7()
        ));
        let _ = std::fs::remove_file(&temp_file);

        let event1 = HistoryJsonlEvent::Start {
            id: "cmd-1".to_string(),
            timestamp: TimestampNanos::new(100),
            command: "echo valid".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        append_jsonl_history_event(&event1, &temp_file).unwrap();

        // Write corrupt/unparseable lines
        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&temp_file)
                .unwrap();
            writeln!(file, "not valid json").unwrap();
            writeln!(file, "{{corrupt json").unwrap();
        }

        let event2 = HistoryJsonlEvent::Start {
            id: "cmd-2".to_string(),
            timestamp: TimestampNanos::new(200),
            command: "echo valid 2".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        append_jsonl_history_event(&event2, &temp_file).unwrap();

        let res = fetch_jsonl_new_entries_from_offset(&temp_file, None).unwrap();
        assert_eq!(res.new_entries.len(), 2);
        assert_eq!(res.new_entries[0].command, "echo valid");
        assert_eq!(res.new_entries[1].command, "echo valid 2");

        let _ = std::fs::remove_file(&temp_file);
    }
}
