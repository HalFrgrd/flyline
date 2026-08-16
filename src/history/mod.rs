use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;
use std::vec;

use crate::content::{self, StatefulSlidingWindow, apply_match_indices_to_lines};
use crate::palette::Palette;
use crate::shell;

#[cfg(not(test))]
use crate::shell::bash::symbols as bash_symbols;
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
        TimestampNanos(raw)
    }

    #[allow(dead_code)]
    pub fn from_nanos(nanos: u64) -> Self {
        TimestampNanos(nanos)
    }

    pub fn from_seconds(secs: u64) -> Self {
        TimestampNanos(secs.saturating_mul(1_000_000_000))
    }

    pub fn now() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        TimestampNanos(nanos)
    }

    pub fn duration_formatted(dur_ns: u64) -> String {
        if dur_ns >= 1_000_000_000 {
            format!("{:.2}s", dur_ns as f64 / 1_000_000_000.0)
        } else if dur_ns >= 1_000_000 {
            format!("{}ms", dur_ns / 1_000_000)
        } else if dur_ns >= 1_000 {
            format!("{}µs", dur_ns / 1_000)
        } else {
            format!("{}ns", dur_ns)
        }
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
        crate::content::ts_to_timeago_string_5chars(self.as_seconds())
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryMetadata {
    pub id: Option<String>,
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
    // Stored out of line for efficiency
    pub metadata: Option<Box<HistoryMetadata>>,
    syntax_highlighted: OnceLock<Vec<Line<'static>>>,
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

    pub fn apply_end_metadata(
        &mut self,
        duration_ns: Option<u64>,
        exit_status: Option<i32>,
        pipestatus: Option<&str>,
    ) {
        let meta = self.metadata_mut();
        meta.duration_ns = duration_ns;
        meta.exit_status = exit_status;
        meta.pipestatus = pipestatus.map(String::from);
    }

    pub fn to_jsonl_start_event(
        &self,
        default_session_id: &str,
        default_hostname: Option<&str>,
    ) -> HistoryJsonlEvent {
        let cmd_id = self
            .id()
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let timestamp = self.timestamp.unwrap_or(TimestampNanos::ZERO);
        let cwd = self.cwd().map(String::from);
        let hostname = self
            .hostname()
            .map(String::from)
            .or_else(|| default_hostname.map(String::from));
        let session = self
            .session()
            .map(String::from)
            .unwrap_or_else(|| default_session_id.to_string());

        HistoryJsonlEvent::Start {
            id: cmd_id,
            timestamp,
            command: self.command.clone(),
            cwd,
            hostname,
            session,
        }
    }

    pub fn to_jsonl_end_event(&self) -> Option<HistoryJsonlEvent> {
        if self.exit_status().is_none() && self.pipestatus().is_none() {
            return None;
        }
        let cmd_id = self
            .id()
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let timestamp = self.timestamp.unwrap_or(TimestampNanos::ZERO);

        Some(HistoryJsonlEvent::End {
            id: cmd_id,
            timestamp,
            exit_status: self.exit_status(),
            pipestatus: self.pipestatus().map(String::from),
        })
    }

    pub(crate) fn new(timestamp: Option<u64>, index: usize, command: String) -> Self {
        let timestamp = timestamp.map(TimestampNanos::new);
        HistoryEntry {
            timestamp,
            index,
            command,
            metadata: None,
            syntax_highlighted: OnceLock::new(),
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

    pub fn format_extra_info(&self) -> String {
        let mut lines = Vec::new();

        if let Some(cwd) = self.cwd() {
            lines.push(format!("Directory: {}", cwd));
        }
        if let Some(host) = self.hostname() {
            lines.push(format!("Host: {}", host));
        }
        if let Some(ts) = self.timestamp {
            if let Some(time_str) = ts.format_local_datetime() {
                let time_ago = ts.format_timeago_5chars();
                lines.push(format!("Time: {} ({} ago)", time_str, time_ago.trim()));
            } else {
                lines.push("Time: N/A".to_string());
            }
        } else {
            lines.push("Time: N/A".to_string());
        }
        if let Some(dur_ns) = self.duration_ns() {
            lines.push(format!(
                "Duration: {}",
                TimestampNanos::duration_formatted(dur_ns)
            ));
        } else {
            lines.push("Duration: N/A".to_string());
        }
        if let Some(exit) = self.exit_status() {
            lines.push(format!("Exit Code: {}", exit));
        } else {
            lines.push("Exit Code: N/A".to_string());
        }
        if let Some(pipe) = self.pipestatus().filter(|s| !s.trim().is_empty()) {
            lines.push(format!("Pipeline Status: {}", pipe));
        } else {
            lines.push("Pipeline Status: N/A".to_string());
        }
        if let Some(session) = self.session() {
            lines.push(format!("Session: {}", session));
        }
        if let Some(id) = self.id() {
            lines.push(format!("ID: {}", id));
        }

        lines.join("\n")
    }
}

impl TryFrom<HistoryJsonlEvent> for HistoryEntry {
    type Error = ();

    fn try_from(event: HistoryJsonlEvent) -> Result<Self, Self::Error> {
        match event {
            HistoryJsonlEvent::Start {
                id,
                timestamp,
                command,
                cwd,
                hostname,
                session,
            } => {
                let mut entry = HistoryEntry::new(Some(timestamp.raw_nanos()), 0, command);
                let meta = entry.metadata_mut();
                meta.id = Some(id);
                meta.cwd = cwd;
                meta.hostname = hostname;
                meta.session = Some(session);
                Ok(entry)
            }
            HistoryJsonlEvent::End { .. } => Err(()),
        }
    }
}

pub mod backend;
pub mod importing;

pub use backend::{HistoryJsonlEvent, LastJsonlReadOffset, default_jsonl_path};
use backend::{
    append_jsonl_history_event, fetch_flyline_jsonl_history_from_offset, is_file_empty_or_missing,
    repopulate_jsonl_from_entries,
};
#[allow(unused_imports)]
pub use importing::{import_atuin_history, import_history_file};

#[derive(Debug)]
pub struct HistoryManager {
    entries: Vec<HistoryEntry>,
    index: usize,
    last_search_prefix: Option<String>,
    last_buffered_command: Option<String>,
    fuzzy_search: FuzzyHistorySearch,
    last_word_insert_index: Option<usize>,
    pub last_jsonl_read_offset: Option<LastJsonlReadOffset>,
    session_id: String,
    jsonl_history_path: PathBuf,
    last_submitted_command: Option<(String, std::time::Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistorySearchDirection {
    Backward,
    Forward,
    PageBackward,
    PageForward,
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl HistoryManager {
    pub fn new_empty() -> HistoryManager {
        Self::new_empty_with_path(None)
    }

    pub fn new_empty_with_path(jsonl_history_path: Option<PathBuf>) -> HistoryManager {
        let jsonl_history_path = jsonl_history_path.unwrap_or_else(default_jsonl_path);
        HistoryManager {
            entries: Vec::new(),
            index: 0,
            last_search_prefix: None,
            last_buffered_command: None,
            fuzzy_search: FuzzyHistorySearch::new(),
            last_word_insert_index: None,
            last_jsonl_read_offset: None,
            session_id: uuid::Uuid::now_v7().to_string(),
            jsonl_history_path,
            last_submitted_command: None,
        }
    }
    pub fn jsonl_path(&self) -> PathBuf {
        self.jsonl_history_path.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn set_jsonl_history_path(&mut self, path: PathBuf) {
        if self.jsonl_history_path != path {
            self.jsonl_history_path = path;
            self.last_jsonl_read_offset = None;
            self.refresh_jsonl_backend();
        }
    }

    pub fn set_last_submitted_command(&mut self, cmd_id: String, start_time: std::time::Instant) {
        self.last_submitted_command = Some((cmd_id, start_time));
    }

    /// Reset history navigation cursor to past-the-end for a fresh prompt session.
    pub fn reset_navigation(&mut self) {
        self.index = self.entries.len();
        self.last_search_prefix = None;
        self.last_buffered_command = None;
        self.last_word_insert_index = None;
    }

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

    #[cfg(test)]
    pub fn parse_bash_history_from_memory() -> Vec<HistoryEntry> {
        Vec::new()
    }

    #[cfg(not(test))]
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
                            HistoryManager::parse_timestamp(timestamp_str)
                                .map(|s| TimestampNanos::from_seconds(s).raw_nanos())
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

    pub fn reload_from_bash_history(&mut self, zsh_history_path: Option<&str>) {
        self.entries.clear();
        self.last_search_prefix = None;
        self.last_buffered_command = None;
        self.last_word_insert_index = None;
        let bash_entries = shell::backend().parse_history_from_memory();
        Self::log_recent_entries(&bash_entries, "bash");
        let entries = if let Some(zsh_path) = zsh_history_path {
            let zsh_entries = Self::parse_zsh_history(Some(zsh_path));
            Self::log_recent_entries(&zsh_entries, "Zsh");
            Self::merge_history_entries(zsh_entries, bash_entries)
        } else {
            bash_entries
        };
        self.entries = Self::normalize_entries(entries);
        self.index = self.entries.len();
        self.fuzzy_search.clear_cache();
    }

    pub fn merge_jsonl_events(&mut self, mut events: Vec<HistoryJsonlEvent>) {
        if events.is_empty() {
            return;
        }
        events.sort_by_key(|e| e.timestamp().raw_nanos());

        let mut entries_changed = false;
        for event in events {
            if self.merge_jsonl_event(event) {
                entries_changed = true;
            }
        }

        if entries_changed {
            self.entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
            for (i, entry) in self.entries.iter_mut().enumerate() {
                entry.index = i;
            }
            self.fuzzy_search.clear_cache();
        }
        self.index = self.entries.len();
    }

    pub fn merge_jsonl_event(&mut self, event: HistoryJsonlEvent) -> bool {
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
                    return false;
                }
                let ts_raw = timestamp.raw_nanos();
                for entry in self.entries.iter_mut().rev() {
                    if entry.id() == Some(&id)
                        || (entry.timestamp.map(|t| t.raw_nanos()) == Some(ts_raw)
                            && entry.command == command)
                    {
                        let meta = entry.metadata_mut();
                        if meta.id.is_none() {
                            meta.id = Some(id);
                        }
                        if meta.cwd.is_none() {
                            meta.cwd = cwd;
                        }
                        if meta.hostname.is_none() {
                            meta.hostname = hostname;
                        }
                        if meta.session.is_none() {
                            meta.session = Some(session);
                        }
                        return true;
                    }
                }

                let mut entry = HistoryEntry::new(Some(timestamp.raw_nanos()), 0, command);
                let meta = entry.metadata_mut();
                meta.id = Some(id);
                meta.cwd = cwd;
                meta.hostname = hostname;
                meta.session = Some(session);

                Self::push_deduped_entry(&mut self.entries, entry);
                true
            }
            HistoryJsonlEvent::End {
                id,
                timestamp,
                exit_status,
                pipestatus,
            } => {
                if let Some(entry) = self.entries.iter_mut().rfind(|e| e.id() == Some(&id)) {
                    let duration_ns = entry
                        .timestamp
                        .map(|start_ts| timestamp.raw_nanos().saturating_sub(start_ts.raw_nanos()));
                    entry.apply_end_metadata(duration_ns, exit_status, pipestatus.as_deref());
                    return true;
                }
                false
            }
        }
    }

    fn ensure_jsonl_repopulated_if_needed(&mut self) -> PathBuf {
        let path = self.jsonl_path();
        if is_file_empty_or_missing(&path) {
            if self.entries.is_empty() {
                let bash_entries = shell::backend().parse_history_from_memory();
                self.entries = Self::normalize_entries(bash_entries);
            }
            if !self.entries.is_empty() {
                let _ = repopulate_jsonl_from_entries(&self.entries, &self.session_id, &path);
            }
            self.last_jsonl_read_offset = None;
        }
        path
    }

    /// Refreshes history entries incrementally from the active backend.
    ///
    /// When using `HistoryBackend::Flyline`, queries ~/.local/share/flyline/history.jsonl.
    pub fn refresh_jsonl_backend(&mut self) {
        let path = self.ensure_jsonl_repopulated_if_needed();
        let prev_offset = self.last_jsonl_read_offset.clone();

        match fetch_flyline_jsonl_history_from_offset(&path, self.last_jsonl_read_offset.as_ref()) {
            Ok(fetch_res) => {
                self.merge_jsonl_events(fetch_res.events);
                self.last_jsonl_read_offset = fetch_res.last_read_offset;
                self.index = self.entries.len();
            }
            Err(e) => {
                log::warn!(
                    "Failed to fetch JSONL history from {:?}: {}",
                    prev_offset,
                    e
                );
            }
        }
    }

    #[allow(dead_code)]
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Push a new entry to the in-memory history list.
    /// `self.index` is kept at `entries.len()` (past-the-end), matching the
    /// invariant established by `new()` and `HistoryManager::search_in_history`.
    /// Resets the fuzzy search cache so the new entry is visible immediately.
    pub fn push_entry(&mut self, command: String) -> String {
        let command_id = uuid::Uuid::now_v7().to_string();
        if command.trim().is_empty() {
            return command_id;
        }

        let shell_cwd = shell::backend().cwd();
        let cwd = if !shell_cwd.is_empty() {
            Some(shell_cwd)
        } else {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        };
        let hostname = Some(shell::backend().hostname()).filter(|h| !h.is_empty());

        let now_ts = TimestampNanos::now();
        let index = self.entries.len();
        let mut entry = HistoryEntry::new(Some(now_ts.raw_nanos()), index, command);
        let meta = entry.metadata_mut();
        meta.id = Some(command_id.clone());
        meta.cwd = cwd;
        meta.hostname = hostname;
        meta.session = Some(self.session_id.clone());
        self.entries.push(entry);
        self.index = self.entries.len();
        self.last_word_insert_index = None;
        self.fuzzy_search.clear_cache();

        command_id
    }

    /// Push a new entry to the in-memory history list AND write the Start event to JSONL history.
    pub fn push_entry_and_jsonl_append(&mut self, command: String) -> String {
        if command.trim().is_empty() {
            return uuid::Uuid::now_v7().to_string();
        }

        let path = self.ensure_jsonl_repopulated_if_needed();
        let command_id = self.push_entry(command.clone());

        if let Some(entry) = self.entries.last() {
            let event = HistoryJsonlEvent::Start {
                id: command_id.clone(),
                timestamp: entry.timestamp.unwrap_or_else(TimestampNanos::now),
                command,
                cwd: entry.cwd().map(String::from),
                hostname: entry.hostname().map(String::from),
                session: self.session_id.clone(),
            };
            if let Err(e) = append_jsonl_history_event(&event, &path) {
                log::warn!("Failed to write start event to JSONL history: {}", e);
            }
        }

        command_id
    }

    pub fn record_last_command_end(&mut self, exit_status: i32, pipestatus: Option<String>) {
        if let Some((cmd_id, _start_time)) = self.last_submitted_command.take() {
            let path = self.ensure_jsonl_repopulated_if_needed();
            let end_ts = TimestampNanos::now();
            let event = HistoryJsonlEvent::End {
                id: cmd_id,
                timestamp: end_ts,
                exit_status: Some(exit_status),
                pipestatus,
            };
            // We dont update last_jsonl_read_offset here so that when we open
            // the history search, we read new events from other sessions
            if let Err(e) = append_jsonl_history_event(&event, &path) {
                log::warn!("Failed to write end event to JSONL history: {}", e);
            }
            self.merge_jsonl_event(event);
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
            if let Some(entry) = self.entries.get(start_idx)
                && get_last_word(&entry.command).is_some()
            {
                self.last_word_insert_index = Some(start_idx);
                return Some(entry.command.as_str());
            }
        }
        None
    }

    pub fn last_word_insert_reset(&mut self) {
        self.last_word_insert_index = None;
    }

    fn parse_timestamp(line: &str) -> Option<u64> {
        // Bash writes `#<timestamp>` directly (digits immediately after `#` with no whitespace).
        let rest = line.strip_prefix('#')?;
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            let val = rest.parse::<u64>().ok()?;
            // Minimum bound: 100_000_000 (year 1973) to filter out small numbers / issue numbers like #123
            if (100_000_000..=10_000_000_000).contains(&val) {
                return Some(val);
            }
        }
        None
    }

    pub fn parse_bash_history_str(s: &str) -> Vec<HistoryEntry> {
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
                current_ts = Some(TimestampNanos::from_seconds(ts).raw_nanos());
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

    pub fn parse_zsh_history_str(s: &str) -> Vec<HistoryEntry> {
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
                            .and_then(|ts| ts.parse::<u64>().ok())
                            .map(|s| TimestampNanos::from_seconds(s).raw_nanos());
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

    pub fn fuzzy_search_entry_by_idx(&self, idx: usize) -> Option<&HistoryEntry> {
        self.fuzzy_search
            .cache
            .get(idx)
            .and_then(|formatted| self.entries.get(formatted.entry_index))
    }

    // fuzzy search cache logic moved to FuzzyHistorySearch
}

#[derive(Debug)]
pub(crate) struct HistoryEntryFormatted {
    pub entry_index: usize,
    pub score: i64,
    pub match_indices: Vec<usize>,
    command_spans: OnceLock<Vec<Line<'static>>>,
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
            command_spans: OnceLock::new(),
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

            if let Some((score, indices)) = content::fuzzy_indices_with_threshold(
                &self.matcher,
                &entry.command,
                current_cmd,
                content::FuzzyMatchThreshold::Medium,
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
        if let Some(closing) = &tokens[curr_idx].annotations.closing
            && closing.opening_idx < curr_idx
        {
            curr_idx = closing.opening_idx;
            jumped = true;
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
        assert_eq!(
            HistoryManager::parse_timestamp("#1625078400"),
            Some(1625078400)
        );
        assert_eq!(
            HistoryManager::parse_timestamp("#1785345081"),
            Some(1785345081)
        );
        assert_eq!(HistoryManager::parse_timestamp("# 1625078400"), None);
        assert_eq!(HistoryManager::parse_timestamp("  #1625078400"), None);
        assert_eq!(HistoryManager::parse_timestamp("1625078400"), None);
        assert_eq!(HistoryManager::parse_timestamp("#12345"), None);
        assert_eq!(HistoryManager::parse_timestamp("#1"), None);
        assert_eq!(HistoryManager::parse_timestamp("#not_a_number"), None);
        assert_eq!(HistoryManager::parse_timestamp("#cd /asdf/asdf"), None);
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

        const NS: u64 = 1_000_000_000;
        check(Some(1_625_078_400 * NS), 0, "ls -al");
        check(
            Some(1_625_078_460 * NS),
            1,
            "echo 'Hello, World!'\npwd\n#cd /asdf/asdf\ncd /home/user",
        );
        check(Some(1_625_078_460 * NS), 2, "cd /home/user2");
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
            Some(TimestampNanos::new(1_625_078_400 * 1_000_000_000))
        );
        assert_eq!(entries[1].command, "echo 'Hello, World!'");
        assert_eq!(
            entries[1].timestamp,
            Some(TimestampNanos::new(1_625_078_460 * 1_000_000_000))
        );
        assert_eq!(entries[2].command, "cd /tmp");
        assert_eq!(
            entries[2].timestamp,
            Some(TimestampNanos::new(1_625_078_520 * 1_000_000_000))
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
        let mut hm = HistoryManager::default();
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
        let mut hm = HistoryManager::default();
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
    fn test_custom_history_file_path() {
        let custom_path = std::path::PathBuf::from("/tmp/custom_flyline_history.jsonl");
        let hm = HistoryManager::new_empty_with_path(Some(custom_path.clone()));
        assert_eq!(hm.jsonl_path(), custom_path);
    }

    #[test]
    fn test_change_jsonl_history_path_mid_session() {
        let file1 =
            std::env::temp_dir().join(format!("flyline_test_mid1_{}.jsonl", uuid::Uuid::now_v7()));
        let file2 =
            std::env::temp_dir().join(format!("flyline_test_mid2_{}.jsonl", uuid::Uuid::now_v7()));
        let _ = std::fs::remove_file(&file1);
        let _ = std::fs::remove_file(&file2);

        let mut hm = HistoryManager::new_empty_with_path(Some(file1.clone()));
        hm.push_entry_and_jsonl_append("command_one".to_string());
        hm.push_entry_and_jsonl_append("command_two".to_string());

        assert_eq!(hm.entries().len(), 2);
        assert!(file1.exists());
        assert!(!file2.exists());

        // Change jsonl history path mid-session to new missing file
        hm.set_jsonl_history_path(file2.clone());

        // new file file2 should now exist and contain the in-memory entries!
        assert!(file2.exists());
        let res = fetch_flyline_jsonl_history_from_offset(&file2, None).unwrap();
        assert_eq!(res.events.len(), 2);

        let _ = std::fs::remove_file(&file1);
        let _ = std::fs::remove_file(&file2);
    }

    #[test]
    fn test_timestamp_nanos_max_value() {
        let ts_max = TimestampNanos::new(u64::MAX);
        assert_eq!(ts_max.raw_nanos(), u64::MAX);
        assert_eq!(ts_max.as_seconds(), u64::MAX / 1_000_000_000);
        // Ensure timeago underflow safety when timestamp is far in the future/max
        let timeago = ts_max.format_timeago_5chars();
        assert_eq!(timeago, " now ");
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

    #[test]
    fn test_jsonl_timestamp_42_billion_not_overflowing_to_future() {
        let json_line = r#"{"type":"start","id":"019ffc2c-74e4-75c0-b0ef-894f3cc3d411","ts":42000000000,"cmd":"oldcommandhere","host":"itx-desktop","sesh":"019ffc2c-74b4-7191-821d-23093c6c1020"}"#;
        let event: HistoryJsonlEvent = serde_json::from_str(json_line).unwrap();
        let entry = HistoryEntry::try_from(event).unwrap();
        assert_eq!(entry.timestamp.unwrap().raw_nanos(), 42_000_000_000);
        assert_eq!(entry.timestamp.unwrap().as_seconds(), 42);
        let dt = entry.timestamp.unwrap().format_local_datetime().unwrap();
        assert!(dt.starts_with("1970"));
    }

    #[test]
    fn test_refactored_jsonl_merging_and_repopulation() {
        let temp_file = std::env::temp_dir().join(format!(
            "flyline_test_refactor_{}.jsonl",
            uuid::Uuid::now_v7()
        ));
        let _ = std::fs::remove_file(&temp_file);

        let mut manager = HistoryManager::new_empty_with_path(Some(temp_file.clone()));

        let id1 = manager.push_entry_and_jsonl_append("cargo build".to_string());
        let _id2 = manager.push_entry_and_jsonl_append("cargo test".to_string());

        assert_eq!(manager.entries().len(), 2);

        let event3 = HistoryJsonlEvent::Start {
            id: "evt-3".to_string(),
            timestamp: TimestampNanos::new(1700000000000000000),
            command: "echo first".to_string(),
            cwd: None,
            hostname: None,
            session: "sess".to_string(),
        };
        manager.merge_jsonl_events(vec![event3]);
        assert_eq!(manager.entries().len(), 3);
        assert_eq!(manager.entries()[0].command, "echo first");

        let end_event = HistoryJsonlEvent::End {
            id: id1,
            timestamp: TimestampNanos::now(),
            exit_status: Some(0),
            pipestatus: Some("0".to_string()),
        };
        manager.merge_jsonl_events(vec![end_event]);
        assert_eq!(
            manager
                .entries()
                .iter()
                .find(|e| e.command == "cargo build")
                .unwrap()
                .exit_status(),
            Some(0)
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_format_history_entry_extra_info() {
        let mut entry = HistoryEntry::new(Some(1700000000000000000), 0, "cargo build".to_string());
        let meta = entry.metadata_mut();
        meta.id = Some("test-uuid-123".to_string());
        meta.cwd = Some("/home/user/project".to_string());
        meta.hostname = Some("my-laptop".to_string());
        meta.duration_ns = Some(1500000000);
        meta.exit_status = Some(0);
        meta.pipestatus = Some("0".to_string());

        let extra_info = entry.format_extra_info();
        assert!(extra_info.contains("Directory: /home/user/project"));
        assert!(extra_info.contains("Host: my-laptop"));
        assert!(extra_info.contains("Duration: 1.50s"));
        assert!(extra_info.contains("Exit Code: 0"));
        assert!(extra_info.contains("Pipeline Status: 0"));
        assert!(extra_info.contains("ID: test-uuid-123"));
    }

    #[test]
    fn test_pipeline_history_entry_formatting() {
        let mut entry = HistoryEntry::new(
            Some(1785451996774964850),
            0,
            "echo foo | exit 32 | echo asdf".to_string(),
        );
        let meta = entry.metadata_mut();
        meta.id = Some("019fb53b-6666-70f1-a720-c242714e4a5f".to_string());
        meta.cwd = Some("/home/hal/projects/flyline".to_string());
        meta.hostname = Some("hal-itx-pc".to_string());
        meta.duration_ns = Some(10000000);
        meta.exit_status = Some(0);
        meta.pipestatus = Some("0 32 0".to_string());

        let extra_info = entry.format_extra_info();
        assert!(extra_info.contains("Directory: /home/hal/projects/flyline"));
        assert!(extra_info.contains("Host: hal-itx-pc"));
        assert!(extra_info.contains("Time: 2026-07-30"));
        assert!(extra_info.contains("Duration: 10ms"));
        assert!(extra_info.contains("Exit Code: 0"));
        assert!(extra_info.contains("Pipeline Status: 0 32 0"));
        assert!(extra_info.contains("ID: 019fb53b-6666-70f1-a720-c242714e4a5f"));
    }

    #[test]
    fn test_custom_history_session_isolation_until_fuzzy_refresh() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("flyline_test_hist_{}.jsonl", uuid::Uuid::now_v7()));

        let mut manager1 = HistoryManager::new_empty_with_path(Some(temp_file.clone()));
        assert!(manager1.is_empty());

        // Initial load
        manager1.refresh_jsonl_backend();
        assert_eq!(manager1.entries().len(), 0);

        // Session 2 runs command A
        let mut manager2 = HistoryManager::new_empty_with_path(Some(temp_file.clone()));
        manager2.refresh_jsonl_backend();
        manager2.push_entry_and_jsonl_append("echo session2_cmd_a".to_string());

        // Session 1 runs command B (appends without advancing read offset)
        manager1.push_entry_and_jsonl_append("echo session1_cmd_b".to_string());
        assert_eq!(manager1.entries().len(), 1);

        // Session 2 runs command C
        manager2.push_entry_and_jsonl_append("echo session2_cmd_c".to_string());

        // In Session 1, pressing Up returns Session 1's command
        let up_cmd = manager1.search_in_history("", HistorySearchDirection::Backward);
        assert_eq!(
            up_cmd.as_ref().map(|e| e.command.as_str()),
            Some("echo session1_cmd_b")
        );

        // When fuzzy history search is triggered in Session 1 (refresh_jsonl_backend called),
        // it must read all interleaved entries (A, B, C) without skipping any!
        manager1.refresh_jsonl_backend();
        assert_eq!(manager1.entries().len(), 3);
        assert!(
            manager1
                .entries()
                .iter()
                .any(|e| e.command == "echo session2_cmd_a")
        );
        assert!(
            manager1
                .entries()
                .iter()
                .any(|e| e.command == "echo session1_cmd_b")
        );
        assert!(
            manager1
                .entries()
                .iter()
                .any(|e| e.command == "echo session2_cmd_c")
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_user_scenario_d1_d2_d3_fuzzy_history() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "flyline_test_user_hist_{}.jsonl",
            uuid::Uuid::now_v7()
        ));

        // 0. Pre-populate file with existing command
        let init_event_start = HistoryJsonlEvent::Start {
            id: "init-0".to_string(),
            timestamp: TimestampNanos::now(),
            command: "echo init".to_string(),
            cwd: None,
            hostname: None,
            session: "init_sess".to_string(),
        };
        let init_event_end = HistoryJsonlEvent::End {
            id: "init-0".to_string(),
            timestamp: TimestampNanos::now(),
            exit_status: Some(0),
            pipestatus: None,
        };
        append_jsonl_history_event(&init_event_start, &temp_file).unwrap();
        append_jsonl_history_event(&init_event_end, &temp_file).unwrap();

        // 1. Shell 1 starts up and loads initial history
        let mut manager1 = HistoryManager::new_empty_with_path(Some(temp_file.clone()));
        if manager1.is_empty() {
            manager1.refresh_jsonl_backend();
        }
        assert_eq!(manager1.entries().len(), 1);
        let offset_after_init = manager1.last_jsonl_read_offset.clone();
        assert_eq!(
            offset_after_init.as_ref().map(|o| o.event_id.as_str()),
            Some("init-0")
        );

        // 2. Shell 1 writes d1 (Start + End events)
        let cmd1_id = manager1.push_entry_and_jsonl_append("d1".to_string());
        manager1.set_last_submitted_command(cmd1_id, Instant::now());
        manager1.record_last_command_end(0, None);
        // Ensure write does NOT change last_jsonl_read_offset
        assert_eq!(manager1.last_jsonl_read_offset, offset_after_init);

        // 3. Shell 2 starts up and loads history
        let mut manager2 = HistoryManager::new_empty_with_path(Some(temp_file.clone()));
        if manager2.is_empty() {
            manager2.refresh_jsonl_backend();
        }

        // 4. Shell 2 writes d2 (Start + End events)
        let cmd2_id = manager2.push_entry_and_jsonl_append("d2".to_string());
        manager2.set_last_submitted_command(cmd2_id, Instant::now());
        manager2.record_last_command_end(0, None);

        // 5. Shell 1 writes d3 (Start + End events)
        let cmd3_id = manager1.push_entry_and_jsonl_append("d3".to_string());
        manager1.set_last_submitted_command(cmd3_id, Instant::now());
        manager1.record_last_command_end(0, None);
        // Ensure last_jsonl_read_offset still matches initial read
        assert_eq!(manager1.last_jsonl_read_offset, offset_after_init);

        // 6. Shell 1 opens fuzzy history -> calls refresh_jsonl_backend()
        manager1.refresh_jsonl_backend();
        let (entries, _results, ..) = manager1.get_fuzzy_search_results("", 20, Some(0));

        let cmds: Vec<&str> = entries.iter().map(|e| e.command.as_str()).collect();
        assert!(cmds.contains(&"echo init"), "echo init missing: {:?}", cmds);
        assert!(cmds.contains(&"d1"), "d1 missing: {:?}", cmds);
        assert!(cmds.contains(&"d2"), "d2 missing: {:?}", cmds);
        assert!(cmds.contains(&"d3"), "d3 missing: {:?}", cmds);

        let _ = std::fs::remove_file(&temp_file);
    }
}
