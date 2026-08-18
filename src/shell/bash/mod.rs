pub mod funcs;
pub mod symbols;

use std::path::PathBuf;

use crate::history::HistoryEntry;
use crate::shell::{CommandWordInfo, ProgrammableCompleteReturn, ShellBackend};

/// The GNU Bash host; delegates directly to `funcs` and `symbols`.
pub struct BashBackend;

impl ShellBackend for BashBackend {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn cwd(&self) -> String {
        funcs::get_cwd()
    }

    fn env_var(&self, name: &str) -> Option<String> {
        funcs::get_envvar_value(name)
    }

    fn export_env_var(&self, name: &str, value: &str) -> anyhow::Result<()> {
        funcs::export_env_var(name, value)
    }

    fn unset_env_var(&self, name: &str) -> anyhow::Result<()> {
        funcs::unset_env_var(name)
    }

    fn hostname(&self) -> String {
        funcs::get_hostname()
    }

    fn format_var(&self, name: &str) -> String {
        funcs::format_shell_var(name)
    }

    fn vars_with_prefix(&self, prefix: &str) -> Vec<String> {
        funcs::get_all_variables_with_prefix(prefix)
    }

    fn expand_path(&self, path: &str) -> String {
        funcs::fully_expand_path(path)
    }

    fn expand_filename(&self, filename: &str) -> String {
        funcs::expand_filename(filename)
    }

    fn decode_prompt(&self, raw: &str, _is_prompt: bool) -> Option<String> {
        if raw.is_empty() {
            return Some(String::new());
        }

        let c_prompt = std::ffi::CString::new(raw).ok()?;
        let _guard = symbols::BASH_LOCK.lock();

        let decoded = unsafe {
            #[cfg(not(feature = "pre_bash_4_4"))]
            let decoded_prompt_cstr =
                symbols::decode_prompt_string(c_prompt.as_ptr(), _is_prompt as i32);
            #[cfg(feature = "pre_bash_4_4")]
            let decoded_prompt_cstr = symbols::decode_prompt_string(c_prompt.as_ptr());
            if decoded_prompt_cstr.is_null() {
                log::warn!("decode_prompt_string returned null");
                return None;
            }

            let decoded = std::ffi::CStr::from_ptr(decoded_prompt_cstr)
                .to_str()
                .ok()?
                .to_string();

            symbols::locked_xfree(decoded_prompt_cstr as *mut std::ffi::c_void);
            decoded
        };

        Some(decoded)
    }

    fn find_alias(&self, cmd: &str) -> Option<String> {
        funcs::find_alias(cmd)
    }

    fn command_info(&self, cmd: &str) -> CommandWordInfo {
        funcs::get_command_info(cmd)
    }

    fn run_programmable_completions(
        &self,
        full_command: &str,
        command_word: &str,
        word_under_cursor: &str,
        cursor_byte_pos: usize,
        word_under_cursor_byte_end: usize,
    ) -> anyhow::Result<ProgrammableCompleteReturn> {
        funcs::run_programmable_completions(
            full_command,
            command_word,
            word_under_cursor,
            cursor_byte_pos,
            word_under_cursor_byte_end,
        )
    }

    fn possible_command_words(&self) -> Vec<CommandWordInfo> {
        funcs::get_possible_command_words().collect()
    }

    fn evaluate_shell_string(&self, script: &str) -> anyhow::Result<()> {
        funcs::evaluate_shell_string(script)
    }

    fn reset_caches(&self) {
        funcs::reset_caches();
    }

    fn warm_completion_caches(&self) {
        funcs::warm_bash_caches();
    }

    fn read_terminating_signal(&self) -> libc::c_int {
        funcs::read_terminating_signal()
    }

    fn has_pending_traps(&self) -> bool {
        funcs::has_pending_traps()
    }

    fn run_pending_traps(&self) {
        funcs::run_pending_traps();
    }

    fn resolve_completion_script_path(
        &self,
        command_word: &str,
        flycomp_output: Option<&str>,
    ) -> PathBuf {
        funcs::resolve_completion_script_path(command_word, flycomp_output)
    }

    fn resolve_and_write_completion_script(
        &self,
        command_word: &str,
        script: &str,
        flycomp_output: Option<&str>,
    ) -> Result<PathBuf, std::io::Error> {
        funcs::resolve_and_write_completion_script(command_word, script, flycomp_output)
    }

    fn parse_history_from_memory(&self) -> Vec<HistoryEntry> {
        crate::history::HistoryManager::parse_bash_history_from_memory()
    }

    fn last_command_exit_status(&self) -> i32 {
        funcs::get_last_command_exit_value()
    }

    fn multiline_command_count(&self) -> i32 {
        unsafe { symbols::current_command_line_count }
    }

    fn shell_pgrp(&self) -> libc::pid_t {
        unsafe { symbols::shell_pgrp }
    }

    fn is_autocd_enabled(&self) -> bool {
        funcs::is_autocd_enabled()
    }

    fn prep_terminal(&self) {
        symbols::set_readline_state(symbols::RL_STATE_TERMPREPPED);
    }

    fn deprep_terminal(&self) {
        symbols::clear_readline_state(symbols::RL_STATE_TERMPREPPED);
    }
}
