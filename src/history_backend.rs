use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

static FLYLINE_CUSTOM_HISTORY_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_flyline_history_jsonl_path<P: AsRef<Path>>(path: P) {
    let p = path.as_ref().to_path_buf();
    if let Ok(mut lock) = FLYLINE_CUSTOM_HISTORY_PATH.lock() {
        *lock = Some(p);
    }
}

#[allow(dead_code)]
pub fn reset_flyline_history_jsonl_path() {
    if let Ok(mut lock) = FLYLINE_CUSTOM_HISTORY_PATH.lock() {
        *lock = None;
    }
}

pub fn get_custom_flyline_history_jsonl_path() -> Option<PathBuf> {
    if let Ok(lock) = FLYLINE_CUSTOM_HISTORY_PATH.lock() {
        lock.clone()
    } else {
        None
    }
}

pub fn flyline_history_jsonl_path() -> PathBuf {
    if let Some(custom) = get_custom_flyline_history_jsonl_path() {
        if let Some(parent) = custom.parent() {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            let _ = builder.create(parent);
        }
        return custom;
    }
    if let Ok(env_path) = std::env::var("FLYLINE_HISTORY_PATH") {
        if !env_path.trim().is_empty() {
            let path = PathBuf::from(env_path);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            return path;
        }
    }
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("flyline");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    let _ = builder.create(&base);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700));
    }
    base.join("history.jsonl")
}

pub fn append_jsonl_history_event(event: &HistoryJsonlEvent) -> anyhow::Result<()> {
    append_jsonl_history_event_to_path(event, &flyline_history_jsonl_path())
}

pub fn append_jsonl_history_event_to_path(
    event: &HistoryJsonlEvent,
    path: &Path,
) -> anyhow::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    let mut open_options = OpenOptions::new();
    open_options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open_options.mode(0o600);
    }
    let file = open_options.open(path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_EX);
    }

    let mut writer = std::io::BufWriter::new(&file);
    serde_json::to_writer(&mut writer, event)?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }

    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct JsonlFetchResult {
    pub new_entries: Vec<HistoryEntry>,
    pub end_updates: Vec<(String, Option<u64>, Option<i32>, Option<String>)>,
    pub new_offset: u64,
    pub last_seen_event_id: Option<String>,
}

pub fn fetch_flyline_jsonl_history_from_offset(
    start_offset: u64,
    last_seen_event_id: Option<&str>,
) -> anyhow::Result<JsonlFetchResult> {
    fetch_flyline_jsonl_history_from_offset_at_path(
        &flyline_history_jsonl_path(),
        start_offset,
        last_seen_event_id,
    )
}

pub fn fetch_flyline_jsonl_history_from_offset_at_path(
    path: &Path,
    start_offset: u64,
    last_seen_event_id: Option<&str>,
) -> anyhow::Result<JsonlFetchResult> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Seek, SeekFrom};
    use std::os::unix::io::AsRawFd;

    if !path.exists() {
        return Ok(JsonlFetchResult::default());
    }

    let mut file = File::open(path)?;

    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_SH);
    }

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

    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }

    Ok(JsonlFetchResult {
        new_entries: entries,
        end_updates,
        new_offset: current_offset,
        last_seen_event_id: last_seen_id,
    })
}

pub fn repopulate_flyline_jsonl_from_entries(
    entries: &[HistoryEntry],
    session_id: &str,
) -> anyhow::Result<u64> {
    let history_path = flyline_history_jsonl_path();
    let current_bash_hostname = crate::bash_funcs::get_hostname();
    let default_hostname = if !current_bash_hostname.is_empty() {
        Some(current_bash_hostname)
    } else {
        None
    };

    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    for entry in &sorted_entries {
        if entry.command.trim().is_empty() {
            continue;
        }
        let start_event = entry.to_start_event(session_id, default_hostname.as_deref());
        append_jsonl_history_event_to_path(&start_event, &history_path)?;

        if let Some(end_event) = entry.to_end_event() {
            append_jsonl_history_event_to_path(&end_event, &history_path)?;
        }
    }

    let file_len = std::fs::metadata(&history_path)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(file_len)
}

pub fn ensure_flyline_jsonl_exists(session_id: &str, entries: &[HistoryEntry]) {
    let path = flyline_history_jsonl_path();
    if !path.exists()
        || std::fs::metadata(&path)
            .map(|m| m.len() == 0)
            .unwrap_or(true)
    {
        let entries_to_use = if !entries.is_empty() {
            entries.to_vec()
        } else {
            HistoryManager::parse_bash_history_from_memory()
        };
        let _ = repopulate_flyline_jsonl_from_entries(&entries_to_use, session_id);
    }
}

pub fn is_sqlite_db_file(path: &Path) -> bool {
    if let Ok(mut file) = std::fs::File::open(path) {
        use std::io::Read;
        let mut header = [0u8; 16];
        if file.read_exact(&mut header).is_ok() {
            return &header == b"SQLite format 3\0";
        }
    }
    false
}

pub fn load_existing_jsonl_dedup_set(
    target_jsonl_path: &Path,
) -> std::collections::HashSet<(u64, String)> {
    let mut seen_set = std::collections::HashSet::new();
    if target_jsonl_path.exists() {
        if let Ok(file) = std::fs::File::open(target_jsonl_path) {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(file);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(event) = serde_json::from_str::<HistoryJsonlEvent>(trimmed) {
                        if let HistoryJsonlEvent::Start {
                            timestamp, command, ..
                        } = event
                        {
                            seen_set.insert((timestamp.as_seconds(), command));
                        }
                    }
                }
            }
        }
    }
    seen_set
}

pub fn append_imported_entry_to_jsonl(
    target_jsonl_path: &Path,
    seen_set: &mut std::collections::HashSet<(u64, String)>,
    id: Option<String>,
    timestamp: TimestampNanos,
    command: String,
    tag: HistoryTag,
    cwd: Option<String>,
    hostname: Option<String>,
    session: String,
    duration_ns: Option<u64>,
    exit_status: Option<i32>,
    pipestatus: Option<String>,
) -> anyhow::Result<bool> {
    if command.trim().is_empty() {
        return Ok(false);
    }
    let ts_secs = timestamp.as_seconds();
    if seen_set.contains(&(ts_secs, command.clone())) {
        return Ok(false);
    }
    seen_set.insert((ts_secs, command.clone()));

    let cmd_id = id.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    let start_event = HistoryJsonlEvent::Start {
        id: cmd_id.clone(),
        timestamp,
        command,
        tag,
        cwd,
        hostname,
        session,
    };
    append_jsonl_history_event_to_path(&start_event, target_jsonl_path)?;

    if duration_ns.is_some() || exit_status.is_some() || pipestatus.is_some() {
        let end_event = HistoryJsonlEvent::End {
            id: cmd_id,
            timestamp,
            duration_ns,
            exit_status,
            pipestatus,
        };
        append_jsonl_history_event_to_path(&end_event, target_jsonl_path)?;
    }

    Ok(true)
}

pub fn import_atuin_sqlite_file_to(
    sqlite_path: &Path,
    target_jsonl_path: &Path,
) -> anyhow::Result<usize> {
    use std::io::BufRead;
    use std::process::Command;

    let mut seen_set = load_existing_jsonl_dedup_set(target_jsonl_path);

    let py_script = r#"
import sqlite3, sys, json

db_path = sys.argv[1]
try:
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    query = """
    SELECT id, timestamp, duration, exit, command, cwd, session, hostname
    FROM history
    WHERE deleted_at IS NULL
    ORDER BY timestamp ASC
    """
    rows = cursor.execute(query).fetchall()
    for row in rows:
        id_str, ts, dur, exit_code, cmd, cwd, session, host = row
        record = {
            "id": str(id_str) if id_str else "",
            "timestamp": int(ts) if ts is not None else 0,
            "duration": int(dur) if dur is not None and dur >= 0 else None,
            "exit": int(exit_code) if exit_code is not None and exit_code >= 0 else None,
            "command": str(cmd) if cmd else "",
            "cwd": str(cwd) if cwd else "",
            "session": str(session) if session else "",
            "hostname": str(host) if host else ""
        }
        sys.stdout.write(json.dumps(record) + "\n")
except Exception:
    sys.exit(1)
"#;

    let mut cmd = Command::new("python3");
    cmd.args(["-c", py_script, sqlite_path.to_str().unwrap_or("")]);

    let output = crate::bash_funcs::with_sigchld_dfl(|| cmd.output())
        .map_err(|e| anyhow::anyhow!("Failed to execute 'python3': {}", e))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to read Atuin SQLite database using Python3"
        ));
    }

    #[derive(serde::Deserialize)]
    struct AtuinPyRecord {
        id: String,
        timestamp: u64,
        duration: Option<u64>,
        exit: Option<i32>,
        command: String,
        cwd: String,
        session: String,
        hostname: String,
    }

    let mut imported_count = 0;
    let stdout_reader = std::io::BufReader::new(&output.stdout[..]);

    for line in stdout_reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let py_rec: AtuinPyRecord = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let timestamp = TimestampNanos::new(py_rec.timestamp);
        let id = if !py_rec.id.is_empty() {
            Some(py_rec.id)
        } else {
            None
        };
        let cwd = if !py_rec.cwd.is_empty() {
            Some(py_rec.cwd)
        } else {
            None
        };
        let hostname = if !py_rec.hostname.is_empty() {
            Some(py_rec.hostname)
        } else {
            None
        };
        let session = if !py_rec.session.is_empty() {
            py_rec.session
        } else {
            uuid::Uuid::now_v7().to_string()
        };

        if append_imported_entry_to_jsonl(
            target_jsonl_path,
            &mut seen_set,
            id,
            timestamp,
            py_rec.command,
            HistoryTag::Normal,
            cwd,
            hostname,
            session,
            py_rec.duration,
            py_rec.exit,
            None,
        )? {
            imported_count += 1;
        }
    }

    Ok(imported_count)
}

pub fn import_history_file(path: &Path) -> anyhow::Result<usize> {
    import_history_file_to(path, &flyline_history_jsonl_path())
}

pub fn import_history_file_to(path: &Path, target_jsonl_path: &Path) -> anyhow::Result<usize> {
    if is_sqlite_db_file(path) {
        return import_atuin_sqlite_file_to(path, target_jsonl_path);
    }
    let content = std::fs::read_to_string(path)?;
    let is_zsh = content.lines().any(|l| l.starts_with(": "));
    let mut entries = if is_zsh {
        HistoryManager::parse_zsh_history_str(&content)
    } else {
        HistoryManager::parse_bash_history_str(&content)
    };

    let mut seen_set = load_existing_jsonl_dedup_set(target_jsonl_path);
    let session = uuid::Uuid::now_v7().to_string();
    let mut imported_count = 0;
    entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    for entry in entries {
        let timestamp = entry.timestamp.unwrap_or(TimestampNanos::ZERO);
        let id = entry.id().map(String::from);
        let tag = entry.tag();
        let cwd = entry.cwd().map(String::from);
        let hostname = entry.hostname().map(String::from);
        let duration_ns = entry.duration_ns();
        let exit_status = entry.exit_status();
        let pipestatus = entry.pipestatus().map(String::from);

        if append_imported_entry_to_jsonl(
            target_jsonl_path,
            &mut seen_set,
            id,
            timestamp,
            entry.command,
            tag,
            cwd,
            hostname,
            session.clone(),
            duration_ns,
            exit_status,
            pipestatus,
        )? {
            imported_count += 1;
        }
    }

    Ok(imported_count)
}

pub fn import_atuin_history_to(target_jsonl_path: &Path) -> anyhow::Result<usize> {
    let db_path = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("atuin")
        .join("history.db");

    if !db_path.exists() {
        return Err(anyhow::anyhow!(
            "Atuin database file not found at {}",
            db_path.display()
        ));
    }

    import_atuin_sqlite_file_to(&db_path, target_jsonl_path)
}

pub fn import_atuin_history() -> anyhow::Result<usize> {
    import_atuin_history_to(&flyline_history_jsonl_path())
}
