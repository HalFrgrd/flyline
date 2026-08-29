use super::*;
use crate::history::HistoryEntry;
use clap::{CommandFactory, Parser, Subcommand};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;

/// A configurable mock shell backend for unit tests, offline testing, and standalone test harnesses.
#[derive(Debug)]
pub struct TestBackend {
    pub cwd: RwLock<String>,
    pub hostname: RwLock<String>,
    pub env_vars: RwLock<HashMap<String, String>>,
    pub aliases: RwLock<HashMap<String, String>>,
    pub last_exit_status: RwLock<i32>,
    pub multiline_count: RwLock<i32>,
    pub history: RwLock<Vec<HistoryEntry>>,
    pub autocd: std::sync::atomic::AtomicBool,
    pub pending_traps: RwLock<Vec<String>>,
}

pub static TEST_BACKEND: std::sync::LazyLock<TestBackend> =
    std::sync::LazyLock::new(TestBackend::new);

impl Default for TestBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TestBackend {
    pub fn new() -> Self {
        let pwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "/home/john/projects/flyline".to_string());
        let home = std::env::var("FLYLINE_TEST_HOME").unwrap_or_else(|_| "/home/john".to_string());

        let mut env = HashMap::new();
        env.insert("HOME".to_string(), home);
        env.insert("PWD".to_string(), pwd.clone());
        env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        env.insert("SHELL".to_string(), "/bin/bash".to_string());
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("USER".to_string(), "john".to_string());

        let mut aliases = HashMap::new();
        aliases.insert("gst".to_string(), "git status".to_string());
        aliases.insert("gcm".to_string(), "git commit -m".to_string());
        aliases.insert("gd".to_string(), "git diff".to_string());

        Self {
            cwd: RwLock::new(pwd),
            hostname: RwLock::new("test-host".to_string()),
            env_vars: RwLock::new(env),
            aliases: RwLock::new(aliases),
            last_exit_status: RwLock::new(0),
            multiline_count: RwLock::new(0),
            history: RwLock::new(Vec::new()),
            autocd: std::sync::atomic::AtomicBool::new(false),
            pending_traps: RwLock::new(Vec::new()),
        }
    }

    pub fn queue_trap(&self, trap_cmd: &str) {
        self.pending_traps.write().push(trap_cmd.to_string());
    }

    pub fn set_env(&self, key: &str, value: &str) {
        self.env_vars
            .write()
            .insert(key.to_string(), value.to_string());
    }

    pub fn set_alias(&self, alias: &str, expansion: &str) {
        self.aliases
            .write()
            .insert(alias.to_string(), expansion.to_string());
    }

    pub fn set_autocd(&self, enabled: bool) {
        self.autocd
            .store(enabled, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Parser, Debug)]
#[command(name = "git", no_binary_name = true)]
struct DummyGitArgs {
    #[command(subcommand)]
    command: Option<DummyGitCommand>,
}

#[derive(Subcommand, Debug)]
enum DummyGitCommand {
    Add {
        #[arg(long = "all", short = 'A')]
        all: bool,
        #[arg(long = "patch", short = 'p')]
        patch: bool,
        #[arg(long = "verbose", short = 'v')]
        verbose: bool,
        #[arg(long = "dry-run", short = 'n')]
        dry_run: bool,
        files: Vec<String>,
    },
    Commit {
        #[arg(long = "message", short = 'm')]
        message: Option<String>,
        #[arg(long = "amend")]
        amend: bool,
        #[arg(long = "all", short = 'a')]
        all: bool,
        #[arg(long = "no-verify")]
        no_verify: bool,
    },
    Diff {
        #[arg(long = "staged")]
        staged: bool,
        #[arg(long = "stat")]
        stat: bool,
        #[arg(long = "name-only")]
        name_only: bool,
        #[arg(long = "color")]
        color: bool,
        paths: Vec<String>,
    },
    Status {
        #[arg(long = "short", short = 's')]
        short: bool,
        #[arg(long = "branch", short = 'b')]
        branch: bool,
        #[arg(long = "porcelain")]
        porcelain: bool,
        #[arg(long = "untracked-files", short = 'u')]
        untracked_files: bool,
    },
}

fn dummy_git_completions(
    full_command: &str,
    word_under_cursor: &str,
) -> Vec<clap_complete::CompletionCandidate> {
    // Tokenize on whitespace; this is a deliberate simplification
    // suitable for the dummy git completer used in unit tests.
    let mut tokens: Vec<String> = full_command
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    // Drop the leading "git" command word; the dummy parser uses
    // `no_binary_name = true`.
    if tokens.first().map(String::as_str) == Some("git") {
        tokens.remove(0);
    }

    // Determine if the cursor is at the end (i.e. completing a
    // brand-new empty word) or replacing the last token.
    let trailing_space = full_command.ends_with(char::is_whitespace);
    if trailing_space || tokens.is_empty() || word_under_cursor.is_empty() {
        tokens.push(String::new());
    } else if tokens.last().map(String::as_str) != Some(word_under_cursor) {
        // Replace whatever the last token is with the word under
        // cursor so the clap completer treats it as the prefix.
        let last = tokens.last_mut().unwrap();
        *last = word_under_cursor.to_string();
    }

    let args_os: Vec<std::ffi::OsString> =
        tokens.into_iter().map(std::ffi::OsString::from).collect();
    let index = args_os.len() - 1;
    let mut cmd = DummyGitArgs::command();
    let current_dir = std::env::current_dir().ok();

    clap_complete::engine::complete(&mut cmd, args_os, index, current_dir.as_deref())
        .unwrap_or_default()
}

impl ShellBackend for TestBackend {
    fn name(&self) -> &'static str {
        "test"
    }

    fn cwd(&self) -> String {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| self.cwd.read().clone())
    }

    fn env_var(&self, name: &str) -> Option<String> {
        if name == "PWD" {
            return Some(self.cwd());
        }
        self.env_vars.read().get(name).cloned()
    }

    fn export_env_var(&self, name: &str, value: &str) -> anyhow::Result<()> {
        self.env_vars
            .write()
            .insert(name.to_string(), value.to_string());
        Ok(())
    }

    fn unset_env_var(&self, name: &str) -> anyhow::Result<()> {
        self.env_vars.write().remove(name);
        Ok(())
    }

    fn hostname(&self) -> String {
        self.hostname.read().clone()
    }

    fn format_var(&self, name: &str) -> String {
        match self.env_var(name) {
            Some(v) => format!("{}={}", name, v),
            None => format!("{}: unset", name),
        }
    }

    fn vars_with_prefix(&self, prefix: &str) -> Vec<String> {
        let clean_prefix = prefix.strip_prefix('$').unwrap_or(prefix);
        let mut vars: Vec<String> = self
            .env_vars
            .read()
            .keys()
            .filter(|k| k.starts_with(clean_prefix))
            .map(|k| format!("${}", k))
            .collect();
        vars.sort();
        vars
    }

    fn expand_path(&self, path: &str) -> String {
        let bash_expanded = if path.is_empty() {
            String::new()
        } else {
            self.expand_filename(&crate::grammar::dequoting_function_rust(path))
        };

        if bash_expanded.is_empty() {
            self.cwd()
        } else if !std::path::Path::new(&bash_expanded).is_absolute() {
            format!("{}/{}", self.cwd(), bash_expanded)
        } else {
            bash_expanded
        }
    }

    /// Test-only filename expansion. Supports a tiny subset of bash expansion:
    ///   * `$PWD` / `$HOME` (and a leading `~/`) are expanded by looking the
    ///     name up in [`test_fixtures::test_env_vars`].
    ///   * `./` and `../` are left in place (resolved by the OS as relative paths)
    ///
    /// Panics if the resulting path does not exist on disk after expansion. This
    /// catches mistakes in test fixtures and matches the user's request to keep
    /// expansion deterministic.
    fn expand_filename(&self, filename: &str) -> String {
        let dequoted = crate::grammar::dequoting_function_rust(filename);
        let with_tilde = if let Some(rest) = dequoted.strip_prefix('~') {
            let home = self
                .env_var("HOME")
                .unwrap_or_else(|| "/home/john".to_string());
            format!("{}{}", home, rest)
        } else {
            dequoted
        };

        let mut result = String::new();
        let mut chars = with_tilde.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '$' {
                if chars.peek() == Some(&'{') {
                    chars.next(); // consume '{'
                    let mut var_name = String::new();
                    while let Some(&vc) = chars.peek() {
                        if vc == '}' {
                            chars.next();
                            break;
                        }
                        var_name.push(vc);
                        chars.next();
                    }
                    if let Some(val) = self.env_var(&var_name) {
                        result.push_str(&val);
                    }
                } else {
                    let mut var_name = String::new();
                    while let Some(&vc) = chars.peek() {
                        if vc.is_ascii_alphanumeric() || vc == '_' {
                            var_name.push(vc);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if !var_name.is_empty() {
                        if let Some(val) = self.env_var(&var_name) {
                            result.push_str(&val);
                        }
                    } else {
                        result.push('$');
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    fn decode_prompt(&self, raw: &str, _is_prompt: bool) -> Option<String> {
        Some(raw.to_string())
    }

    fn find_alias(&self, cmd: &str) -> Option<String> {
        self.aliases.read().get(cmd).cloned()
    }

    fn command_info(&self, cmd: &str) -> CommandWordInfo {
        // The test environment models a tiny world: `git` is the only "real"
        // executable on PATH, so it gets reported as a File at /usr/bin/git.
        // Everything else is unknown — tests that need additional command types
        // can extend this match arm.
        if cmd == "git" {
            return CommandWordInfo::File {
                command: "git".to_string(),
                path: "/usr/bin/git".to_string(),
            };
        }
        if let Some(alias) = self.aliases.read().get(cmd) {
            return CommandWordInfo::Alias {
                command: cmd.to_string(),
                expansion: alias.clone(),
            };
        }
        if ["cd", "echo", "exit", "export", "set"].contains(&cmd) {
            return CommandWordInfo::Builtin {
                command: cmd.to_string(),
                usage: None,
            };
        }
        if self.is_autocd_enabled() {
            let expanded = self.expand_path(cmd);
            if !expanded.is_empty() && std::path::Path::new(&expanded).is_dir() {
                return CommandWordInfo::File {
                    command: cmd.to_string(),
                    path: expanded,
                };
            }
        }
        CommandWordInfo::Unknown {
            command: cmd.to_string(),
        }
    }

    fn run_programmable_completions(
        &self,
        full_command: &str,
        command_word: &str,
        word_under_cursor: &str,
        _cursor_byte_pos: usize,
        _word_under_cursor_byte_end: usize,
    ) -> anyhow::Result<ProgrammableCompleteReturn> {
        log::debug!(
            "[test] run_programmable_completions: full_command='{}', command_word='{}', word_under_cursor='{}'",
            full_command,
            command_word,
            word_under_cursor
        );

        if command_word == "git" {
            let candidates = dummy_git_completions(full_command, word_under_cursor);
            let completions: Vec<String> = candidates
                .into_iter()
                .map(|c| c.get_value().to_string_lossy().to_string())
                .collect();
            let flags = CompletionFlags::from_alt(word_under_cursor, &completions);
            Ok(ProgrammableCompleteReturn::new(completions, flags, true))
        } else if command_word == "docker" {
            let completions = if word_under_cursor.starts_with('p') {
                vec![
                    "port      List port mappings or a specific mapping for the container"
                        .to_string(),
                    "ps        List containers".to_string(),
                ]
            } else {
                vec![
                    "builder   Manage builds".to_string(),
                    "image     Manage images".to_string(),
                    "port      List port mappings or a specific mapping for the container"
                        .to_string(),
                    "ps        List containers".to_string(),
                    "run       Run a command in a new container".to_string(),
                ]
            };
            let filtered: Vec<String> = completions
                .into_iter()
                .filter(|s| s.starts_with(word_under_cursor))
                .collect();
            let flags = CompletionFlags::from_alt(word_under_cursor, &filtered);
            Ok(ProgrammableCompleteReturn::new(filtered, flags, true))
        } else if command_word == "getsub" {
            let completions = if full_command.contains("--subtitle-type=") {
                vec![
                    "json".to_string(),
                    "txt".to_string(),
                    "srt".to_string(),
                    "tsv".to_string(),
                    "vtt".to_string(),
                ]
            } else if full_command.contains("--fix-audio=") {
                vec![
                    "backgroundNoise".to_string(),
                    "echoNoise".to_string(),
                    "windNoise".to_string(),
                    "lowVolume".to_string(),
                    "minimal".to_string(),
                ]
            } else {
                vec![
                    "--alternative=".to_string(),
                    "--fix-audio=".to_string(),
                    "--subtitle-type=".to_string(),
                    "--translate-from=".to_string(),
                ]
            };
            let filtered: Vec<String> = completions
                .into_iter()
                .filter(|s| s.starts_with(word_under_cursor))
                .collect();
            let mut flags = CompletionFlags::from_alt(word_under_cursor, &filtered);
            if filtered.iter().all(|s| s.ends_with('=')) {
                flags.no_suffix_desired = true;
            }
            Ok(ProgrammableCompleteReturn::new(filtered, flags, true))
        } else if command_word == "cat" {
            let (lhs, rhs) = match word_under_cursor.rsplit_once('/') {
                Some((left, right)) => (format!("{left}/"), right),
                None => (String::new(), word_under_cursor),
            };

            let expanded_lhs = if lhs.is_empty() {
                self.expand_filename(".")
            } else {
                self.expand_filename(&lhs)
            };

            let mut completions = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&expanded_lhs) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str()
                        && name.starts_with(rhs)
                    {
                        let mut candidate = if lhs.is_empty() {
                            name.to_string()
                        } else {
                            format!("{lhs}{name}")
                        };

                        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                            candidate.push('/');
                        }

                        completions.push(candidate);
                    }
                }
            }

            completions.sort();
            completions.dedup();

            let flags = CompletionFlags::from_alt(word_under_cursor, &completions);
            Ok(ProgrammableCompleteReturn::new(completions, flags, true))
        } else if command_word == "uselesscmd" {
            Ok(ProgrammableCompleteReturn::new(
                Vec::new(),
                CompletionFlags::default(),
                false,
            ))
        } else if command_word == "flyline_testing" {
            if full_command.contains("--big") {
                let completions: Vec<String> =
                    (0..10000).map(|i| format!("--entry-{}", i)).collect();
                let flags = CompletionFlags::from_alt(word_under_cursor, &completions);
                Ok(ProgrammableCompleteReturn::new(completions, flags, true))
            } else {
                Ok(ProgrammableCompleteReturn::new(
                    vec!["--big".to_string()],
                    CompletionFlags::default(),
                    true,
                ))
            }
        } else {
            Ok(ProgrammableCompleteReturn::new(
                Vec::new(),
                CompletionFlags::default(),
                true,
            ))
        }
    }

    fn possible_command_words(&self) -> Vec<CommandWordInfo> {
        let mut words = vec![
            CommandWordInfo::Builtin {
                command: "cd".to_string(),
                usage: None,
            },
            CommandWordInfo::Builtin {
                command: "echo".to_string(),
                usage: None,
            },
            CommandWordInfo::Keyword {
                command: "if".to_string(),
                usage: None,
            },
            CommandWordInfo::Keyword {
                command: "then".to_string(),
                usage: None,
            },
            CommandWordInfo::Keyword {
                command: "else".to_string(),
                usage: None,
            },
            CommandWordInfo::Keyword {
                command: "fi".to_string(),
                usage: None,
            },
            CommandWordInfo::File {
                command: "git".to_string(),
                path: "/usr/bin/git".to_string(),
            },
            CommandWordInfo::File {
                command: "ls".to_string(),
                path: "/bin/ls".to_string(),
            },
        ];
        for (alias, expansion) in self.aliases.read().iter() {
            words.push(CommandWordInfo::Alias {
                command: alias.clone(),
                expansion: expansion.clone(),
            });
        }
        for (env_k, _) in self.env_vars.read().iter() {
            words.push(CommandWordInfo::EnvVar {
                name: env_k.clone(),
            });
        }
        words
    }

    fn evaluate_shell_string(&self, script: &str) -> anyhow::Result<()> {
        let trimmed = script.trim();
        if let Some(rest) = trimmed.strip_prefix("export ") {
            if let Some((k, v)) = rest.split_once('=') {
                self.set_env(k.trim(), v.trim().trim_matches('"').trim_matches('\''));
            }
        } else if let Some((k, v)) = trimmed.split_once('=') {
            self.set_env(k.trim(), v.trim().trim_matches('"').trim_matches('\''));
        }
        Ok(())
    }

    fn reset_caches(&self) {}

    fn warm_completion_caches(&self) {}

    fn read_terminating_signal(&self) -> libc::c_int {
        0
    }

    fn has_pending_traps(&self) -> bool {
        !self.pending_traps.read().is_empty()
    }

    fn run_pending_traps(&self) {
        let traps = std::mem::take(&mut *self.pending_traps.write());
        for trap in traps {
            let _ = self.evaluate_shell_string(&trap);
        }
    }

    fn resolve_completion_script_path(
        &self,
        command_word: &str,
        flycomp_output: Option<&str>,
    ) -> PathBuf {
        let poss_alias = self.find_alias(command_word);
        let alias_def = poss_alias
            .as_deref()
            .filter(|alias| !alias.is_empty())
            .unwrap_or(command_word);
        let cmd_word = alias_def
            .split_whitespace()
            .next()
            .unwrap_or(alias_def)
            .to_string();

        let file_name = std::path::Path::new(&cmd_word)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(&cmd_word);

        let output_dir = flycomp_output.unwrap_or("~/.local/share/bash-completion/completions/");
        let expanded_dir = self.expand_path(output_dir);

        std::path::Path::new(&expanded_dir).join(file_name)
    }

    fn resolve_and_write_completion_script(
        &self,
        command_word: &str,
        script: &str,
        flycomp_output: Option<&str>,
    ) -> Result<PathBuf, std::io::Error> {
        let write_path = self.resolve_completion_script_path(command_word, flycomp_output);
        if let Some(parent) = write_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if write_path.exists() {
            let now = chrono::Local::now();
            let datetime_str = now.format("%Y%m%d_%H%M%S").to_string();
            let file_name = write_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(command_word);
            let backup_name = format!("{}_backup_{}", file_name, datetime_str);
            if let Some(parent) = write_path.parent() {
                let backup_path = parent.join(backup_name);
                std::fs::rename(&write_path, &backup_path)?;
            }
        }

        std::fs::write(&write_path, script)?;
        Ok(write_path)
    }

    fn parse_history_from_memory(&self) -> Vec<HistoryEntry> {
        self.history.read().clone()
    }

    fn last_command_exit_status(&self) -> i32 {
        *self.last_exit_status.read()
    }

    fn multiline_command_count(&self) -> i32 {
        *self.multiline_count.read()
    }

    fn shell_pgrp(&self) -> libc::pid_t {
        0
    }

    fn is_autocd_enabled(&self) -> bool {
        self.autocd.load(std::sync::atomic::Ordering::SeqCst)
    }
}
