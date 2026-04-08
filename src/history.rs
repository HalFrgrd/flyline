use std::cell::OnceCell;
use std::time::Instant;
use std::vec;

use crate::bash_symbols;
use crate::palette::Palette;
use crate::settings::Settings;
use crate::stateful_sliding_window::StatefulSlidingWindow;
use flash::lexer::TokenKind;
use itertools::Itertools;
use ratatui::text::{Line, Span};
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub timestamp: Option<u64>,
    pub index: usize,
    pub command: String,
    syntax_highlighted: OnceCell<Vec<Line<'static>>>,
}

impl HistoryEntry {
    fn new(timestamp: Option<u64>, index: usize, command: String) -> Self {
        HistoryEntry {
            timestamp,
            index,
            command,
            syntax_highlighted: OnceCell::new(),
        }
    }

    pub fn get_syntax_highlighted(&self, palette: &Palette) -> &Vec<Line<'static>> {
        self.syntax_highlighted.get_or_init(|| {
            let mut parser = crate::dparser::DParser::from(&self.command as &str);
            parser.walk_to_end();
            let tokens = parser.into_tokens();
            let formatted = crate::app::formated_buffer::format_buffer(
                &tokens,
                self.command.len(),
                self.command.len(),
                false,
                Some(Box::new(crate::app::get_word_info)),
                palette,
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

#[derive(Debug)]
pub struct HistoryManager {
    entries: Vec<HistoryEntry>,
    index: usize,
    last_search_prefix: Option<String>,
    last_buffered_command: Option<String>,
    fuzzy_search: FuzzyHistorySearch,
}

pub enum HistorySearchDirection {
    Backward,
    Forward,
    PageBackward,
    PageForward,
}

impl HistoryManager {
    /// Read the user's bash history file into a Vec<String>.
    /// Tries $HISTFILE first, otherwise falls back to $HOME/.bash_history.
    #[allow(dead_code)]
    fn parse_bash_history_from_file() -> Vec<HistoryEntry> {
        let start_time = std::time::Instant::now();

        let hist_path = std::env::var("HISTFILE").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.bash_history", home)
        });

        log::debug!("Reading bash history from: {}", hist_path);

        let content = std::fs::read_to_string(hist_path).unwrap_or_default();
        let res = HistoryManager::parse_bash_history_str(&content);

        let duration = start_time.elapsed();
        log::debug!(
            "Parsed bash history ({} entries) in {:?}",
            res.len(),
            duration
        );
        res
    }

    pub fn parse_bash_history_from_memory() -> Vec<HistoryEntry> {
        let mut res = Vec::new();
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
        let start_time = std::time::Instant::now();

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
        let res = HistoryManager::parse_zsh_history_str(&content);

        let duration = start_time.elapsed();
        log::debug!(
            "Parsed Zsh history ({} entries) in {:?}",
            res.len(),
            duration
        );
        res
    }

    pub fn new(settings: &Settings) -> HistoryManager {
        // Bash will load the history into memory, so we can read it from there
        // Bash parses it after bashrc is loaded.
        let bash_entries = Self::parse_bash_history_from_memory();
        // Print last 5 bash entries for debugging
        if bash_entries.is_empty() {
            log::warn!("No bash history entries found");
        } else {
            log::debug!("Loaded {} bash history entries", bash_entries.len());
            for entry in bash_entries.iter().rev().take(5) {
                log::debug!("bash_entries => {:?}", entry);
            }
        }

        // Alternative is to do it ourselves
        // let bash_entries = Self::parse_bash_history_from_file();

        let mut entries: Vec<_> = if let Some(ref zsh_path) = settings.zsh_history_path {
            // As a Zsh user migrating to Bash, I want to have my Zsh history available too
            let zsh_entries = Self::parse_zsh_history(Some(zsh_path.as_str()));
            log::debug!("Loaded {} Zsh history entries", zsh_entries.len());
            for entry in zsh_entries.iter().rev().take(5) {
                log::debug!("zsh_entries => {:?}", entry);
            }
            zsh_entries
                .into_iter()
                .merge_by(bash_entries, |a, b| {
                    a.timestamp.unwrap_or(0) <= b.timestamp.unwrap_or(0)
                })
                .collect()
        } else {
            bash_entries
        };

        entries.dedup_by(|a, b| a.command == b.command);

        let mut i = 0;
        for entry in &mut entries {
            entry.index = i;
            i += 1;
        }

        let index = entries.len();
        // SAFETY: We transmute the lifetime to 'static because entries lives as long as HistoryManager
        HistoryManager {
            entries,
            index,
            last_search_prefix: None,
            last_buffered_command: None,
            fuzzy_search: FuzzyHistorySearch::new(),
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
        }
    }

    /// Push a new entry to the history list.
    /// `self.index` is kept at `entries.len()` (past-the-end), matching the
    /// invariant established by `new()` and `HistoryManager::search_in_history`.
    /// Resets the fuzzy search cache so the new entry is visible immediately.
    pub fn push_entry(&mut self, command: String) {
        if command.trim().is_empty() {
            return;
        }
        let index = self.entries.len();
        self.entries.push(HistoryEntry::new(None, index, command));
        self.index = self.entries.len();
        self.fuzzy_search.clear_cache();
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
    ) -> (
        &[HistoryEntry],
        &[HistoryEntryFormatted],
        usize,
        usize,
        usize,
    ) {
        let (formatted, idx, num_results, num_searched) = self
            .fuzzy_search
            .get_fuzzy_search_results(&self.entries, current_cmd, max_visible);
        (&self.entries, formatted, idx, num_results, num_searched)
    }

    /// Pre-warm the fuzzy search cache when entering fuzzy-search mode.
    /// Uses the default visible window size as the actual terminal size is not yet
    /// available at keypress time.  The render path will call `get_fuzzy_search_results`
    /// with the correct dynamic size on the next frame.
    pub(crate) fn warm_fuzzy_search_cache(&mut self, current_cmd: &str) {
        let _ = self.fuzzy_search.get_fuzzy_search_results(
            &self.entries,
            current_cmd,
            FuzzyHistorySearch::VISIBLE_CACHE_SIZE,
        );
    }

    pub fn accept_fuzzy_search_result(&self) -> Option<&HistoryEntry> {
        self.fuzzy_search.accept_fuzzy_search_result(&self.entries)
    }

    pub fn fuzzy_search_set_idx(&mut self, idx: usize) {
        self.fuzzy_search.set_fuzzy_search_idx(idx);
    }

    pub fn fuzzy_search_onkeypress(&mut self, direction: HistorySearchDirection) {
        self.fuzzy_search.fuzzy_search_onkeypress(direction);
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
    fn new(entry_index: usize, score: i64, match_indices: Vec<usize>) -> Self {
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
            palette.apply_match_indices_to_lines(base_lines, &self.match_indices)
        })
    }
}

struct FuzzyHistorySearch {
    matcher: ArinaeMatcher,
    cache: Vec<HistoryEntryFormatted>,
    cache_command: Option<String>,
    global_index: usize,
    cache_index: usize,
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
            cache_index: 0,
            window: StatefulSlidingWindow::new(0, Self::VISIBLE_CACHE_SIZE, 0),
        }
    }

    fn clear_cache(&mut self) {
        self.cache.clear();
        self.cache_command = None;
        self.global_index = 0;
        self.cache_index = 0;
        self.window = StatefulSlidingWindow::new(0, Self::VISIBLE_CACHE_SIZE, 0);
    }

    fn get_fuzzy_search_results(
        &mut self,
        entries: &[HistoryEntry],
        current_cmd: &str,
        max_visible: usize,
    ) -> (&[HistoryEntryFormatted], usize, usize, usize) {
        // when the command changes, reset the cache
        // but keep the current visual row if possible
        let mut desired_visual_row = None;

        if Some(current_cmd.to_string()) != self.cache_command {
            self.cache_command = Some(current_cmd.to_string());
            self.cache = vec![];
            self.global_index = 0;
            desired_visual_row = Some(self.window.visual_index_of_interest());
            self.cache_index = 0;
            self.window = StatefulSlidingWindow::new(0, Self::VISIBLE_CACHE_SIZE, 0);
        }

        self.grow_fuzzy_search_cache(entries, current_cmd);

        let cache_len = self.cache.len();

        self.window.update_max_index(cache_len);
        self.window.update_window_size(max_visible);
        if let Some(desired) = desired_visual_row {
            self.window.set_visual_index_of_interest(desired);
        }
        self.window.move_index_to(self.cache_index);

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
        self.cache
            .get(self.cache_index)
            .map(|formatted| &entries[formatted.entry_index])
    }

    fn set_fuzzy_search_idx(&mut self, idx: usize) {
        self.cache_index = idx.min(self.cache.len().saturating_sub(1));
    }

    fn fuzzy_search_onkeypress(&mut self, direction: HistorySearchDirection) {
        if self.cache.is_empty() {
            return;
        }
        match direction {
            HistorySearchDirection::Backward => {
                if self.cache_index + 1 < self.cache.len() {
                    self.cache_index += 1;
                }
            }
            HistorySearchDirection::Forward => {
                if self.cache_index > 0 {
                    self.cache_index -= 1;
                }
            }
            HistorySearchDirection::PageBackward => {
                self.cache_index = (self.cache_index + self.window.get_window_range().len())
                    .min(self.cache.len() - 1);
            }
            HistorySearchDirection::PageForward => {
                self.cache_index = self
                    .cache_index
                    .saturating_sub(self.window.get_window_range().len());
            }
        }
    }

    fn grow_fuzzy_search_cache(&mut self, entries: &[HistoryEntry], current_cmd: &str) {
        let start = Instant::now();
        let start_index = self.global_index;
        let time_budget = std::time::Duration::from_millis(Self::TIME_BUDGET_MS);

        let score_threshold = match current_cmd.len() {
            0..1 => 0,
            1..3 => 10,
            3..5 => 20,
            _ => 30,
        };

        let mut new_cache_entries = vec![];

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

            if let Some((score, indices)) = self.matcher.fuzzy_indices(&entry.command, current_cmd)
                && score >= score_threshold
            {
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
}
