use std::cell::OnceCell;
use std::time::Instant;
use std::vec;

use crate::content_utils::apply_match_indices_to_lines;
use crate::palette::Palette;
use crate::settings::{HistoryBackend, Settings};
use crate::stateful_sliding_window::StatefulSlidingWindow;
use crate::{bash_symbols, content_utils};
use flash::lexer::TokenKind;
use itertools::Itertools;
use ratatui::text::{Line, Span};
use skim::fuzzy_matcher::arinae::ArinaeMatcher;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct TimestampNanos(pub u64);

impl TimestampNanos {
    pub const ZERO: TimestampNanos = TimestampNanos(0);

    pub fn new(raw: u64) -> Self {
        TimestampNanos(crate::content_utils::ensure_timestamp_nanos(raw))
    }

    #[allow(dead_code)]
    pub fn from_nanos(nanos: u64) -> Self {
        TimestampNanos(nanos)
    }

    pub fn now() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        TimestampNanos(nanos)
    }

    pub fn raw_nanos(&self) -> u64 {
        self.0
    }

    pub fn as_seconds(&self) -> u64 {
        self.0 / 1_000_000_000
    }

    pub fn fractional_ns(&self) -> u32 {
        (self.0 % 1_000_000_000) as u32
    }

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn format_timeago_5chars(&self) -> String {
        crate::content_utils::ts_to_timeago_string_5chars(self.as_seconds())
    }

    pub fn format_local_datetime(&self) -> Option<String> {
        if self.is_zero() {
            None
        } else {
            let ts_secs = self.as_seconds() as i64;
            let ts_nanos = self.fractional_ns();
            chrono::DateTime::from_timestamp(ts_secs, ts_nanos).map(|dt| {
                dt.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryTag {
    #[default]
    Normal,
    Cancelled,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryMetadata {
    pub id: Option<String>,
    pub tag: HistoryTag,
    pub cwd: Option<String>,
    pub hostname: Option<String>,
    pub session: Option<String>,
    pub duration_ns: Option<u64>,
    pub exit_status: Option<i32>,
    pub pipestatus: Option<String>,
    pub raw_output: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub timestamp: Option<TimestampNanos>,
    pub index: usize,
    pub command: String,
    pub metadata: Option<Box<HistoryMetadata>>,
    syntax_highlighted: OnceCell<Vec<Line<'static>>>,
}

impl PartialEq for HistoryEntry {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
            && self.index == other.index
            && self.command == other.command
            && self.metadata == other.metadata
    }
}

impl Eq for HistoryEntry {}

impl HistoryEntry {
    pub fn sort_key(&self) -> (u64, &str) {
        (
            self.timestamp.map(|t| t.raw_nanos()).unwrap_or(0),
            &self.command,
        )
    }

    #[allow(dead_code)]
    pub fn metadata(&self) -> Option<&HistoryMetadata> {
        self.metadata.as_deref()
    }

    pub fn metadata_mut(&mut self) -> &mut HistoryMetadata {
        self.metadata.get_or_insert_with(Default::default)
    }

    pub fn id(&self) -> Option<&str> {
        self.metadata.as_ref()?.id.as_deref()
    }

    pub fn tag(&self) -> HistoryTag {
        self.metadata
            .as_ref()
            .map(|m| m.tag)
            .unwrap_or(HistoryTag::Normal)
    }

    pub fn cwd(&self) -> Option<&str> {
        self.metadata.as_ref()?.cwd.as_deref()
    }

    pub fn hostname(&self) -> Option<&str> {
        self.metadata.as_ref()?.hostname.as_deref()
    }

    pub fn session(&self) -> Option<&str> {
        self.metadata.as_ref()?.session.as_deref()
    }

    pub fn duration_ns(&self) -> Option<u64> {
        self.metadata.as_ref()?.duration_ns
    }

    pub fn exit_status(&self) -> Option<i32> {
        self.metadata.as_ref()?.exit_status
    }

    pub fn pipestatus(&self) -> Option<&str> {
        self.metadata.as_ref()?.pipestatus.as_deref()
    }

    pub fn raw_output(&self) -> Option<&str> {
        self.metadata.as_ref()?.raw_output.as_deref()
    }

    pub(crate) fn new(timestamp: Option<u64>, index: usize, command: String) -> Self {
        let timestamp = timestamp.map(TimestampNanos::new);
        HistoryEntry {
            timestamp,
            index,
            command,
            metadata: None,
            syntax_highlighted: OnceCell::new(),
        }
    }

    pub fn get_syntax_highlighted(&self, palette: &Palette) -> &Vec<Line<'static>> {
        self.syntax_highlighted.get_or_init(|| {
            let mut parser = crate::dparser::DParser::from(&self.command as &str);
            parser.walk_to_end();
            let tokens = parser.into_tokens();
            let formatted = crate::app::formatted_buffer::format_buffer(
                &tokens,
                self.command.len(),
                None,
                self.command.len(),
                false,
                palette,
                true,
            );
            let mut lines: Vec<Line<'static>> = vec![];
            let mut current_spans: Vec<Span<'static>> = vec![];
            for part in &formatted.parts {
                if matches!(part.token.token.kind, TokenKind::Newline) {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                } else {
                    current_spans.push(part.normal_span().clone());
                }
            }
            lines.push(Line::from(current_spans));
            lines
        })
    }
}

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

pub fn flyline_history_jsonl_path() -> std::path::PathBuf {
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
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
    path: &std::path::Path,
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

fn repopulate_flyline_jsonl_from_entries(
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

pub fn is_sqlite_db_file(path: &std::path::Path) -> bool {
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
    sqlite_path: &std::path::Path,
    target_jsonl_path: &std::path::Path,
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

pub fn import_history_file(path: &std::path::Path) -> anyhow::Result<usize> {
    import_history_file_to(path, &flyline_history_jsonl_path())
}

pub fn import_history_file_to(
    path: &std::path::Path,
    target_jsonl_path: &std::path::Path,
) -> anyhow::Result<usize> {
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

#[allow(dead_code)]
pub fn import_bash_history_file(path: &std::path::Path) -> anyhow::Result<usize> {
    import_history_file(path)
}

pub fn import_atuin_history() -> anyhow::Result<usize> {
    import_atuin_history_to(&flyline_history_jsonl_path())
}

pub fn import_atuin_history_to(target_jsonl_path: &std::path::Path) -> anyhow::Result<usize> {
    let default_atuin_db = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
        .join("atuin")
        .join("history.db");

    if default_atuin_db.exists() && is_sqlite_db_file(&default_atuin_db) {
        import_atuin_sqlite_file_to(&default_atuin_db, target_jsonl_path)
    } else {
        Err(anyhow::anyhow!(
            "Atuin SQLite database not found at {}",
            default_atuin_db.display()
        ))
    }
}

#[derive(Debug)]
pub struct HistoryManager {
    entries: Vec<HistoryEntry>,
    index: usize,
    last_search_prefix: Option<String>,
    last_buffered_command: Option<String>,
    fuzzy_search: FuzzyHistorySearch,
    last_word_insert_index: Option<usize>,
    history_backend: HistoryBackend,
    last_loaded_external_count: usize,
    last_read_jsonl_byte_offset: u64,
    session_id: String,
    default_tag: HistoryTag,
}

pub enum HistorySearchDirection {
    Backward,
    Forward,
    PageBackward,
    PageForward,
}

impl HistoryManager {
    fn log_recent_entries(entries: &[HistoryEntry], source: &str) {
        if entries.is_empty() {
            log::warn!("No {} history entries found", source);
            return;
        }

        log::debug!("Loaded {} {} history entries", entries.len(), source);
        for entry in entries.iter().rev().take(3) {
            log::debug!("{}_entries => {:?}", source, entry);
        }
    }

    fn push_deduped_entry(entries: &mut Vec<HistoryEntry>, mut entry: HistoryEntry) {
        if let Some(prev) = entries.last_mut() {
            let prev_secs = prev.timestamp.map(|t| t.as_seconds()).unwrap_or(0);
            let entry_secs = entry.timestamp.map(|t| t.as_seconds()).unwrap_or(0);
            if prev.command == entry.command
                && (prev_secs == entry_secs || prev_secs == 0 || entry_secs == 0)
            {
                if entry_secs >= prev_secs {
                    if entry.timestamp.is_some() {
                        prev.timestamp = entry.timestamp;
                    }
                    if entry.metadata.is_some() {
                        prev.metadata = entry.metadata;
                    }
                }
                return;
            }
        }

        entry.index = entries.len();
        entries.push(entry);
    }

    fn normalize_entries(mut entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
        entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        let mut normalized = Vec::with_capacity(entries.len());
        for entry in entries {
            Self::push_deduped_entry(&mut normalized, entry);
        }
        for (i, entry) in normalized.iter_mut().enumerate() {
            entry.index = i;
        }
        normalized
    }

    fn merge_history_entries(
        zsh_entries: Vec<HistoryEntry>,
        bash_entries: Vec<HistoryEntry>,
    ) -> Vec<HistoryEntry> {
        let mut all = zsh_entries;
        all.extend(bash_entries);
        Self::normalize_entries(all)
    }

    /// Read the user's bash history file into a Vec<String>.
    /// Tries $HISTFILE first, otherwise falls back to $HOME/.bash_history.
    #[allow(dead_code)]
    fn parse_bash_history_from_file() -> Vec<HistoryEntry> {
        let hist_path = std::env::var("HISTFILE").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.bash_history", home)
        });

        log::debug!("Reading bash history from: {}", hist_path);

        let content = std::fs::read_to_string(hist_path).unwrap_or_default();
        let res = time_it!(
            "parse bash history",
            HistoryManager::parse_bash_history_str(&content)
        );

        log::debug!("Parsed bash history ({} entries)", res.len());
        res
    }

    pub fn parse_bash_history_from_memory() -> Vec<HistoryEntry> {
        let mut res = Vec::with_capacity(4096);
        unsafe {
            let hist_array = bash_symbols::history_list();
            if hist_array.is_null() {
                log::warn!("History list is null");
                return res;
            }

            let mut index = 0;
            loop {
                let entry_ptr = *hist_array.offset(index);
                if entry_ptr.is_null() {
                    break;
                }

                let hist_entry = &*entry_ptr;

                // Check if line pointer is valid before dereferencing
                if !hist_entry.line.is_null() {
                    let command_cstr = std::ffi::CStr::from_ptr(hist_entry.line);
                    let command_str = command_cstr.to_string_lossy().into_owned();

                    // Parse timestamp if available
                    let timestamp = if !hist_entry.timestamp.is_null() {
                        let timestamp_cstr = std::ffi::CStr::from_ptr(hist_entry.timestamp);
                        if let Ok(timestamp_str) = timestamp_cstr.to_str() {
                            // If there are no timestamps in the history file,
                            // Bash will use the current time for all entries, which can lead to many identical timestamps.
                            let ts_str = timestamp_str.trim_start_matches('#').trim();
                            ts_str.parse::<u64>().ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let entry = HistoryEntry::new(timestamp, index as usize, command_str);
                    res.push(entry);
                }

                index += 1;
            }
        }
        res
    }

    fn parse_zsh_history(custom_path: Option<&str>) -> Vec<HistoryEntry> {
        let hist_path = match custom_path {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/.zsh_history", home)
            }
        };

        log::debug!("Reading Zsh history from: {}", hist_path);

        let content = match std::fs::read(&hist_path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    // The file contains invalid UTF-8; fall back to a lossy conversion
                    let bytes = e.into_bytes();
                    log::warn!(
                        "Zsh history at {} contains invalid UTF-8, using lossy conversion",
                        hist_path
                    );
                    String::from_utf8_lossy(&bytes).into_owned()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("flyline: Zsh history file not found: {}", hist_path);
                log::warn!("Zsh history file not found: {}", hist_path);
                String::new()
            }
            Err(e) => {
                log::error!("Failed to read Zsh history from {}: {}", hist_path, e);
                String::new()
            }
        };
        let res = time_it!(
            "parse zsh history",
            HistoryManager::parse_zsh_history_str(&content)
        );

        log::debug!("Parsed Zsh history ({} entries)", res.len());
        res
    }

    pub fn new(settings: &Settings) -> HistoryManager {
        Self::new_with_tag(settings, HistoryTag::Normal)
    }

    pub fn new_with_tag(settings: &Settings, default_tag: HistoryTag) -> HistoryManager {
        let history_backend = settings.history_backend;
        let mut last_loaded_external_count = 0;
        let mut last_read_jsonl_byte_offset = 0;

        let entries = if history_backend == HistoryBackend::Flyline {
            match fetch_flyline_jsonl_history_from_offset(0) {
                Ok(fetch_res) if !fetch_res.new_entries.is_empty() => {
                    let matching: Vec<HistoryEntry> = fetch_res
                        .new_entries
                        .into_iter()
                        .filter(|e| e.tag() == default_tag)
                        .collect();
                    last_loaded_external_count = matching.len();
                    last_read_jsonl_byte_offset = fetch_res.new_offset;
                    Self::log_recent_entries(&matching, "flyline_jsonl");
                    Self::normalize_entries(matching)
                }
                _ => {
                    let bash_entries = Self::parse_bash_history_from_memory();
                    if !bash_entries.is_empty() {
                        if let Ok(new_offset) = repopulate_flyline_jsonl_from_entries(
                            &bash_entries,
                            &settings.session_id,
                        ) {
                            last_read_jsonl_byte_offset = new_offset;
                        }
                    }
                    last_loaded_external_count = bash_entries.len();
                    Self::log_recent_entries(&bash_entries, "bash");
                    Self::normalize_entries(bash_entries)
                }
            }
        } else if let Some(ref zsh_path) = settings.zsh_history_path {
            let zsh_entries = Self::parse_zsh_history(Some(zsh_path.as_str()));
            let bash_entries = Self::parse_bash_history_from_memory();
            Self::log_recent_entries(&zsh_entries, "Zsh");
            Self::log_recent_entries(&bash_entries, "bash");
            Self::merge_history_entries(zsh_entries, bash_entries)
        } else {
            let bash_entries = Self::parse_bash_history_from_memory();
            Self::log_recent_entries(&bash_entries, "bash");
            Self::normalize_entries(bash_entries)
        };

        let index = entries.len();
        HistoryManager {
            entries,
            index,
            last_search_prefix: None,
            last_buffered_command: None,
            fuzzy_search: FuzzyHistorySearch::new(),
            last_word_insert_index: None,
            history_backend,
            last_loaded_external_count,
            last_read_jsonl_byte_offset,
            session_id: settings.session_id.clone(),
            default_tag,
        }
    }

    /// Create an empty `HistoryManager` that starts with no entries.
    /// New entries are added at runtime via `push_entry`.
    #[allow(dead_code)]
    pub fn new_empty() -> HistoryManager {
        Self::new_empty_with_tag(HistoryTag::Normal)
    }

    pub fn new_empty_with_tag(default_tag: HistoryTag) -> HistoryManager {
        HistoryManager {
            entries: Vec::new(),
            index: 0,
            last_search_prefix: None,
            last_buffered_command: None,
            fuzzy_search: FuzzyHistorySearch::new(),
            last_word_insert_index: None,
            history_backend: HistoryBackend::Flyline,
            last_loaded_external_count: 0,
            last_read_jsonl_byte_offset: 0,
            session_id: uuid::Uuid::now_v7().to_string(),
            default_tag,
        }
    }

    /// Refreshes history entries incrementally from the active backend.
    ///
    /// When using `HistoryBackend::Flyline`, queries ~/.local/share/flyline/history.jsonl.
    /// When using `HistoryBackend::Atuin`, queries the Atuin database for new entries.
    /// When using `HistoryBackend::Bash`, re-checks Bash memory history.
    pub fn refresh_history_backend(&mut self) {
        if self.history_backend == HistoryBackend::Flyline {
            let path = flyline_history_jsonl_path();
            if !path.exists()
                || std::fs::metadata(&path)
                    .map(|m| m.len() == 0)
                    .unwrap_or(true)
            {
                let entries_to_use = if !self.entries.is_empty() {
                    self.entries.clone()
                } else {
                    Self::parse_bash_history_from_memory()
                };
                if let Ok(offset) =
                    repopulate_flyline_jsonl_from_entries(&entries_to_use, &self.session_id)
                {
                    self.last_read_jsonl_byte_offset = offset;
                }
            } else if let Ok(fetch_res) =
                fetch_flyline_jsonl_history_from_offset(self.last_read_jsonl_byte_offset)
            {
                for (id, duration_ns, exit_status, pipestatus) in fetch_res.end_updates {
                    self.update_entry_end_metadata(&id, duration_ns, exit_status, pipestatus);
                }
                let matching_new_entries: Vec<HistoryEntry> = fetch_res
                    .new_entries
                    .into_iter()
                    .filter(|e| e.tag() == self.default_tag)
                    .collect();
                if !matching_new_entries.is_empty() {
                    log::debug!(
                        "Refreshed Flyline JSONL history: loaded {} matching entries from byte offset {}",
                        matching_new_entries.len(),
                        self.last_read_jsonl_byte_offset
                    );
                    for entry in matching_new_entries {
                        Self::push_deduped_entry(&mut self.entries, entry);
                    }
                    self.entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
                    for (i, entry) in self.entries.iter_mut().enumerate() {
                        entry.index = i;
                    }
                    self.last_loaded_external_count = self.entries.len();
                    self.fuzzy_search.clear_cache();
                }
                self.last_read_jsonl_byte_offset = fetch_res.new_offset;
                self.index = self.entries.len();
            }
        }
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Push a new entry to the history list.
    /// `self.index` is kept at `entries.len()` (past-the-end), matching the
    /// invariant established by `new()` and `HistoryManager::search_in_history`.
    /// Resets the fuzzy search cache so the new entry is visible immediately.
    pub fn push_entry(&mut self, command: String) -> String {
        let command_id = uuid::Uuid::now_v7().to_string();
        if command.trim().is_empty() {
            return command_id;
        }

        let bash_cwd = crate::bash_funcs::get_cwd();
        let cwd = if !bash_cwd.is_empty() {
            Some(bash_cwd)
        } else {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        };
        let bash_hostname = crate::bash_funcs::get_hostname();
        let hostname = if !bash_hostname.is_empty() {
            Some(bash_hostname)
        } else {
            None
        };

        let now_ts = TimestampNanos::now();
        if self.history_backend == HistoryBackend::Flyline {
            let event = HistoryJsonlEvent::Start {
                id: command_id.clone(),
                timestamp: now_ts,
                command: command.clone(),
                tag: self.default_tag,
                cwd: cwd.clone(),
                hostname: hostname.clone(),
                session: self.session_id.clone(),
            };
            if let Err(e) = append_jsonl_history_event(&event) {
                log::warn!("Failed to write start event to JSONL history: {}", e);
            }
        }

        let index = self.entries.len();
        let mut entry = HistoryEntry::new(Some(now_ts.raw_nanos()), index, command);
        let meta = entry.metadata_mut();
        meta.id = Some(command_id.clone());
        meta.tag = self.default_tag;
        meta.cwd = cwd;
        meta.hostname = hostname;
        meta.session = Some(self.session_id.clone());
        self.entries.push(entry);
        self.index = self.entries.len();
        self.last_word_insert_index = None;
        self.fuzzy_search.clear_cache();

        command_id
    }

    pub fn update_entry_end_metadata(
        &mut self,
        id: &str,
        duration_ns: Option<u64>,
        exit_status: Option<i32>,
        pipestatus: Option<String>,
    ) {
        let found = self.entries.iter_mut().rev().find(|e| e.id() == Some(id));
        if let Some(entry) = found {
            let meta = entry.metadata_mut();
            meta.duration_ns = duration_ns;
            meta.exit_status = exit_status;
            meta.pipestatus = pipestatus;
        } else if let Some(last) = self.entries.last_mut() {
            let meta = last.metadata_mut();
            meta.duration_ns = duration_ns;
            meta.exit_status = exit_status;
            meta.pipestatus = pipestatus;
        }
    }

    pub fn set_last_raw_output(&mut self, raw_output: String) {
        if let Some(last) = self.entries.last_mut() {
            last.metadata_mut().raw_output = Some(raw_output);
        }
    }

    pub fn get_last_word_insert_command(&self) -> Option<&str> {
        let idx = self.last_word_insert_index?;
        self.entries.get(idx).map(|e| e.command.as_str())
    }

    pub fn last_word_insert_move_prev(&mut self) -> Option<&str> {
        let mut start_idx = self.last_word_insert_index.unwrap_or(self.entries.len());
        while start_idx > 0 {
            start_idx -= 1;
            if let Some(entry) = self.entries.get(start_idx) {
                if get_last_word(&entry.command).is_some() {
                    self.last_word_insert_index = Some(start_idx);
                    return Some(entry.command.as_str());
                }
            }
        }
        None
    }

    pub fn last_word_insert_reset(&mut self) {
        self.last_word_insert_index = None;
    }

    fn parse_timestamp(line: &str) -> Option<u64> {
        if let Some(stripped) = line.strip_prefix('#') {
            stripped.trim().parse::<u64>().ok()
        } else {
            None
        }
    }

    fn parse_bash_history_str(s: &str) -> Vec<HistoryEntry> {
        let mut res = Vec::<HistoryEntry>::new();
        let mut current_ts: Option<u64> = None;
        let mut current_cmd_lines: Vec<&str> = Vec::new();
        let mut has_seen_timestamp = false;

        for line in s.lines() {
            if let Some(ts) = HistoryManager::parse_timestamp(line) {
                has_seen_timestamp = true;

                let cmd_str = current_cmd_lines.join("\n");
                let trimmed = cmd_str.trim();
                if !trimmed.is_empty() {
                    let entry = HistoryEntry::new(current_ts, res.len(), trimmed.to_string());
                    res.push(entry);
                }
                current_cmd_lines.clear();
                current_ts = Some(ts);
            } else if has_seen_timestamp && current_ts.is_some() {
                if current_cmd_lines.len() >= 100 {
                    let cmd_str = current_cmd_lines.join("\n");
                    let trimmed = cmd_str.trim();
                    if !trimmed.is_empty() {
                        let entry = HistoryEntry::new(current_ts, res.len(), trimmed.to_string());
                        res.push(entry);
                    }
                    current_cmd_lines.clear();
                    current_ts = None;
                }
                current_cmd_lines.push(line);
            } else {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let entry = HistoryEntry::new(None, res.len(), trimmed.to_string());
                    res.push(entry);
                }
            }
        }

        let cmd_str = current_cmd_lines.join("\n");
        let trimmed = cmd_str.trim();
        if !trimmed.is_empty() {
            let entry = HistoryEntry::new(current_ts, res.len(), trimmed.to_string());
            res.push(entry);
        }

        res
    }

    fn parse_zsh_history_str(s: &str) -> Vec<HistoryEntry> {
        let mut res = Vec::<HistoryEntry>::new();

        for line in s.lines() {
            if line.trim().is_empty() {
                continue;
            }

            // Zsh extended history format: ": timestamp:duration;command"
            // Simple format: "command"
            let (timestamp, command) = if line.starts_with(": ") {
                // Extended history format
                if let Some(rest) = line.strip_prefix(": ") {
                    if let Some((ts_dur, cmd)) = rest.split_once(';') {
                        // ts_dur is like "1234567890:0"
                        let timestamp = ts_dur
                            .split(':')
                            .next()
                            .and_then(|ts| ts.parse::<u64>().ok());
                        (timestamp, cmd.to_string())
                    } else {
                        // Malformed extended format, treat as simple
                        (None, line.to_string())
                    }
                } else {
                    (None, line.to_string())
                }
            } else {
                // Simple format (no timestamp)
                (None, line.to_string())
            };

            let entry = HistoryEntry::new(timestamp, res.len(), command);
            res.push(entry);
        }

        res
    }

    pub fn get_command_suggestion_suffix(
        &mut self,
        command: &str,
    ) -> Option<(HistoryEntry, String)> {
        for entry in self.entries.iter().take(self.index).rev() {
            if entry.command.starts_with(command) {
                return Some((entry.clone(), entry.command[command.len()..].to_string()));
            }
        }
        None
    }

    pub fn search_in_history(
        &mut self,
        current_cmd: &str,
        direction: HistorySearchDirection,
    ) -> Option<HistoryEntry> {
        let is_command_different_to_last_buffered = self
            .last_buffered_command
            .as_ref()
            .is_none_or(|c| c != current_cmd);

        if self.last_search_prefix.is_none() || is_command_different_to_last_buffered {
            self.last_search_prefix = Some(current_cmd.to_string());
        }

        let prefix = self.last_search_prefix.as_ref().unwrap();

        let indices: Vec<usize> = match direction {
            HistorySearchDirection::Backward | HistorySearchDirection::PageBackward => {
                (0..self.index).rev().collect()
            }
            HistorySearchDirection::Forward | HistorySearchDirection::PageForward => {
                (self.index + 1..self.entries.len()).collect()
            }
        };

        for i in indices {
            let entry = &self.entries[i];
            if entry.command.starts_with(prefix) && entry.command != current_cmd {
                self.last_buffered_command = Some(entry.command.clone());
                // Update the index only when found.
                self.index = i;
                return Some(entry.clone());
            }
        }

        None
    }

    pub(crate) fn get_fuzzy_search_results(
        &mut self,
        current_cmd: &str,
        max_visible: usize,
        default_index: Option<usize>,
    ) -> (
        &[HistoryEntry],
        &[HistoryEntryFormatted],
        Option<usize>,
        usize,
        usize,
    ) {
        let (formatted, idx, num_results, num_searched) = self
            .fuzzy_search
            .get_fuzzy_search_results(&self.entries, current_cmd, max_visible, default_index);
        (&self.entries, formatted, idx, num_results, num_searched)
    }

    /// Pre-warm the fuzzy search cache when entering fuzzy-search mode.
    /// Uses the default visible window size as the actual terminal size is not yet
    /// available at keypress time.  The render path will call `get_fuzzy_search_results`
    /// with the correct dynamic size on the next frame.
    pub(crate) fn warm_fuzzy_search_cache(
        &mut self,
        current_cmd: &str,
        default_index: Option<usize>,
    ) {
        self.fuzzy_search.set_fuzzy_search_idx(default_index);
        let _ = self.fuzzy_search.get_fuzzy_search_results(
            &self.entries,
            current_cmd,
            FuzzyHistorySearch::VISIBLE_CACHE_SIZE,
            default_index,
        );
    }

    pub fn accept_fuzzy_search_result(&self) -> Option<&HistoryEntry> {
        self.fuzzy_search.accept_fuzzy_search_result(&self.entries)
    }

    pub fn fuzzy_search_set_idx(&mut self, idx: Option<usize>) {
        self.fuzzy_search.set_fuzzy_search_idx(idx);
    }

    pub fn fuzzy_search_idx(&self) -> Option<usize> {
        self.fuzzy_search.cache_index
    }

    pub fn fuzzy_search_onkeypress(&mut self, direction: HistorySearchDirection) {
        self.fuzzy_search.fuzzy_search_onkeypress(direction);
    }

    pub fn fuzzy_search_command_by_idx(&self, idx: usize) -> Option<String> {
        self.fuzzy_search
            .cache
            .get(idx)
            .and_then(|formatted| self.entries.get(formatted.entry_index))
            .map(|entry| entry.command.clone())
    }

    // fuzzy search cache logic moved to FuzzyHistorySearch
}

#[derive(Debug)]
pub(crate) struct HistoryEntryFormatted {
    pub entry_index: usize,
    pub score: i64,
    pub match_indices: Vec<usize>,
    command_spans: OnceCell<Vec<Line<'static>>>,
    pub idx_in_cache: Option<usize>,
}

impl std::cmp::Eq for HistoryEntryFormatted {}
impl std::cmp::PartialEq for HistoryEntryFormatted {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl std::cmp::Ord for HistoryEntryFormatted {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .score
            .cmp(&self.score)
            .then_with(|| other.entry_index.cmp(&self.entry_index))
    }
}
impl std::cmp::PartialOrd for HistoryEntryFormatted {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl HistoryEntryFormatted {
    pub(crate) fn new(entry_index: usize, score: i64, match_indices: Vec<usize>) -> Self {
        HistoryEntryFormatted {
            entry_index,
            score,
            match_indices,
            command_spans: OnceCell::new(),
            idx_in_cache: None,
        }
    }

    pub fn command_spans(
        &self,
        entries: &[HistoryEntry],
        palette: &Palette,
    ) -> &Vec<Line<'static>> {
        self.command_spans.get_or_init(|| {
            let entry = &entries[self.entry_index];
            let base_lines = entry.get_syntax_highlighted(palette);
            apply_match_indices_to_lines(palette, base_lines, &self.match_indices)
        })
    }
}

struct FuzzyHistorySearch {
    matcher: ArinaeMatcher,
    cache: Vec<HistoryEntryFormatted>,
    cache_command: Option<String>,
    global_index: usize,
    cache_index: Option<usize>,
    window: StatefulSlidingWindow,
}

impl std::fmt::Debug for FuzzyHistorySearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FuzzyHistorySearch")
            .field("cache_command", &self.cache_command)
            .field("global_index", &self.global_index)
            .field("cache_index", &self.cache_index)
            .field("window", &self.window)
            .field("cache_len", &self.cache.len())
            .finish()
    }
}

impl FuzzyHistorySearch {
    // Check time budget every N entries to balance responsiveness and performance
    const TIME_CHECK_INTERVAL: usize = 64;
    // Time budget for processing history entries in milliseconds
    const TIME_BUDGET_MS: u64 = 20;
    // Number of visible rows in the fuzzy history search list
    const VISIBLE_CACHE_SIZE: usize = 18;
    // Number of recent cache entries to check for duplicates before inserting
    const DUPLICATE_CHECK_WINDOW: usize = 50;

    fn merge_sort_and_window_dedup(
        &mut self,
        sorted_new_cache_entries: Vec<HistoryEntryFormatted>,
        entries: &[HistoryEntry],
    ) {
        if sorted_new_cache_entries.is_empty() {
            return;
        }

        let old_cache = std::mem::take(&mut self.cache);
        self.cache = old_cache
            .into_iter()
            .merge(sorted_new_cache_entries)
            .collect();

        let mut deduped: Vec<HistoryEntryFormatted> = Vec::with_capacity(self.cache.len());
        for entry in self.cache.drain(..) {
            let entry_trimmed = entries[entry.entry_index].command.trim();
            let is_duplicate = deduped
                .iter()
                .rev()
                .take(Self::DUPLICATE_CHECK_WINDOW)
                .any(|e| entries[e.entry_index].command.trim() == entry_trimmed);

            if !is_duplicate {
                deduped.push(entry);
            }
        }

        self.cache = deduped;
    }

    fn new() -> Self {
        FuzzyHistorySearch {
            matcher: ArinaeMatcher::new(skim::CaseMatching::Smart, true),
            cache: Vec::new(),
            cache_command: None,
            global_index: 0,
            cache_index: Some(0),
            window: StatefulSlidingWindow::new(0, Self::VISIBLE_CACHE_SIZE, 0, None),
        }
    }

    fn clear_cache(&mut self) {
        self.cache.clear();
        self.cache_command = None;
        self.global_index = 0;
        self.cache_index = Some(0);
        self.window = StatefulSlidingWindow::new(0, Self::VISIBLE_CACHE_SIZE, 0, None);
    }

    fn get_fuzzy_search_results(
        &mut self,
        entries: &[HistoryEntry],
        current_cmd: &str,
        max_visible: usize,
        default_index: Option<usize>,
    ) -> (&[HistoryEntryFormatted], Option<usize>, usize, usize) {
        // when the command changes, reset the cache
        if Some(current_cmd.to_string()) != self.cache_command {
            self.cache_command = Some(current_cmd.to_string());
            self.cache = vec![];
            self.global_index = 0;
            self.cache_index = default_index;
            self.window = StatefulSlidingWindow::new(0, Self::VISIBLE_CACHE_SIZE, 0, None);
        }

        self.grow_fuzzy_search_cache(entries, current_cmd);

        let cache_len = self.cache.len();

        self.window.update_max_index(cache_len);
        self.window.update_window_size(max_visible);
        self.window.move_index_to(self.cache_index.unwrap_or(0));

        let entries_to_show = &mut self.cache[self.window.get_window_range()];
        entries_to_show.iter_mut().enumerate().for_each(|(idx, e)| {
            e.idx_in_cache = Some(self.window.get_window_range().start + idx);
        });

        (
            entries_to_show,
            self.cache_index,
            cache_len,
            self.global_index,
        )
    }

    fn accept_fuzzy_search_result<'a>(
        &self,
        entries: &'a [HistoryEntry],
    ) -> Option<&'a HistoryEntry> {
        self.cache_index
            .and_then(|idx| self.cache.get(idx))
            .map(|formatted| &entries[formatted.entry_index])
    }

    fn set_fuzzy_search_idx(&mut self, idx: Option<usize>) {
        self.cache_index = idx.and_then(|i| {
            if self.cache.is_empty() {
                None
            } else {
                Some(i.min(self.cache.len().saturating_sub(1)))
            }
        });
    }

    fn fuzzy_search_onkeypress(&mut self, direction: HistorySearchDirection) {
        if self.cache.is_empty() {
            return;
        }
        let current_idx = match self.cache_index {
            Some(idx) => idx,
            None => {
                self.cache_index = Some(0);
                return;
            }
        };
        match direction {
            HistorySearchDirection::Backward => {
                if current_idx + 1 < self.cache.len() {
                    self.cache_index = Some(current_idx + 1);
                }
            }
            HistorySearchDirection::Forward => {
                if current_idx > 0 {
                    self.cache_index = Some(current_idx - 1);
                }
            }
            HistorySearchDirection::PageBackward => {
                self.cache_index = Some(
                    (current_idx + self.window.get_window_range().len()).min(self.cache.len() - 1),
                );
            }
            HistorySearchDirection::PageForward => {
                self.cache_index =
                    Some(current_idx.saturating_sub(self.window.get_window_range().len()));
            }
        }
    }

    fn grow_fuzzy_search_cache(&mut self, entries: &[HistoryEntry], current_cmd: &str) {
        let start = Instant::now();
        let start_index = self.global_index;
        let time_budget = std::time::Duration::from_millis(Self::TIME_BUDGET_MS);

        let mut new_cache_entries = Vec::with_capacity(256);

        // Process as many entries as possible within the time budget
        for (iter_idx, _) in entries.iter().rev().skip(self.global_index).enumerate() {
            // Check if we've exceeded the time budget every TIME_CHECK_INTERVAL entries
            if iter_idx % Self::TIME_CHECK_INTERVAL == 0 && start.elapsed() >= time_budget {
                break;
            }

            // entry_index in the original entries slice: entries are iterated in reverse,
            // so the current entry is at entries.len() - 1 - self.global_index (before increment).
            let entry_index = entries.len() - 1 - self.global_index;
            let entry = &entries[entry_index];

            if let Some((score, indices)) = content_utils::fuzzy_indices_with_threshold(
                &self.matcher,
                &entry.command,
                current_cmd,
                content_utils::FuzzyMatchThreshold::Medium,
            ) {
                new_cache_entries.push(HistoryEntryFormatted::new(entry_index, score, indices));
            }
            self.global_index += 1;
        }

        new_cache_entries.sort();
        self.merge_sort_and_window_dedup(new_cache_entries, entries);

        if start_index != self.global_index {
            let duration = start.elapsed();
            log::debug!("Fuzzy cache increase took: {:?}", duration);
        }
    }
}

pub fn get_last_word(command: &str) -> Option<String> {
    let tokens = crate::dparser::DParser::parse_and_annotate(command);
    if tokens.is_empty() {
        return None;
    }

    let is_boundary_token = |kind: &TokenKind| -> bool {
        matches!(
            kind,
            TokenKind::Whitespace(_)
                | TokenKind::Newline
                | TokenKind::Semicolon
                | TokenKind::DoubleSemicolon
                | TokenKind::And
                | TokenKind::Background
                | TokenKind::Or
                | TokenKind::Pipe
                | TokenKind::Less
                | TokenKind::Great
                | TokenKind::DGreat
                | TokenKind::InputDup
                | TokenKind::OutputDup
                | TokenKind::ReadWrite
                | TokenKind::Clobber
                | TokenKind::HereDoc { .. }
                | TokenKind::HereDocDash { .. }
                | TokenKind::HereString
        )
    };

    let mut end_idx = None;
    for i in (0..tokens.len()).rev() {
        let kind = &tokens[i].token.kind;
        if !matches!(kind, TokenKind::Whitespace(_) | TokenKind::Newline)
            && !is_boundary_token(kind)
        {
            end_idx = Some(i);
            break;
        }
    }

    let end_idx = end_idx?;
    let mut curr_idx = end_idx;
    let mut start_idx = end_idx;

    loop {
        let mut jumped = false;
        if let Some(closing) = &tokens[curr_idx].annotations.closing {
            if closing.opening_idx < curr_idx {
                curr_idx = closing.opening_idx;
                jumped = true;
            }
        }

        if !jumped && is_boundary_token(&tokens[curr_idx].token.kind) {
            break;
        }

        start_idx = curr_idx;

        if curr_idx == 0 {
            break;
        }
        curr_idx -= 1;
    }

    let start_byte = tokens[start_idx].token.byte_range().start;
    let end_byte = tokens[end_idx].token.byte_range().end;

    if start_byte <= end_byte && end_byte <= command.len() {
        Some(command[start_byte..end_byte].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp() {
        assert_eq!(HistoryManager::parse_timestamp("#12345"), Some(12345));
        assert_eq!(HistoryManager::parse_timestamp("12345"), None);
        assert_eq!(HistoryManager::parse_timestamp("#not_a_number"), None);
    }

    #[test]
    fn test_parse_bash_history() {
        const TEST_HISTORY: &str = r"#1625078400
ls -al
#1625078460
echo 'Hello, World!'
pwd
#cd /asdf/asdf
cd /home/user
#1625078460
#1625078460
#1625078460
cd /home/user2
";
        let entries = HistoryManager::parse_bash_history_str(TEST_HISTORY);
        for entry in &entries {
            println!(
                "Timestamp: {:?}, Command: {}",
                entry.timestamp, entry.command
            );
        }
        assert_eq!(entries.len(), 3);

        let mut entries_iter = entries.iter();

        let mut check = |expected_ts: Option<u64>, expected_index: usize, expected_cmd: &str| {
            let entry = entries_iter.next().unwrap();
            assert_eq!(entry.timestamp, expected_ts.map(TimestampNanos::new));
            assert_eq!(entry.index, expected_index);
            assert_eq!(entry.command, expected_cmd);
        };

        check(Some(1625078400_000_000_000), 0, "ls -al");
        check(
            Some(1625078460_000_000_000),
            1,
            "echo 'Hello, World!'\npwd\n#cd /asdf/asdf\ncd /home/user",
        );
        check(Some(1625078460_000_000_000), 2, "cd /home/user2");
    }

    #[test]
    fn test_parse_zsh_history() {
        // Test simple format (no timestamps)
        const SIMPLE_HISTORY: &str = r"cd ~
ls -la
git status
";
        let entries = HistoryManager::parse_zsh_history_str(SIMPLE_HISTORY);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].command, "cd ~");
        assert_eq!(entries[0].timestamp, None);
        assert_eq!(entries[1].command, "ls -la");
        assert_eq!(entries[2].command, "git status");

        // Test extended format (with timestamps)
        const EXTENDED_HISTORY: &str = r": 1625078400:0;ls -al
: 1625078460:5;echo 'Hello, World!'
: 1625078520:0;cd /tmp
";
        let entries = HistoryManager::parse_zsh_history_str(EXTENDED_HISTORY);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].command, "ls -al");
        assert_eq!(
            entries[0].timestamp,
            Some(TimestampNanos::new(1625078400_000_000_000))
        );
        assert_eq!(entries[1].command, "echo 'Hello, World!'");
        assert_eq!(
            entries[1].timestamp,
            Some(TimestampNanos::new(1625078460_000_000_000))
        );
        assert_eq!(entries[2].command, "cd /tmp");
        assert_eq!(
            entries[2].timestamp,
            Some(TimestampNanos::new(1625078520_000_000_000))
        );
    }

    #[test]
    fn test_merge_sort_and_window_dedup_respects_window() {
        let mut search = FuzzyHistorySearch::new();

        // Build a flat entries table that the formatted entries will index into.
        // Use entries.len() when creating each entry so the stored index always
        // matches the position in the vec.
        let mut entries: Vec<HistoryEntry> = Vec::new();
        let seed_idx = entries.len();
        entries.push(HistoryEntry::new(None, seed_idx, "echo hi".to_string()));

        // Pre-populate cache with a high-score "echo hi".
        search
            .cache
            .push(HistoryEntryFormatted::new(seed_idx, 100, vec![]));

        // Add entries sorted by score after merge. We place another "echo hi" far enough away
        // (more than DUPLICATE_CHECK_WINDOW ranks lower) so it should NOT be removed.
        let mut new_entries = Vec::new();

        // Many unique commands that will sit between the two duplicates.
        for i in 0..(FuzzyHistorySearch::DUPLICATE_CHECK_WINDOW + 5) {
            let idx = entries.len();
            entries.push(HistoryEntry::new(None, idx, format!("cmd_{i}")));
            new_entries.push(HistoryEntryFormatted::new(idx, 99 - (i as i64), vec![]));
        }

        // Lower-score duplicate; should survive because it's outside the window.
        let far_dup_idx = entries.len();
        entries.push(HistoryEntry::new(
            None,
            far_dup_idx,
            "  echo hi  ".to_string(),
        ));
        new_entries.push(HistoryEntryFormatted::new(far_dup_idx, 1, vec![]));

        // Another near-duplicate (close in rank): should be removed.
        let near_dup_idx = entries.len();
        entries.push(HistoryEntry::new(None, near_dup_idx, "echo hi".to_string()));
        new_entries.push(HistoryEntryFormatted::new(near_dup_idx, 98, vec![]));

        new_entries.sort();

        search.merge_sort_and_window_dedup(new_entries, &entries);

        // After sorting/merging we should have exactly 2 "echo hi" entries:
        // - the original high-score one
        // - the far-away low-score one (outside dedup window)
        let echo_hi_count = search
            .cache
            .iter()
            .filter(|e| entries[e.entry_index].command.trim() == "echo hi")
            .count();

        assert_eq!(echo_hi_count, 2);
    }

    #[test]
    fn test_normalize_entries_dedups_adjacent_and_reindexes() {
        let entries = vec![
            HistoryEntry::new(Some(1), 99, "echo hi".to_string()),
            HistoryEntry::new(Some(1), 42, "echo hi".to_string()),
            HistoryEntry::new(Some(3), 7, "pwd".to_string()),
        ];

        let normalized = HistoryManager::normalize_entries(entries);

        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].command, "echo hi");
        assert_eq!(normalized[0].index, 0);
        assert_eq!(normalized[1].command, "pwd");
        assert_eq!(normalized[1].index, 1);
    }

    #[test]
    fn test_merge_history_entries_dedups_adjacent_and_reindexes() {
        let zsh_entries = vec![
            HistoryEntry::new(Some(1), 10, "echo hi".to_string()),
            HistoryEntry::new(Some(3), 11, "pwd".to_string()),
        ];
        let bash_entries = vec![
            HistoryEntry::new(Some(1), 20, "echo hi".to_string()),
            HistoryEntry::new(Some(4), 21, "ls".to_string()),
        ];

        let merged = HistoryManager::merge_history_entries(zsh_entries, bash_entries);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].command, "echo hi");
        assert_eq!(merged[0].index, 0);
        assert_eq!(merged[1].command, "pwd");
        assert_eq!(merged[1].index, 1);
        assert_eq!(merged[2].command, "ls");
        assert_eq!(merged[2].index, 2);
    }

    #[test]
    fn test_last_word_insert_logic() {
        let mut hm = HistoryManager::new_empty();
        hm.push_entry("echo one".to_string());
        hm.push_entry("echo two".to_string());
        hm.push_entry("echo three".to_string());

        // Initially no insert command
        assert_eq!(hm.get_last_word_insert_command(), None);

        // Move prev starts search from the end (index 2)
        assert_eq!(hm.last_word_insert_move_prev(), Some("echo three"));
        assert_eq!(hm.get_last_word_insert_command(), Some("echo three"));

        // Move prev again moves to index 1
        assert_eq!(hm.last_word_insert_move_prev(), Some("echo two"));
        assert_eq!(hm.get_last_word_insert_command(), Some("echo two"));

        // Move prev again moves to index 0
        assert_eq!(hm.last_word_insert_move_prev(), Some("echo one"));
        assert_eq!(hm.get_last_word_insert_command(), Some("echo one"));

        // Move prev again returns None (no more commands)
        assert_eq!(hm.last_word_insert_move_prev(), None);

        // Reset clears it
        hm.last_word_insert_reset();
        assert_eq!(hm.get_last_word_insert_command(), None);
    }

    #[test]
    fn test_get_last_word() {
        assert_eq!(get_last_word("echo hello"), Some("hello".to_string()));
        assert_eq!(
            get_last_word("echo 'hello world'"),
            Some("'hello world'".to_string())
        );
        assert_eq!(
            get_last_word("echo `git status`"),
            Some("`git status`".to_string())
        );
        assert_eq!(
            get_last_word("echo $(git status)"),
            Some("$(git status)".to_string())
        );
        assert_eq!(get_last_word("echo ${VAR}"), Some("${VAR}".to_string()));
        assert_eq!(get_last_word("ls -la;"), Some("-la".to_string()));
        assert_eq!(get_last_word("ls > file"), Some("file".to_string()));
        assert_eq!(get_last_word("ls >"), Some("ls".to_string()));
        assert_eq!(get_last_word("echo hello &&"), Some("hello".to_string()));
        assert_eq!(get_last_word("cmd1 | cmd2"), Some("cmd2".to_string()));
        assert_eq!(get_last_word("cmd1 | "), Some("cmd1".to_string()));
        assert_eq!(get_last_word("   "), None);
        assert_eq!(get_last_word(";"), None);
        assert_eq!(
            get_last_word("hello\"world\""),
            Some("hello\"world\"".to_string())
        );
        assert_eq!(
            get_last_word("cat <<EOF\nhello world\nEOF"),
            Some("<<EOF\nhello world\nEOF".to_string())
        );
        assert_eq!(
            get_last_word("echo <<EOF1 <<EOF2\nbody1\nEOF1\nbody2\nEOF2"),
            Some("<<EOF2\nbody1\nEOF1\nbody2\nEOF2".to_string())
        );
    }

    #[test]
    fn test_last_word_insert_skips_empty() {
        let mut hm = HistoryManager::new_empty();
        hm.push_entry("echo one".to_string());
        hm.push_entry(";".to_string());
        hm.push_entry("echo two".to_string());

        assert_eq!(hm.last_word_insert_move_prev(), Some("echo two"));
        // Moving prev again should skip ";" and go to "echo one"
        assert_eq!(hm.last_word_insert_move_prev(), Some("echo one"));
        assert_eq!(hm.last_word_insert_move_prev(), None);
    }

    #[test]
    fn test_jsonl_history_serialization_and_locking() {
        let session_uuid = uuid::Uuid::now_v7().to_string();
        let cmd_uuid = uuid::Uuid::now_v7().to_string();
        let start_event = HistoryJsonlEvent::Start {
            id: cmd_uuid.clone(),
            timestamp: TimestampNanos::new(1700000000000000000),
            command: "cargo test --lib".to_string(),
            tag: HistoryTag::Normal,
            cwd: Some("/home/user/project".to_string()),
            hostname: Some("test-host".to_string()),
            session: session_uuid.clone(),
        };
        let end_event = HistoryJsonlEvent::End {
            id: cmd_uuid.clone(),
            timestamp: TimestampNanos::new(1700000005000000000),
            duration_ns: Some(5000000000),
            exit_status: Some(0),
            pipestatus: Some("0".to_string()),
        };

        let start_json = serde_json::to_string(&start_event).unwrap();
        let end_json = serde_json::to_string(&end_event).unwrap();

        assert!(start_json.contains("\"event\":\"start\""));
        assert!(start_json.contains("\"session\":\""));
        assert!(start_json.contains("\"command\":\"cargo test --lib\""));
        assert!(end_json.contains("\"event\":\"end\""));
        assert!(end_json.contains("\"duration_ns\":5000000000"));
    }

    #[test]
    fn test_import_bash_history_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_hist_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let hist_file = temp_dir.join("bash_history");
        let target_jsonl = temp_dir.join("history.jsonl");
        std::fs::write(&hist_file, "#1700000000\nls -la\n#1700000010\ncargo test\n").unwrap();

        let count = import_history_file_to(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count, 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_zsh_history_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_zsh_hist_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let hist_file = temp_dir.join("zsh_history");
        let target_jsonl = temp_dir.join("history.jsonl");
        std::fs::write(
            &hist_file,
            ": 1700000000:0;ls -la\n: 1700000010:0;cargo test\n",
        )
        .unwrap();

        let count = import_history_file_to(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count, 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_history_idempotent() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_idempotent_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let hist_file = temp_dir.join("bash_history_no_ts");
        let target_jsonl = temp_dir.join("history.jsonl");
        std::fs::write(&hist_file, "echo hello\necho world\n").unwrap();

        let count1 = import_history_file_to(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count1, 2);

        let count2 = import_history_file_to(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count2, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_atuin_history() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_atuin_hist_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let target_jsonl = temp_dir.join("history.jsonl");

        let res = import_atuin_history_to(&target_jsonl);
        if let Ok(count) = res {
            assert!(target_jsonl.exists());
            println!("Imported {} items from Atuin in test", count);
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_atuin_sqlite_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "flyline_test_atuin_sqlite_{}",
            rand::random::<u64>()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let db_path = temp_dir.join("history.db");
        let target_jsonl = temp_dir.join("history.jsonl");

        let py_setup = format!(
            r#"
import sqlite3
conn = sqlite3.connect("{}")
conn.execute("CREATE TABLE history (id text primary key, timestamp integer not null, duration integer not null, exit integer not null, command text not null, cwd text not null, session text not null, hostname text not null, deleted_at integer)")
conn.execute("INSERT INTO history VALUES ('id1', 1785102235000000000, 1500000000, 0, 'echo sqlite_test', '/home/user', 'session1', 'host1', NULL)")
conn.commit()
"#,
            db_path.to_str().unwrap()
        );

        let py_status = std::process::Command::new("python3")
            .args(["-c", &py_setup])
            .status();

        if let Ok(status) = py_status {
            if status.success() {
                let count = import_atuin_sqlite_file_to(&db_path, &target_jsonl).unwrap();
                assert_eq!(count, 1);
                let content = std::fs::read_to_string(&target_jsonl).unwrap();
                assert!(content.contains("echo sqlite_test"));
                assert!(content.contains("/home/user"));
                assert!(content.contains("host1"));
            }
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_timestamp_nanos_methods() {
        let ts_zero = TimestampNanos::ZERO;
        assert!(ts_zero.is_zero());
        assert_eq!(ts_zero.as_seconds(), 0);
        assert_eq!(ts_zero.fractional_ns(), 0);
        assert_eq!(ts_zero.format_local_datetime(), None);

        let raw = 1_785_451_996_774_964_850u64;
        let ts = TimestampNanos::new(raw);
        assert!(!ts.is_zero());
        assert_eq!(ts.as_seconds(), 1_785_451_996);
        assert_eq!(ts.fractional_ns(), 774_964_850);
        assert_eq!(ts.raw_nanos(), raw);

        let formatted_dt = ts.format_local_datetime();
        assert!(formatted_dt.is_some());
        assert!(formatted_dt.unwrap().contains("2026"));

        let timeago = ts.format_timeago_5chars();
        assert_eq!(timeago.len(), 5);
    }

    #[test]
    fn test_history_manager_tags() {
        let mut normal_hm = HistoryManager::new_empty_with_tag(HistoryTag::Normal);
        normal_hm.push_entry("ls -la".to_string());
        assert_eq!(normal_hm.entries()[0].tag(), HistoryTag::Normal);

        let mut cancelled_hm = HistoryManager::new_empty_with_tag(HistoryTag::Cancelled);
        cancelled_hm.push_entry("git status".to_string());
        assert_eq!(cancelled_hm.entries()[0].tag(), HistoryTag::Cancelled);

        let mut agent_hm = HistoryManager::new_empty_with_tag(HistoryTag::Agent);
        agent_hm.push_entry("explain this code".to_string());
        assert_eq!(agent_hm.entries()[0].tag(), HistoryTag::Agent);
    }

    #[test]
    fn test_multiline_bash_history_parsing() {
        let sample = r#"#1785345081
git pull
#1785345107
git checkout -b quad_click
#1785345375
cat <<EOF1
asdf oiuweoir uwer 
asdf asd fds f
asdfasdfsdf 
EOF1

#1785345413
clear
"#;
        let entries = HistoryManager::parse_bash_history_str(sample);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].command, "git pull");
        assert_eq!(
            entries[0].timestamp.map(|t| t.as_seconds()),
            Some(1785345081)
        );
        assert_eq!(entries[1].command, "git checkout -b quad_click");
        assert_eq!(
            entries[2].command,
            "cat <<EOF1\nasdf oiuweoir uwer \nasdf asd fds f\nasdfasdfsdf \nEOF1"
        );
        assert_eq!(
            entries[2].timestamp.map(|t| t.as_seconds()),
            Some(1785345375)
        );
        assert_eq!(entries[3].command, "clear");
    }
}
