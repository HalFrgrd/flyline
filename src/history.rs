use itertools::Itertools;

use crate::{bash_funcs, bash_symbols};

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
}

pub enum HistorySearchDirection {
    Backward,
    Forward,
}

impl HistoryManager {
    /// Read the user's bash history file into a Vec<String>.
    /// Tries $HISTFILE first, otherwise falls back to $HOME/.bash_history.
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
                            timestamp_str.parse::<u64>().ok()
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
                    log::warn!("History parsing stopped at {} entries to prevent infinite loop", index);
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

        let content = std::fs::read_to_string(hist_path).unwrap_or_default();
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

        let bash_entries_from_memory = Self::parse_bash_history_from_memory();
        log::debug!(
            "Parsed bash history from memory ({} entries)",
            bash_entries_from_memory.len()
        );
        for entry in bash_entries_from_memory.iter().take(5) {
            log::debug!("  [{}] {}", entry.index, entry.command);
        }

        let bash_entries = Self::parse_bash_history_from_file();
        let zsh_entries = Self::parse_zsh_history();

        let entries: Vec<_> = bash_entries
            .into_iter()
            .merge_by(zsh_entries, |a, b| {
                a.timestamp.unwrap_or(0) <= b.timestamp.unwrap_or(0)
            })
            .collect();

        let index = entries.len();
        HistoryManager {
            entries,
            index,
            last_search_prefix: None,
            last_buffered_command: None,
        }
    }

    pub fn new_session(&mut self) {
        self.index = self.entries.len();
        self.last_buffered_command = None;
        self.last_search_prefix = None;
    }

    pub fn add_entry(&mut self, ts: Option<u64>, command: &str) {
        let entry = HistoryEntry {
            timestamp: ts,
            index: self.entries.len(),
            command: command.to_string(),
        };
        self.entries.push(entry);
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
