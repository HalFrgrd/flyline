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
}

pub fn fetch_flyline_jsonl_history_from_offset(
    start_offset: u64,
) -> anyhow::Result<JsonlFetchResult> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Seek, SeekFrom};
    use std::os::unix::io::AsRawFd;

    let path = flyline_history_jsonl_path();
    if !path.exists() {
        return Ok(JsonlFetchResult::default());
    }

    let mut file = File::open(&path)?;

    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_SH);
    }

    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start_offset = if start_offset > file_len {
        0
    } else {
        start_offset
    };

    if start_offset > 0 {
        let _ = file.seek(SeekFrom::Start(start_offset));
    }

    let mut reader = BufReader::new(&file);
    let mut entries = Vec::new();
    let mut end_updates = Vec::new();
    let mut entry_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut current_offset = start_offset;
    let mut line_buf = String::new();
    let mut line_idx = 0;

    while let Ok(bytes_read) = reader.read_line(&mut line_buf) {
        if bytes_read == 0 {
            break;
        }
        current_offset += bytes_read as u64;

        let line = line_buf.trim();
        if !line.is_empty() {
            if let Ok(event) = serde_json::from_str::<HistoryJsonlEvent>(line) {
                match event {
                    HistoryJsonlEvent::Start {
                        id,
                        timestamp,
                        command,
                        tag,
                        cwd,
                        hostname,
                        session,
                    } => {
                        let mut entry =
                            HistoryEntry::new(Some(timestamp.raw_nanos()), line_idx, command);
                        let meta = entry.metadata_mut();
                        meta.id = Some(id.clone());
                        meta.tag = tag;
                        meta.cwd = cwd;
                        meta.hostname = hostname;
                        meta.session = Some(session);

                        entry_map.insert(id, entries.len());
                        entries.push(entry);
                        line_idx += 1;
                    }
                    HistoryJsonlEvent::End {
                        id,
                        duration_ns,
                        exit_status,
                        pipestatus,
                        ..
                    } => {
                        if let Some(&idx) = entry_map.get(&id) {
                            if let Some(entry) = entries.get_mut(idx) {
                                let meta = entry.metadata_mut();
                                meta.duration_ns = duration_ns;
                                meta.exit_status = exit_status;
                                meta.pipestatus = pipestatus.clone();
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
        let cmd_id = entry
            .id()
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let timestamp = entry.timestamp.unwrap_or(TimestampNanos::ZERO);
        let cwd = entry.cwd().map(String::from);
        let hostname = entry
            .hostname()
            .map(String::from)
            .or_else(|| default_hostname.clone());
        let session = entry
            .session()
            .map(String::from)
            .unwrap_or_else(|| session_id.to_string());

        let start_event = HistoryJsonlEvent::Start {
            id: cmd_id.clone(),
            timestamp,
            command: entry.command.clone(),
            tag: entry.tag(),
            cwd,
            hostname,
            session,
        };
        append_jsonl_history_event_to_path(&start_event, &history_path)?;

        if entry.duration_ns().is_some()
            || entry.exit_status().is_some()
            || entry.pipestatus().is_some()
        {
            let end_event = HistoryJsonlEvent::End {
                id: cmd_id,
                timestamp,
                duration_ns: entry.duration_ns(),
                exit_status: entry.exit_status(),
                pipestatus: entry.pipestatus().map(String::from),
            };
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

pub fn import_atuin_sqlite_file_to(
    sqlite_path: &Path,
    target_jsonl_path: &Path,
) -> anyhow::Result<usize> {
    use std::collections::HashSet;
    use std::io::BufRead;
    use std::process::Command;

    let mut seen_set: HashSet<(u64, String)> = HashSet::new();

    if target_jsonl_path.exists() {
        if let Ok(file) = std::fs::File::open(target_jsonl_path) {
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

        if py_rec.command.trim().is_empty() {
            continue;
        }

        let timestamp = TimestampNanos::new(py_rec.timestamp);
        let ts_secs = timestamp.as_seconds();

        if seen_set.contains(&(ts_secs, py_rec.command.clone())) {
            continue;
        }

        seen_set.insert((ts_secs, py_rec.command.clone()));

        let id = if !py_rec.id.is_empty() {
            py_rec.id
        } else {
            uuid::Uuid::now_v7().to_string()
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

        let start_event = HistoryJsonlEvent::Start {
            id: id.clone(),
            timestamp,
            command: py_rec.command,
            tag: HistoryTag::Normal,
            cwd,
            hostname,
            session,
        };
        append_jsonl_history_event_to_path(&start_event, target_jsonl_path)?;

        if py_rec.duration.is_some() || py_rec.exit.is_some() {
            let end_event = HistoryJsonlEvent::End {
                id,
                timestamp,
                duration_ns: py_rec.duration,
                exit_status: py_rec.exit,
                pipestatus: None,
            };
            append_jsonl_history_event_to_path(&end_event, target_jsonl_path)?;
        }

        imported_count += 1;
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
    let entries = if is_zsh {
        HistoryManager::parse_zsh_history_str(&content)
    } else {
        HistoryManager::parse_bash_history_str(&content)
    };

    let mut seen_set: std::collections::HashSet<(u64, String)> = std::collections::HashSet::new();
    if target_jsonl_path.exists() {
        if let Ok(file) = std::fs::File::open(target_jsonl_path) {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(file);
            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    if let Ok(event) = serde_json::from_str::<HistoryJsonlEvent>(&line) {
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

    let session = uuid::Uuid::now_v7().to_string();
    let mut imported_count = 0;
    let mut entries = entries;
    entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    for entry in entries {
        if entry.command.trim().is_empty() {
            continue;
        }
        let timestamp = entry.timestamp.unwrap_or(TimestampNanos::ZERO);
        let ts_secs = timestamp.as_seconds();

        if seen_set.contains(&(ts_secs, entry.command.clone())) {
            continue;
        }

        seen_set.insert((ts_secs, entry.command.clone()));
        let cmd_id = uuid::Uuid::now_v7().to_string();
        let tag = entry.tag();
        let event = HistoryJsonlEvent::Start {
            id: cmd_id,
            timestamp,
            command: entry.command,
            tag,
            cwd: None,
            hostname: None,
            session: session.clone(),
        };
        append_jsonl_history_event_to_path(&event, target_jsonl_path)?;

        imported_count += 1;
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
