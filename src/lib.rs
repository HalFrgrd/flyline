#![cfg_attr(test, expect(dead_code))]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub const FILENAME_INFERENCE_LIMIT: usize = 5000;

/// Writes formatted arguments to stdout and flushes immediately.
#[macro_export]
macro_rules! flush_stdout {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let mut stdout = std::io::stdout();
        write!(stdout, $($arg)*).and_then(|_| stdout.flush())
    }};
}

#[macro_use]
pub(crate) mod perf;
mod active_suggestions;
mod agent_mode;
mod app;
#[cfg(not(test))]
mod bash_builtin;
mod changelog;
mod cli;
pub mod completions;
pub mod tag;
pub mod content {
    pub use crate::tag::*;
    pub use flycontent::builder::{Coord, RelativePosition, reset_matrix_anim_state};
    pub use flycontent::easing;
    pub use flycontent::palette;
    pub use flycontent::sliding_window::*;
    pub use flycontent::snake_animation::*;
    pub use flycontent::table::*;
    pub use flycontent::unicode::*;
    pub use flycontent::utils::*;
}
pub use flycontent::palette;
mod cursor;
pub mod git;
mod globbing;
pub mod grammar;
mod history;
pub mod hostnames;
mod iter_first_last;
mod kill_on_drop_child;
mod logging;
mod mouse_state;
pub mod path;
mod prompt_manager;
mod settings;
pub(crate) use settings::settings;
pub mod shell;
mod shell_integration;
pub(crate) mod subshell_ipc;
pub mod term_info;
mod tutorial;
mod users;

pub use tag::{ClipboardTypes, Contents, Tag, TaggedCell, TaggedLine, TaggedSpan};

pub use completions::context as tab_completion_context;
pub use grammar::command_acceptance;
pub use grammar::dparser;
pub use grammar::lexer;

#[derive(Debug, Default)]
pub struct LongLived {
    pub history_manager: crate::history::HistoryManager,
    pub cancelled_command_history_manager: crate::history::HistoryManager,
    pub agent_prompt_history_manager: crate::history::HistoryManager,
}

/// Resets `SIGCHLD` disposition to `SIG_DFL` (default action) using `sigaction(2)`.
///
/// Bash frequently installs its own `SIGCHLD` handler (e.g. during prompt expansion
/// or command substitution execution), which interferes with process spawning in Rust
/// (`std::process::Command`), causing child wait calls to fail with `ECHILD`.
///
/// Using `sigaction` with `SIG_DFL` ensures `SA_NOCLDWAIT` and custom signal handlers are cleared.
pub fn reset_sigchld() {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGCHLD, &action, std::ptr::null_mut());
    }
}

/// An RAII guard that sets `SIGCHLD` to `SIG_DFL` upon creation, and restores
/// the previous signal disposition when dropped (even on panic or early return).
#[must_use]
pub struct SigchldGuard {
    prev_action: libc::sigaction,
}

impl SigchldGuard {
    /// Resets `SIGCHLD` to `SIG_DFL` and returns a guard that will restore
    /// the previous `SIGCHLD` disposition when dropped.
    pub fn new() -> Self {
        unsafe {
            let mut prev_action: libc::sigaction = std::mem::zeroed();
            let mut new_action: libc::sigaction = std::mem::zeroed();
            new_action.sa_sigaction = libc::SIG_DFL;
            libc::sigaction(libc::SIGCHLD, &new_action, &mut prev_action);
            Self { prev_action }
        }
    }
}

impl Default for SigchldGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SigchldGuard {
    fn drop(&mut self) {
        unsafe {
            libc::sigaction(libc::SIGCHLD, &self.prev_action, std::ptr::null_mut());
        }
    }
}
