//! Host shell abstraction layer.
//!
//! `ShellBackend` provides a clean seam over shell-specific runtime operations,
//! decoupling the editor UI, completion engine, and prompt managers from direct
//! GNU Bash C FFI symbols.
//!
//! # Architecture & Future Shells (e.g. Zsh)
//!
//! By default, flyline operates with [`BashBackend`], which communicates directly
//! with GNU Bash's runtime when loaded as a dynamic builtin (`libflyline.so`).
//!
//! Downstream fork maintainers wishing to support additional shells (such as Zsh
//! via an out-of-process standalone editor or ZLE widget) can implement [`ShellBackend`]
//! for their shell (e.g. `pub mod zsh;` containing `ZshBackend`) and register it:
//!
//! ```ignore
//! // In downstream zsh implementation:
//! use crate::shell;
//!
//! if shell::is_zsh_host_env() {
//!     shell::set_backend(&zsh::ZSH_BACKEND);
//! }
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::history::HistoryEntry;

pub use crate::completions::{
    CompletionFlags, ProgrammableCompleteReturn, detect_and_convert_inline_descriptions,
};
pub use crate::grammar::{
    QuoteType, dequoting_function_rust, find_quote_type, quoting_function_rust,
};
pub use crate::path::{ExecutablesOnPath, PathScanPayload};

#[derive(Debug, PartialEq, Eq, Hash, Clone, serde::Serialize, serde::Deserialize)]
pub enum CommandWordInfo {
    Unknown {
        command: String,
    },
    Alias {
        command: String,
        expansion: String,
    },
    Keyword {
        command: String,
        usage: Option<String>,
    },
    Function {
        command: String,
        source_file: Option<String>,
        line: Option<i32>,
    },
    Builtin {
        command: String,
        usage: Option<String>,
    },
    File {
        command: String,
        path: String,
    },
}

impl CommandWordInfo {
    pub fn is_known(&self) -> bool {
        !matches!(self, CommandWordInfo::Unknown { .. })
    }

    pub fn command(&self) -> &str {
        match self {
            CommandWordInfo::Unknown { command } => command,
            CommandWordInfo::Alias { command, .. } => command,
            CommandWordInfo::Keyword { command, .. } => command,
            CommandWordInfo::Function { command, .. } => command,
            CommandWordInfo::Builtin { command, .. } => command,
            CommandWordInfo::File { command, .. } => command,
        }
    }

    pub fn to_description(&self) -> String {
        match self {
            CommandWordInfo::Unknown { .. } => "unknown".to_string(),
            CommandWordInfo::Alias { expansion, .. } => format!("alias: {}", expansion),
            CommandWordInfo::Keyword { command, usage } => {
                if let Some(u) = usage {
                    format!("keyword: {}", u)
                } else {
                    format!("keyword: {}", command)
                }
            }
            CommandWordInfo::Builtin { command, usage } => {
                if let Some(u) = usage {
                    format!("builtin: {}", u)
                } else {
                    format!("builtin: {}", command)
                }
            }
            CommandWordInfo::File { path, .. } => path.clone(),
            CommandWordInfo::Function {
                source_file, line, ..
            } => match (source_file, line) {
                (Some(file), Some(l)) => format!("function {}:{}", file, l),
                (Some(file), None) => format!("function {}", file),
                (None, Some(l)) => format!("function :{}", l),
                (None, None) => "function".to_string(),
            },
        }
    }
}

/// Shell builtin exit code constants (GNU Bash compatible).
#[repr(i32)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinExitCode {
    ExecutionSuccess = 0,
    BadSyntax = 257,    // shell syntax error
    Usage = 258,        // syntax error in usage
    RedirFail = 259,    // redirection failed
    BadAssign = 260,    // variable assignment error
    ExpFail = 261,      // word expansion failed
    DiskFallback = 262, // fall back to disk command from builtin
    UtilError = 263,    // Posix special builtin utility error
}

pub mod test_backend;
pub use test_backend::TestBackend;

/// The host shell that flyline is embedded in or driven by.
///
/// Implementors must be `Sync`: a single backend is shared by `&'static`
/// reference across flyline's threads.
pub trait ShellBackend: Sync {
    /// Human-readable identifier for the host shell (e.g. `"bash"`, `"test"`, `"zsh"`).
    fn name(&self) -> &'static str {
        "bash"
    }

    /// True when flyline is running as a Bash loadable builtin.
    fn is_bash(&self) -> bool {
        true
    }

    /// Working directory as the shell sees it (used for prompt + OSC 7 reporting).
    fn cwd(&self) -> String;

    /// Retrieve the value of an environment / shell variable.
    fn env_var(&self, name: &str) -> Option<String>;

    /// Set an exported environment variable.
    fn export_env_var(&self, name: &str, value: &str) -> anyhow::Result<()>;

    /// Unset an environment variable.
    fn unset_env_var(&self, name: &str) -> anyhow::Result<()>;

    /// Return the host machine's hostname as reported by the shell.
    fn hostname(&self) -> String;

    /// Format a variable assignment (e.g. `"VAR=val"` with quoting if needed).
    fn format_var(&self, name: &str) -> String;

    /// List all variable names matching the given prefix.
    fn vars_with_prefix(&self, prefix: &str) -> Vec<String>;

    /// Fully expand a shell path string (tilde expansion, subshell, vars, globs).
    fn expand_path(&self, path: &str) -> String;

    /// Expand a filename (tilde expansion, variable expansion).
    fn expand_filename(&self, filename: &str) -> String;

    /// Expand prompt escape sequences (`\u`, `\h`, `\w`, etc.).
    fn decode_prompt(&self, raw: &str, is_prompt: bool) -> Option<String>;

    /// Look up an alias definition by name.
    fn find_alias(&self, cmd: &str) -> Option<String>;

    /// Classify a command name (alias, keyword, function, builtin, file, unknown).
    fn command_info(&self, cmd: &str) -> CommandWordInfo;

    /// Run host programmable completions for a command line.
    fn run_programmable_completions(
        &self,
        full_command: &str,
        command_word: &str,
        word_under_cursor: &str,
        cursor_byte_pos: usize,
        word_under_cursor_byte_end: usize,
    ) -> anyhow::Result<ProgrammableCompleteReturn>;

    /// Return all known first-word command candidates.
    fn possible_command_words(&self) -> Vec<CommandWordInfo>;

    /// Evaluate an arbitrary shell script string inside the host shell context.
    fn evaluate_shell_string(&self, script: &str) -> anyhow::Result<()>;

    /// Invalidate any host shell caches (e.g. before prompt redraw).
    fn reset_caches(&self) {}

    /// Pre-warm completion and metadata caches in background/foreground.
    fn warm_completion_caches(&self) {}

    /// Read any pending terminating signal from host shell globals.
    fn read_terminating_signal(&self) -> libc::c_int {
        0
    }

    /// Resolve the filesystem path where generated completions should be written.
    fn resolve_completion_script_path(
        &self,
        command_word: &str,
        flycomp_output: Option<&str>,
    ) -> PathBuf {
        let file_name = std::path::Path::new(command_word)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(command_word);
        let output_dir = flycomp_output.unwrap_or("~/.local/share/bash-completion/completions/");
        let expanded = self.expand_path(output_dir);
        std::path::Path::new(&expanded).join(file_name)
    }

    /// Resolve output path and persist a newly generated completion script.
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

    /// Parse history entries currently loaded in shell memory.
    fn parse_history_from_memory(&self) -> Vec<HistoryEntry> {
        Vec::new()
    }

    /// Return the exit status of the last executed foreground command.
    fn last_command_exit_status(&self) -> i32 {
        0
    }

    /// Return current multiline command line count in shell parser.
    fn multiline_command_count(&self) -> i32 {
        0
    }

    /// Return host shell process group ID.
    fn shell_pgrp(&self) -> libc::pid_t {
        0
    }

    /// True if the shell has `autocd` enabled.
    fn is_autocd_enabled(&self) -> bool {
        false
    }

    /// Called when the line editor UI enters and prepares terminal mode.
    fn prep_terminal(&self) {}

    /// Called when the line editor UI exits and restores terminal mode.
    fn deprep_terminal(&self) {}
}

/// The GNU Bash host; delegates directly to `crate::bash_funcs` and `crate::bash_symbols`.
#[cfg(not(test))]
pub struct BashBackend;

#[cfg(not(test))]
impl ShellBackend for BashBackend {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn is_bash(&self) -> bool {
        true
    }

    fn cwd(&self) -> String {
        crate::bash_funcs::get_cwd()
    }

    fn env_var(&self, name: &str) -> Option<String> {
        crate::bash_funcs::get_envvar_value(name)
    }

    fn export_env_var(&self, name: &str, value: &str) -> anyhow::Result<()> {
        crate::bash_funcs::export_env_var(name, value)
    }

    fn unset_env_var(&self, name: &str) -> anyhow::Result<()> {
        crate::bash_funcs::unset_env_var(name)
    }

    fn hostname(&self) -> String {
        crate::bash_funcs::get_hostname()
    }

    fn format_var(&self, name: &str) -> String {
        crate::bash_funcs::format_shell_var(name)
    }

    fn vars_with_prefix(&self, prefix: &str) -> Vec<String> {
        crate::bash_funcs::get_all_variables_with_prefix(prefix)
    }

    fn expand_path(&self, path: &str) -> String {
        crate::bash_funcs::fully_expand_path(path)
    }

    fn expand_filename(&self, filename: &str) -> String {
        crate::bash_funcs::expand_filename(filename)
    }

    fn decode_prompt(&self, raw: &str, is_prompt: bool) -> Option<String> {
        bash_decode_prompt(raw, is_prompt)
    }

    fn find_alias(&self, cmd: &str) -> Option<String> {
        crate::bash_funcs::find_alias(cmd)
    }

    fn command_info(&self, cmd: &str) -> CommandWordInfo {
        crate::bash_funcs::get_command_info(cmd)
    }

    fn run_programmable_completions(
        &self,
        full_command: &str,
        command_word: &str,
        word_under_cursor: &str,
        cursor_byte_pos: usize,
        word_under_cursor_byte_end: usize,
    ) -> anyhow::Result<ProgrammableCompleteReturn> {
        crate::bash_funcs::run_programmable_completions(
            full_command,
            command_word,
            word_under_cursor,
            cursor_byte_pos,
            word_under_cursor_byte_end,
        )
    }

    fn possible_command_words(&self) -> Vec<CommandWordInfo> {
        crate::bash_funcs::get_possible_command_words().collect()
    }

    fn evaluate_shell_string(&self, script: &str) -> anyhow::Result<()> {
        crate::bash_funcs::evaluate_shell_string(script)
    }

    fn reset_caches(&self) {
        crate::bash_funcs::reset_caches();
    }

    fn warm_completion_caches(&self) {
        crate::bash_funcs::warm_bash_caches();
    }

    fn read_terminating_signal(&self) -> libc::c_int {
        crate::bash_funcs::read_terminating_signal()
    }

    fn resolve_completion_script_path(
        &self,
        command_word: &str,
        flycomp_output: Option<&str>,
    ) -> PathBuf {
        crate::bash_funcs::resolve_completion_script_path(command_word, flycomp_output)
    }

    fn resolve_and_write_completion_script(
        &self,
        command_word: &str,
        script: &str,
        flycomp_output: Option<&str>,
    ) -> Result<PathBuf, std::io::Error> {
        crate::bash_funcs::resolve_and_write_completion_script(command_word, script, flycomp_output)
    }

    fn parse_history_from_memory(&self) -> Vec<HistoryEntry> {
        crate::history::HistoryManager::parse_bash_history_from_memory()
    }

    fn last_command_exit_status(&self) -> i32 {
        read_last_command_exit_status()
    }

    fn multiline_command_count(&self) -> i32 {
        read_multiline_command_count()
    }

    fn shell_pgrp(&self) -> libc::pid_t {
        read_shell_pgrp()
    }

    fn is_autocd_enabled(&self) -> bool {
        crate::bash_funcs::is_autocd_enabled()
    }

    fn prep_terminal(&self) {
        #[cfg(not(test))]
        crate::bash_symbols::set_readline_state(crate::bash_symbols::RL_STATE_TERMPREPPED);
    }

    fn deprep_terminal(&self) {
        #[cfg(not(test))]
        crate::bash_symbols::clear_readline_state(crate::bash_symbols::RL_STATE_TERMPREPPED);
    }
}

#[cfg(not(test))]
fn bash_decode_prompt(raw: &str, is_prompt: bool) -> Option<String> {
    if raw.is_empty() {
        return Some(String::new());
    }

    let c_prompt = std::ffi::CString::new(raw).ok()?;
    let _guard = crate::bash_symbols::BASH_LOCK.lock();

    let decoded = unsafe {
        #[cfg(not(feature = "pre_bash_4_4"))]
        let decoded_prompt_cstr =
            crate::bash_symbols::decode_prompt_string(c_prompt.as_ptr(), is_prompt as i32);
        #[cfg(feature = "pre_bash_4_4")]
        let decoded_prompt_cstr = crate::bash_symbols::decode_prompt_string(c_prompt.as_ptr());
        if decoded_prompt_cstr.is_null() {
            log::warn!("decode_prompt_string returned null");
            return None;
        }

        let decoded = std::ffi::CStr::from_ptr(decoded_prompt_cstr)
            .to_str()
            .ok()?
            .to_string();

        crate::bash_symbols::locked_xfree(decoded_prompt_cstr as *mut std::ffi::c_void);
        decoded
    };

    Some(decoded)
}

#[cfg(test)]
fn bash_decode_prompt(raw: &str, _is_prompt: bool) -> Option<String> {
    if raw.is_empty() {
        Some(String::new())
    } else {
        Some(raw.to_string())
    }
}

#[cfg(not(test))]
fn read_last_command_exit_status() -> i32 {
    crate::bash_funcs::get_last_command_exit_value()
}

#[cfg(test)]
fn read_last_command_exit_status() -> i32 {
    0
}

#[cfg(not(test))]
fn read_multiline_command_count() -> i32 {
    unsafe { crate::bash_symbols::current_command_line_count }
}

#[cfg(test)]
fn read_multiline_command_count() -> i32 {
    0
}

#[cfg(not(test))]
fn read_shell_pgrp() -> libc::pid_t {
    unsafe { crate::bash_symbols::shell_pgrp }
}

#[cfg(test)]
fn read_shell_pgrp() -> libc::pid_t {
    0
}

#[cfg(not(test))]
static BASH: BashBackend = BashBackend;
static ACTIVE: OnceLock<&'static dyn ShellBackend> = OnceLock::new();

/// True when `FLYLINE_HOST=zsh` environment variable is set.
pub fn is_zsh_host_env() -> bool {
    std::env::var("FLYLINE_HOST").as_deref() == Ok("zsh")
}

#[cfg(not(test))]
fn init_backend() -> &'static dyn ShellBackend {
    &BASH
}

#[cfg(test)]
fn init_backend() -> &'static dyn ShellBackend {
    &*test_backend::TEST_BACKEND
}

/// The active host shell backend. Defaults to `BashBackend`.
pub fn backend() -> &'static dyn ShellBackend {
    *ACTIVE.get_or_init(init_backend)
}

/// Select the host backend once, at load, before the first `backend()` call.
#[allow(dead_code)]
pub fn set_backend(b: &'static dyn ShellBackend) {
    let _ = ACTIVE.set(b);
}

/// Helper for tests to toggle `autocd` behavior on the test/mock backend.
#[cfg(test)]
pub fn set_test_autocd(enabled: bool) {
    test_backend::TEST_BACKEND.set_autocd(enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_test_backend_identity() {
        assert_eq!(backend().name(), "test");
        assert!(!backend().is_bash());
    }

    #[test]
    fn default_test_backend_defaults() {
        assert_eq!(backend().hostname(), "test-host");
        assert_eq!(
            backend().cwd(),
            std::env::current_dir().unwrap().to_string_lossy()
        );
        assert_eq!(backend().env_var("USER"), Some("john".to_string()));
        assert_eq!(backend().env_var("FLYLINE_DEFINITELY_UNSET_VAR"), None);
    }

    #[test]
    fn default_test_backend_mock_functionality() {
        let tb = TestBackend::new();
        assert_eq!(tb.name(), "test");
        assert!(!tb.is_bash());
        assert_eq!(tb.hostname(), "test-host");
        assert_eq!(tb.env_var("USER"), Some("john".to_string()));

        tb.set_env("FOO", "BAR");
        assert_eq!(tb.env_var("FOO"), Some("BAR".to_string()));
        assert_eq!(tb.format_var("FOO"), "FOO=BAR");

        tb.set_alias("ll", "ls -la");
        assert_eq!(tb.find_alias("ll"), Some("ls -la".to_string()));
        assert!(matches!(
            tb.command_info("ll"),
            CommandWordInfo::Alias { .. }
        ));

        assert_eq!(tb.expand_path("~/foo"), "/home/john/foo");
    }
}
