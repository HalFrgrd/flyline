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

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub timestamp: Option<u64>,
    pub index: usize,
    pub command: String,
    pub raw_output: Option<String>,
    syntax_highlighted: OnceCell<Vec<Line<'static>>>,
}

impl HistoryEntry {
    pub(crate) fn new(timestamp: Option<u64>, index: usize, command: String) -> Self {
        HistoryEntry {
            timestamp,
            index,
            command,
            raw_output: None,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HistoryJsonlEvent {
    Start {
        id: String,
        timestamp: u64,
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hostname: Option<String>,
        session: String,
    },
    End {
        id: String,
        timestamp: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
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
    let _ = std::fs::create_dir_all(&base);
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

    let file = OpenOptions::new().create(true).append(true).open(path)?;

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

fn fetch_flyline_jsonl_history_from_offset(
    start_offset: u64,
) -> anyhow::Result<(Vec<HistoryEntry>, u64)> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Seek, SeekFrom};
    use std::os::unix::io::AsRawFd;

    let path = flyline_history_jsonl_path();
    if !path.exists() {
        return Ok((Vec::new(), 0));
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
                if let HistoryJsonlEvent::Start {
                    timestamp, command, ..
                } = event
                {
                    let ts_secs = if timestamp > 1_000_000_000_000 {
                        timestamp / 1_000_000_000
                    } else {
                        timestamp
                    };
                    entries.push(HistoryEntry::new(Some(ts_secs), line_idx, command));
                    line_idx += 1;
                }
            }
        }
        line_buf.clear();
    }

    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }

    Ok((entries, current_offset))
}

fn repopulate_flyline_jsonl_from_entries(
    entries: &[HistoryEntry],
    session_id: &str,
) -> anyhow::Result<u64> {
    let history_path = flyline_history_jsonl_path();
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

    for entry in entries {
        if entry.command.trim().is_empty() {
            continue;
        }
        let timestamp = entry.timestamp.map(|s| s * 1_000_000_000).unwrap_or(0);
        let cmd_id = atuin_common::utils::uuid_v7().to_string();
        let event = HistoryJsonlEvent::Start {
            id: cmd_id,
            timestamp,
            command: entry.command.clone(),
            cwd: cwd.clone(),
            hostname: hostname.clone(),
            session: session_id.to_string(),
        };
        append_jsonl_history_event_to_path(&event, &history_path)?;
    }

    let file_len = std::fs::metadata(&history_path)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(file_len)
}

pub fn import_history_file(path: &std::path::Path) -> anyhow::Result<usize> {
    import_history_file_to(path, &flyline_history_jsonl_path())
}

pub fn import_history_file_to(
    path: &std::path::Path,
    target_jsonl_path: &std::path::Path,
) -> anyhow::Result<usize> {
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
                            seen_set.insert((timestamp, command));
                        }
                    }
                }
            }
        }
    }

    let session = atuin_common::utils::uuid_v7().to_string();
    let mut imported_count = 0;

    for entry in entries {
        if entry.command.trim().is_empty() {
            continue;
        }
        let timestamp = entry.timestamp.map(|s| s * 1_000_000_000).unwrap_or(0);

        if seen_set.contains(&(timestamp, entry.command.clone())) {
            continue;
        }

        seen_set.insert((timestamp, entry.command.clone()));
        let cmd_id = atuin_common::utils::uuid_v7().to_string();
        let event = HistoryJsonlEvent::Start {
            id: cmd_id,
            timestamp,
            command: entry.command,
            cwd: None,
            hostname: None,
            session: session.clone(),
        };
        append_jsonl_history_event_to_path(&event, target_jsonl_path)?;
        imported_count += 1;
    }

    Ok(imported_count)
}

pub fn import_bash_history_file(path: &std::path::Path) -> anyhow::Result<usize> {
    import_history_file(path)
}

fn fetch_atuin_history() -> anyhow::Result<Vec<HistoryEntry>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        use atuin_client::database::Database;
        use atuin_client::database::Sqlite;
        use atuin_client::settings::Settings as AtuinSettings;

        let atuin_settings = AtuinSettings::new().map_err(|e| anyhow::anyhow!("{e}"))?;
        let db = Sqlite::new(&atuin_settings.db_path, 5.0).await?;

        let ctx = atuin_client::database::Context {
            session: String::new(),
            cwd: String::new(),
            git_root: None,
            hostname: String::new(),
            host_id: String::new(),
        };

        let mut histories = db.list(&[], &ctx, None, false, false, None).await?;
        // TODO: Sorting history entries by timestamp might not be optimal.
        histories.sort_by_key(|h| h.timestamp);

        let mut entries = Vec::with_capacity(histories.len());

        for (idx, h) in histories.into_iter().enumerate() {
            let ts_secs = h.timestamp.unix_timestamp();
            let timestamp = if ts_secs > 0 {
                Some(ts_secs as u64)
            } else {
                None
            };
            entries.push(HistoryEntry::new(timestamp, idx, h.command));
        }

        Ok(entries)
    })
}

fn save_atuin_history(command: &str) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let cmd = command.to_string();
    rt.block_on(async {
        use atuin_client::database::Database;
        use atuin_client::database::Sqlite;
        use atuin_client::history::History as AtuinHistory;
        use atuin_client::settings::Settings as AtuinSettings;

        let atuin_settings = AtuinSettings::new().map_err(|e| anyhow::anyhow!("{e}"))?;
        let db = Sqlite::new(&atuin_settings.db_path, 5.0).await?;

        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let history: AtuinHistory = AtuinHistory::import()
            .timestamp(std::time::SystemTime::now().into())
            .command(cmd)
            .cwd(cwd)
            .build()
            .into();

        db.save(&history).await?;
        Ok(())
    })
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
        if entries
            .last()
            .is_some_and(|prev| prev.command == entry.command)
        {
            return;
        }

        entry.index = entries.len();
        entries.push(entry);
    }

    fn normalize_entries(entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
        let mut normalized = Vec::with_capacity(entries.len());
        for entry in entries {
            Self::push_deduped_entry(&mut normalized, entry);
        }
        normalized
    }

    fn merge_history_entries(
        zsh_entries: Vec<HistoryEntry>,
        bash_entries: Vec<HistoryEntry>,
    ) -> Vec<HistoryEntry> {
        let mut merged = Vec::with_capacity(zsh_entries.len() + bash_entries.len());
        let mut zsh_iter = zsh_entries.into_iter().peekable();
        let mut bash_iter = bash_entries.into_iter().peekable();

        while let (Some(zsh_entry), Some(bash_entry)) = (zsh_iter.peek(), bash_iter.peek()) {
            let take_zsh = zsh_entry.timestamp.unwrap_or(0) <= bash_entry.timestamp.unwrap_or(0);
            if take_zsh {
                Self::push_deduped_entry(&mut merged, zsh_iter.next().unwrap());
            } else {
                Self::push_deduped_entry(&mut merged, bash_iter.next().unwrap());
            }
        }

        for entry in zsh_iter {
            Self::push_deduped_entry(&mut merged, entry);
        }

        for entry in bash_iter {
            Self::push_deduped_entry(&mut merged, entry);
        }

        merged
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
        let history_backend = settings.history_backend;
        let mut last_loaded_external_count = 0;
        let mut last_read_jsonl_byte_offset = 0;

        let entries = if history_backend == HistoryBackend::Flyline {
            match fetch_flyline_jsonl_history_from_offset(0) {
                Ok((jsonl_entries, offset)) if !jsonl_entries.is_empty() => {
                    last_loaded_external_count = jsonl_entries.len();
                    last_read_jsonl_byte_offset = offset;
                    Self::log_recent_entries(&jsonl_entries, "flyline_jsonl");
                    Self::normalize_entries(jsonl_entries)
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
        } else if history_backend == HistoryBackend::Atuin {
            match fetch_atuin_history() {
                Ok(atuin_entries) => {
                    last_loaded_external_count = atuin_entries.len();
                    Self::log_recent_entries(&atuin_entries, "atuin");
                    Self::normalize_entries(atuin_entries)
                }
                Err(e) => {
                    log::warn!(
                        "Failed to load Atuin history DB: {}; falling back to Bash memory history",
                        e
                    );
                    let bash_entries = Self::parse_bash_history_from_memory();
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
        }
    }

    /// Create an empty `HistoryManager` that starts with no entries.
    /// New entries are added at runtime via `push_entry`.
    pub fn new_empty() -> HistoryManager {
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
            session_id: atuin_common::utils::uuid_v7().to_string(),
        }
    }

    /// Refreshes history entries incrementally from the active backend.
    ///
    /// When using `HistoryBackend::Flyline`, queries ~/.local/share/flyline/history.jsonl.
    /// When using `HistoryBackend::Atuin`, queries the Atuin database for new entries.
    /// When using `HistoryBackend::Bash`, re-checks Bash memory history.
    pub fn refresh_history_backend(&mut self) {
        if self.history_backend == HistoryBackend::Flyline {
            if let Ok((jsonl_entries, new_offset)) =
                fetch_flyline_jsonl_history_from_offset(self.last_read_jsonl_byte_offset)
            {
                if !jsonl_entries.is_empty() {
                    log::debug!(
                        "Refreshed Flyline JSONL history: loaded {} new entries from byte offset {}",
                        jsonl_entries.len(),
                        self.last_read_jsonl_byte_offset
                    );
                    for entry in jsonl_entries {
                        Self::push_deduped_entry(&mut self.entries, entry);
                    }
                    self.last_read_jsonl_byte_offset = new_offset;
                    self.last_loaded_external_count = self.entries.len();
                    self.index = self.entries.len();
                    self.fuzzy_search.clear_cache();
                } else if new_offset == 0 {
                    let bash_entries = Self::parse_bash_history_from_memory();
                    if !bash_entries.is_empty() {
                        if let Ok(offset) =
                            repopulate_flyline_jsonl_from_entries(&bash_entries, &self.session_id)
                        {
                            self.last_read_jsonl_byte_offset = offset;
                        }
                    }
                }
            }
        } else if self.history_backend == HistoryBackend::Atuin {
            match fetch_atuin_history() {
                Ok(atuin_entries) => {
                    if atuin_entries.len() > self.last_loaded_external_count {
                        log::debug!(
                            "Refreshed Atuin history: loaded {} new entries",
                            atuin_entries.len() - self.last_loaded_external_count
                        );
                        for entry in atuin_entries
                            .into_iter()
                            .skip(self.last_loaded_external_count)
                        {
                            Self::push_deduped_entry(&mut self.entries, entry);
                        }
                        self.last_loaded_external_count = self.entries.len();
                        self.index = self.entries.len();
                        self.fuzzy_search.clear_cache();
                    }
                }
                Err(e) => {
                    log::warn!("Failed to refresh Atuin history DB: {}", e);
                }
            }
        } else {
            let memory_entries = Self::parse_bash_history_from_memory();
            if memory_entries.len() > self.entries.len() {
                for entry in memory_entries.into_iter().skip(self.entries.len()) {
                    Self::push_deduped_entry(&mut self.entries, entry);
                }
                self.index = self.entries.len();
                self.fuzzy_search.clear_cache();
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Push a new entry to the history list.
    /// `self.index` is kept at `entries.len()` (past-the-end), matching the
    /// invariant established by `new()` and `HistoryManager::search_in_history`.
    /// Resets the fuzzy search cache so the new entry is visible immediately.
    pub fn push_entry(&mut self, command: String) -> String {
        let command_id = atuin_common::utils::uuid_v7().to_string();
        if command.trim().is_empty() {
            return command_id;
        }

        if self.history_backend == HistoryBackend::Flyline {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
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
            let event = HistoryJsonlEvent::Start {
                id: command_id.clone(),
                timestamp,
                command: command.clone(),
                cwd,
                hostname,
                session: self.session_id.clone(),
            };
            if let Err(e) = append_jsonl_history_event(&event) {
                log::warn!("Failed to write start event to JSONL history: {}", e);
            }
        } else if self.history_backend == HistoryBackend::Atuin {
            if let Err(e) = save_atuin_history(&command) {
                log::warn!("Failed to save command to Atuin DB: {}", e);
            }
        }

        let index = self.entries.len();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
        self.entries
            .push(HistoryEntry::new(timestamp, index, command));
        self.index = self.entries.len();
        self.last_word_insert_index = None;
        self.fuzzy_search.clear_cache();

        command_id
    }

    pub fn set_last_raw_output(&mut self, raw_output: String) {
        if let Some(last) = self.entries.last_mut() {
            last.raw_output = Some(raw_output);
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

        s.lines().fold(None, |my_ts, l| {
            let l_ts = HistoryManager::parse_timestamp(l);

            if l_ts.is_some() {
                // replace current timestamp
                l_ts
            } else if l.trim().is_empty() {
                // Empty line
                my_ts
            } else {
                // It's a command line
                let entry = HistoryEntry::new(my_ts, res.len(), l.to_string());
                res.push(entry);
                None
            }
            // TODO multiline commands
        });

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
        other.score.cmp(&self.score)
    }
}
impl std::cmp::PartialOrd for HistoryEntryFormatted {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(other.score.cmp(&self.score))
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
        assert_eq!(entries.len(), 6);

        let mut entries_iter = entries.iter();

        let mut check = |expected_ts: Option<u64>, expected_index: usize, expected_cmd: &str| {
            let entry = entries_iter.next().unwrap();
            assert_eq!(entry.timestamp, expected_ts);
            assert_eq!(entry.index, expected_index);
            assert_eq!(entry.command, expected_cmd);
        };

        check(Some(1625078400), 0, "ls -al");
        check(Some(1625078460), 1, "echo 'Hello, World!'");
        check(None, 2, "pwd");
        check(None, 3, "#cd /asdf/asdf");
        check(None, 4, "cd /home/user");
        check(Some(1625078460), 5, "cd /home/user2");
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
        assert_eq!(entries[0].timestamp, Some(1625078400));
        assert_eq!(entries[1].command, "echo 'Hello, World!'");
        assert_eq!(entries[1].timestamp, Some(1625078460));
        assert_eq!(entries[2].command, "cd /tmp");
        assert_eq!(entries[2].timestamp, Some(1625078520));
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
            HistoryEntry::new(Some(2), 42, "echo hi".to_string()),
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
            HistoryEntry::new(Some(2), 20, "echo hi".to_string()),
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
        let session_uuid = atuin_common::utils::uuid_v7().to_string();
        let cmd_uuid = atuin_common::utils::uuid_v7().to_string();
        let start_event = HistoryJsonlEvent::Start {
            id: cmd_uuid.clone(),
            timestamp: 1700000000000000000,
            command: "cargo test --lib".to_string(),
            cwd: Some("/home/user/project".to_string()),
            hostname: Some("test-host".to_string()),
            session: session_uuid.clone(),
        };
        let end_event = HistoryJsonlEvent::End {
            id: cmd_uuid.clone(),
            timestamp: 1700000005000000000,
            duration_ms: Some(5000),
            exit_status: Some(0),
            pipestatus: Some("0".to_string()),
        };

        let start_json = serde_json::to_string(&start_event).unwrap();
        let end_json = serde_json::to_string(&end_event).unwrap();

        assert!(start_json.contains("\"event\":\"start\""));
        assert!(start_json.contains("\"session\":\""));
        assert!(start_json.contains("\"command\":\"cargo test --lib\""));
        assert!(end_json.contains("\"event\":\"end\""));
        assert!(end_json.contains("\"duration_ms\":5000"));
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
}
