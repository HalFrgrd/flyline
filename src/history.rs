use std::time::Instant;
use std::vec;

use crate::bash_symbols;
use crate::palette::Palette;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use itertools::Itertools;
use ratatui::text::Line;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub timestamp: Option<u64>,
    pub index: usize,
    pub command: String,
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
        log::info!(
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
                            let ts_str = timestamp_str.trim_start_matches('#').trim();
                            ts_str.parse::<u64>().ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let entry = HistoryEntry {
                        timestamp,
                        index: index as usize,
                        command: command_str,
                    };
                    res.push(entry);
                }

                index += 1;

                // Safety check to prevent infinite loops
                if index > 100000 {
                    log::warn!(
                        "History parsing stopped at {} entries to prevent infinite loop",
                        index
                    );
                    break;
                }
            }
        }
        res
    }

    fn parse_zsh_history() -> Vec<HistoryEntry> {
        let start_time = std::time::Instant::now();

        let hist_path = {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.zsh_history", home)
        };

        log::debug!("Reading zsh history from: {}", hist_path);

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
            Err(e) => {
                log::error!("Failed to read zsh history from {}: {}", hist_path, e);
                String::new()
            }
        };
        let res = HistoryManager::parse_zsh_history_str(&content);

        let duration = start_time.elapsed();
        log::info!(
            "Parsed zsh history ({} entries) in {:?}",
            res.len(),
            duration
        );
        res
    }

    pub fn new() -> HistoryManager {
        // Bash will load the history into memory, so we can read it from there
        // Bash parses it after bashrc is loaded.
        let bash_entries = Self::parse_bash_history_from_memory();
        // Print last 5 bash entries for debugging
        if bash_entries.is_empty() {
            log::warn!("No bash history entries found");
        } else {
            log::info!("Loaded {} bash history entries", bash_entries.len());
            for entry in bash_entries.iter().rev().take(5) {
                log::info!("bash_entries => {:?}", entry);
            }
        }

        // Alternative is to do it ourselves
        // let bash_entries = Self::parse_bash_history_from_file();

        // As a zsh user migrating to bash, I want to have my zsh history available too
        let zsh_entries = Self::parse_zsh_history();

        let mut entries: Vec<_> = zsh_entries
            .into_iter()
            .merge_by(bash_entries, |a, b| {
                a.timestamp.unwrap_or(0) <= b.timestamp.unwrap_or(0)
            })
            .collect();
        // let mut entries = bash_entries;

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

    fn parse_timestamp(line: &str) -> Option<u64> {
        if line.starts_with('#') {
            if let Ok(ts) = line[1..].trim().parse::<u64>() {
                Some(ts)
            } else {
                None
            }
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
                let entry = HistoryEntry {
                    timestamp: my_ts,
                    index: res.len(),
                    command: l.to_string(),
                };
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

            let entry = HistoryEntry {
                timestamp,
                index: res.len(),
                command,
            };
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
            .map_or(true, |c| c != current_cmd);

        if self.last_search_prefix.is_none() || is_command_different_to_last_buffered {
            self.last_search_prefix = Some(current_cmd.to_string());
        }

        let prefix = self.last_search_prefix.as_ref().unwrap();

        let indices: Vec<usize> = match direction {
            HistorySearchDirection::Backward => (0..self.index).rev().collect(),
            HistorySearchDirection::Forward => (self.index + 1..self.entries.len()).collect(),
        };

        for i in indices {
            let entry = &self.entries[i];
            if entry.command.starts_with(prefix) {
                self.last_buffered_command = Some(entry.command.clone());
                self.index = i;
                return Some(entry.clone());
            }
        }

        None
    }

    pub(crate) fn get_fuzzy_search_results(
        &mut self,
        current_cmd: &str,
    ) -> (&mut [HistoryEntryFormatted], usize, usize, usize) {
        self.fuzzy_search
            .get_fuzzy_search_results(&mut self.entries, current_cmd)
    }

    pub fn accept_fuzzy_search_result(&self) -> Option<&HistoryEntry> {
        self.fuzzy_search.accept_fuzzy_search_result()
    }

    pub fn fuzzy_search_set_by_visual_idx(&mut self, visual_idx: usize) {
        self.fuzzy_search.set_fuzzy_search_by_visual_idx(visual_idx);
    }

    pub fn fuzzy_search_onkeypress(&mut self, direction: HistorySearchDirection) {
        self.fuzzy_search.fuzzy_search_onkeypress(direction);
    }

    // fuzzy search cache logic moved to FuzzyHistorySearch
}

pub(crate) struct HistoryEntryFormatted {
    pub entry: HistoryEntry,
    pub score: i64,
    pub match_indices: Vec<usize>,
    pub command_spans: Option<Vec<Line<'static>>>,
    pub command_spans_selected: Option<Vec<Line<'static>>>,
}

impl HistoryEntryFormatted {
    fn new(entry: HistoryEntry, score: i64, match_indices: Vec<usize>) -> Self {
        HistoryEntryFormatted {
            entry,
            score,
            match_indices,
            command_spans: None,
            command_spans_selected: None,
        }
    }

    pub fn gen_formatted_command(&mut self) {
        if self.command_spans.is_some() && self.command_spans_selected.is_some() {
            return;
        }

        let (command_spans, command_spans_selected) =
            Palette::highlight_maching_indices(&self.entry.command, &self.match_indices);

        self.command_spans = Some(command_spans);
        self.command_spans_selected = Some(command_spans_selected);
    }
}

struct FuzzyHistorySearch {
    matcher: SkimMatcherV2,
    cache: Vec<HistoryEntryFormatted>,
    cache_command: Option<String>,
    global_index: usize,
    cache_index: usize,
    cache_visible_offset: usize,
}

impl std::fmt::Debug for FuzzyHistorySearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FuzzyHistorySearch")
            .field("cache_command", &self.cache_command)
            .field("global_index", &self.global_index)
            .field("cache_index", &self.cache_index)
            .field("cache_visible_offset", &self.cache_visible_offset)
            .field("cache_len", &self.cache.len())
            .finish()
    }
}

impl FuzzyHistorySearch {
    // Check time budget every N entries to balance responsiveness and performance
    const TIME_CHECK_INTERVAL: usize = 64;
    // Time budget for processing history entries in milliseconds
    const TIME_BUDGET_MS: u64 = 15;

    fn new() -> Self {
        FuzzyHistorySearch {
            matcher: SkimMatcherV2::default().smart_case(),
            cache: Vec::new(),
            cache_command: None,
            global_index: 0,
            cache_index: 0,
            cache_visible_offset: 0,
        }
    }

    fn get_fuzzy_search_results(
        &mut self,
        entries: &[HistoryEntry],
        current_cmd: &str,
    ) -> (&mut [HistoryEntryFormatted], usize, usize, usize) {
        // when the command changes, reset the cache
        // but keep the current visual row if possible
        let mut desired_visual_row = None;

        if Some(current_cmd.to_string()) != self.cache_command {
            self.cache_command = Some(current_cmd.to_string());
            self.cache = vec![];
            self.global_index = 0;
            desired_visual_row = Some(self.cache_index.saturating_sub(self.cache_visible_offset));
            self.cache_index = 0;
            self.cache_visible_offset = 0;
        }

        self.grow_fuzzy_search_cache(entries, current_cmd);

        if let Some(desired_row) = desired_visual_row {
            self.cache_index = self.cache_visible_offset + desired_row;
        }

        self.cache_index = self.cache_index.min(self.cache.len().saturating_sub(1));

        let visible_cache_size = 18;

        if self.cache_visible_offset + visible_cache_size <= self.cache_index + 2 {
            self.cache_visible_offset =
                (self.cache_index + 2).saturating_sub(visible_cache_size - 1);
        } else if self.cache_index < self.cache_visible_offset + 2 {
            self.cache_visible_offset = self.cache_index.saturating_sub(2);
        }

        assert!(self.cache_index >= self.cache_visible_offset);
        let visible_index = self.cache_index.saturating_sub(self.cache_visible_offset);

        let cache_len = self.cache.len();

        let end = (self.cache_visible_offset + visible_cache_size).min(cache_len);

        {
            let entries_to_show = &mut self.cache[self.cache_visible_offset..end];
            entries_to_show
                .iter_mut()
                .for_each(|e| e.gen_formatted_command());
        }

        (
            &mut self.cache[self.cache_visible_offset..end],
            visible_index,
            cache_len,
            self.global_index,
        )
    }

    fn accept_fuzzy_search_result(&self) -> Option<&HistoryEntry> {
        if self.cache.is_empty() {
            return None;
        }
        self.cache
            .get(self.cache_index)
            .map(|formatted| &formatted.entry)
    }

    fn set_fuzzy_search_by_visual_idx(&mut self, visual_idx: usize) {
        let new_index = self.cache_visible_offset + visual_idx;
        if new_index < self.cache.len() {
            self.cache_index = new_index;
        }
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

        let mut new_entries = vec![];

        // Process as many entries as possible within the time budget
        for (idx, entry) in entries.iter().rev().skip(self.global_index).enumerate() {
            // Check if we've exceeded the time budget every TIME_CHECK_INTERVAL entries
            if idx % Self::TIME_CHECK_INTERVAL == 0 && start.elapsed() >= time_budget {
                break;
            }

            if let Some((score, indices)) = self.matcher.fuzzy_indices(&entry.command, current_cmd)
            {
                if score >= score_threshold {
                    let new_entry = HistoryEntryFormatted::new(entry.clone(), score, indices);
                    new_entries.push(new_entry);
                }
            }
            self.global_index += 1;
        }

        // Sort explicitly by score. Then insert stable order of history entries
        new_entries.sort_by_key(|e| std::cmp::Reverse(e.score));

        let mut new_cache = std::mem::take(&mut self.cache)
            .into_iter()
            .merge_by(new_entries.into_iter(), |a, b| a.score >= b.score)
            .collect::<Vec<_>>();

        // Remove duplicates, keeping the lowest indexed one (first occurrence)
        new_cache.dedup_by(|a, b| a.entry.command == b.entry.command);
        self.cache = new_cache;

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
}
