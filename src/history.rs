use std::ops::Index;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub timestamp: Option<u64>,
    pub command: String,
}

#[derive(Debug)]
pub struct HistoryManager {
    entries: Vec<HistoryEntry>,
    index: usize,
}

impl HistoryManager {
    /// Read the user's bash history file into a Vec<String>.
    /// Tries $HISTFILE first, otherwise falls back to $HOME/.bash_history.
    fn parse_bash_history() -> Vec<HistoryEntry> {
        let start_time = std::time::Instant::now();

        let hist_path = std::env::var("HISTFILE").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.bash_history", home)
        });

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

    pub fn new() -> HistoryManager {
        let entries = Self::parse_bash_history();
        let index = entries.len();
        HistoryManager { entries, index }
    }

    pub fn new_session(&mut self) {
        self.index = self.entries.len();
    }

    pub fn add_entry(&mut self, entry: HistoryEntry) {
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
                    command: l.to_string(),
                };
                res.push(entry);
                None
            }
            // TODO multiline commands
        });

        res
    }

    pub fn get_command_suggestions(history: &Vec<HistoryEntry>, prefix: &str) -> Vec<String> {
        let mut suggestions = Vec::new();
        for entry in history.iter().rev() {
            if entry.command.starts_with(prefix) && !suggestions.contains(&entry.command) {
                suggestions.push(entry.command.clone());
            }
        }
        suggestions
    }

    pub fn go_back_in_history(&mut self) -> Option<&HistoryEntry> {
        self.index = self.index.saturating_sub(1);
        self.entries.get(self.index)
    }

    pub fn go_forward_in_history(&mut self) -> Option<&HistoryEntry> {
        self.index = (self.index + 1).min(self.entries.len() - 1);
        self.entries.get(self.index)
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

        let mut check = |expected_ts: Option<u64>, expected_cmd: &str| {
            let entry = entries_iter.next().unwrap();
            assert_eq!(entry.timestamp, expected_ts);
            assert_eq!(entry.command, expected_cmd);
        };

        check(Some(1625078400), "ls -al");
        check(Some(1625078460), "echo 'Hello, World!'");
        check(None, "pwd");
        check(None, "#cd /asdf/asdf");
        check(None, "cd /home/user");
        check(Some(1625078460), "cd /home/user2");
    }
}
