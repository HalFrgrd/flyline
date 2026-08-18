pub(crate) mod actions;
pub(crate) mod auto_close;
pub(crate) mod formatted_buffer;
mod ui;
use crate::subshell_ipc::{self, IpcStatus, SubshellHandle};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
pub(crate) use ui::DrawnContent;

#[derive(Debug, Clone)]
pub struct LastKeyPress {
    pub key: KeyEvent,
    pub display: String,
    pub context: String,
    pub actions: Vec<KeyEventAction>,
    pub sequence_number: u64,
}

#[derive(Debug, Clone)]
pub struct LastMouseEvent {
    pub mouse: MouseEvent,
    pub matches: Vec<(String, String)>,
    pub time: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RightClickCopyTarget {
    Selection(String),
    Buffer(String),
    HistoryEntry(String, Option<crate::history::HistoryEntry>),
    Cwd(String),
    Suggestion(String),
    AiResult(String),
    Clipboard(String),
}

use crate::LongLived;
use crate::active_suggestions::{ActiveSuggestions, ActiveSuggestionsBuilder, COLUMN_PADDING};
use crate::agent_mode::{AiOutputSelection, parse_ai_output};
use crate::app::actions::KeyEventAction;
use crate::app::formatted_buffer::{FormattedBuffer, format_agent_buffer, format_buffer};
use crate::content::{Contents, Coord, SpanTag, Tag, TaggedLine, TaggedSpan};
use crate::cursor::{Cursor, CursorBackend};
use crate::dparser::{AnnotatedToken, ToInclusiveRange};
use crate::history::{HistoryEntry, HistoryEntryFormatted, HistoryManager};
use crate::iter_first_last::FirstLast;
use crate::kill_on_drop_child::KillOnDropChild;
use crate::mouse_state::{MouseState, mouse_state};
use crate::palette::{ButtonState, Palette};
use crate::prompt_manager::PromptManager;
use crate::settings::{self, MatrixAnimation, MouseMode};
use crate::shell_integration;
use crate::{command_acceptance, dparser};
use crate::{shell, tab_completion_context};
use flybuffer::{SubString, TextBuffer};

use flash::lexer::TokenKind;
use itertools::Itertools;
use ratatui::prelude::*;
use ratatui::text::StyledGrapheme;
use ratatui::{TerminalOptions, Viewport};
use std::boxed::Box;
use std::io::{Error, ErrorKind, IsTerminal};
use std::time::Duration;
use std::vec;
use termina::escape::csi::{
    Csi, Cursor as CsiCursor, DecPrivateMode, DecPrivateModeCode, Keyboard, KittyKeyboardFlags,
    Mode as DecMode,
};
use termina::event::{
    KeyCode, KeyEvent, Modifiers as KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use termina::{Event as TerminaEvent, Terminal};

use std::io::Write;
use std::sync::LazyLock;

/// The reason for the global event reader is that it often buffers events
/// and if we drop it, those buffered events are lost.
/// This is apparent when you type `sleep 5\necho foo\necho bar\n`.
pub static GLOBAL_EVENT_READER: LazyLock<termina::EventReader> = LazyLock::new(|| {
    let temp_terminal = termina::PlatformTerminal::new().unwrap();
    temp_terminal.event_reader()
});

/// After this duration of inactivity the frame rate drops to `idle_frame_rate` and the
/// cursor is rendered in the unfocused (dim, non-animated) state.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

fn restore_terminal(write: &mut impl std::io::Write) {
    let reset = |code| Csi::Mode(DecMode::ResetDecPrivateMode(DecPrivateMode::Code(code)));
    let _ = write!(
        write,
        "{}{}{}",
        reset(DecPrivateModeCode::BracketedPaste),
        reset(DecPrivateModeCode::FocusTracking),
        Csi::Keyboard(Keyboard::PopFlags(1))
    );
    mouse_state(|m| m.disable());
    let _ = write.flush();
}

fn configure_terminal(extended_key_codes: bool, mouse_mode: &crate::settings::MouseMode) {
    let set_mode = |code| Csi::Mode(DecMode::SetDecPrivateMode(DecPrivateMode::Code(code)));

    let flags = if extended_key_codes {
        // Enabling REPORT_ALL_KEYS_AS_ESCAPE_CODES causes Ctrl+C to not copy to clipboard in VS Code with default settings
        // because it causes the press of Ctrl to be sent as a key code thus clearing the selection before 'c' is pressed.
        // https://blog.fsck.com/releases/2026/02/26/terminal-keyboard-protocol/ is a good reference for understanding the terminal key code problem.
        KittyKeyboardFlags::DISAMBIGUATE_ESCAPE_CODES | KittyKeyboardFlags::REPORT_ALTERNATE_KEYS
    } else {
        KittyKeyboardFlags::empty()
    };
    let _ = crate::flush_stdout!(
        "{}{}",
        set_mode(DecPrivateModeCode::BracketedPaste),
        Csi::Keyboard(Keyboard::PushFlags(flags))
    );

    MouseState::enable_mode(mouse_mode);
}

fn stdin_unavailable_reason() -> Option<&'static str> {
    // I was finding bash processes were often spinning trying to read from stdin
    // When the terminal emulator closed.
    // I believe this problem was fixed by setting `use-dev-tty` in crossterm.
    // The following are defensive checks to avoid calling crossterm poll when the terminal closes.

    // If stdin has been closed outright, bail out before crossterm enters its
    // Unix event loop. In crossterm 0.29 that path can spin on closed input.
    if unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFD) } == -1
        && Error::last_os_error().raw_os_error() == Some(libc::EBADF)
    {
        return Some("stdin file descriptor is closed");
    }

    if !std::io::stdin().is_terminal() {
        return Some("stdin is no longer attached to a terminal");
    }

    // On macOS, when the terminal emulator closes its end of the PTY (the
    // master), the slave PTY fd (stdin) remains valid and isatty() continues to
    // return true.  The is_terminal() check above therefore does NOT fire on
    // macOS after the terminal is closed, so crossterm is called even though
    // input is gone.
    //
    // Crossterm uses mio, which on macOS uses kqueue.  kqueue registers
    // EVFILT_READ on the slave PTY fd.  After the PTY master closes, kqueue
    // immediately marks the fd readable (it reports the EOF/hangup condition as
    // a read-ready event).  Crossterm's inner read loop then calls read(2),
    // which returns 0 bytes (macOS PTY slave returns EOF rather than EIO when
    // the master closes).  Since read_count == 0 is not treated as WouldBlock,
    // the loop never breaks and spins at 100% CPU indefinitely.
    //
    // On Linux the is_terminal() guard above is sufficient: after the PTY
    // master closes, Linux updates the slave's session state so that isatty()
    // returns 0, which is caught above.  If isatty() somehow still returns 1,
    // Linux's epoll reports EPOLLHUP without EPOLLIN, so mio's poll times out
    // normally.  And when Linux read() does return EIO the same inner-loop
    // fall-through occurs, but the earlier guards prevent reaching that point.
    //
    // The reliable cross-platform guard is to call poll(2) with a zero timeout
    // and check for POLLHUP.  POSIX guarantees that POLLHUP is set in revents
    // whenever a hang-up has occurred on the fd, regardless of which events
    // were requested.  A set POLLHUP therefore means we should not call
    // crossterm.
    let mut pfd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: pfd is a valid, stack-allocated pollfd.  Passing its address with
    // nfds=1 and timeout=0 is a standard non-blocking poll probe.
    // poll(2) returns -1 on error, 0 on timeout, or a positive count of ready
    // fds.  We check > 0 so that we only inspect revents when poll actually
    // reported an event; a return of 0 (timeout) leaves revents as 0.
    if unsafe { libc::poll(&raw mut pfd, 1, 0) } > 0 && (pfd.revents & libc::POLLHUP) != 0 {
        return Some("stdin PTY has hung up (POLLHUP)");
    }

    None
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ExitState {
    WithCommand(String),
    WithoutCommand,
    EOF,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) enum AppRunningState {
    Running,
    Exiting(ExitState),
}

impl AppRunningState {
    pub fn is_running(&self) -> bool {
        *self == AppRunningState::Running
    }
}

pub fn get_command(long_lived: &mut LongLived) -> ExitState {
    // If stdin is closed, bash expects us to just return EOF a few times
    if let Some(reason) = stdin_unavailable_reason() {
        log::error!(
            "Standard input is not available: {}. Exiting without command.",
            reason
        );

        return ExitState::EOF;
    }

    let app = time_it!("startup: app creation", App::new(long_lived));

    let end_state = app.run();

    restore_terminal(&mut std::io::stdout());

    log::debug!("Final state: {:?}", end_state);
    end_state
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FuzzyHistorySource {
    PastCommands,
    CancelledCommands,
    AgentPrompts,
}

impl FuzzyHistorySource {
    fn label(&self) -> &'static str {
        match self {
            FuzzyHistorySource::PastCommands => "Fuzzy search",
            FuzzyHistorySource::CancelledCommands => "Cancelled commands",
            FuzzyHistorySource::AgentPrompts => "Agent prompts",
        }
    }
}

/// Guard that owns the tab-completion background process and the result channel.
/// Killing the process (on drop) ensures it does not outlive the app.
pub(crate) type TabCompletionPayload = Option<(ActiveSuggestionsBuilder, std::time::Duration)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum FlycompPromptSelection {
    Yes,
    No,
    DontAsk,
}

#[derive(Debug)]
pub(crate) enum ContentMode {
    Normal,
    FuzzyHistorySearch(FuzzyHistorySource),
    TabCompletion(Box<ActiveSuggestions>),
    /// Tab completion is running in a background thread.  The handle owns both
    /// the result channel receiver and the thread join-handle so that cleanup
    /// happens automatically when the mode transitions.
    TabCompletionWaiting {
        handle: SubshellHandle<TabCompletionPayload>,
        wuc_substring: SubString,
        start_time: std::time::Instant,
        auto_started: bool,
        last_active_suggestions: Option<Box<ActiveSuggestions>>,
    },
    /// AI command is running as a child process.  The child is polled each
    /// event-loop iteration with `try_wait`; on drop it is killed and reaped.
    AgentModeWaiting {
        child: KillOnDropChild,
        command_display: String,
        start_time: std::time::Instant,
    },
    /// AI output has been parsed; user is selecting a suggestion from the list.
    AgentOutputSelection(AiOutputSelection),
    /// AI command or JSON parsing failed; stores the error message and any raw output.
    /// When `suggested_setup_command` is set, an agent from the example file was found on PATH;
    /// pressing Enter will run that `flyline set-agent-mode ...` command to configure it.
    AgentError {
        message: String,
        raw_output: String,
        suggested_setup_command: Option<String>,
    },
    /// User is navigating the CWD path segments displayed in the prompt.
    /// The inner value is the currently highlighted segment index (0 = rightmost/current dir).
    PromptDirSelect(usize),
    TabCompletionAskForFlycomp {
        command_word: String,
        word_under_cursor: String,
        selection: FlycompPromptSelection,
        dump_path: String,
        forced: bool,
    },
    TabCompletionRunningFlycomp {
        command_word: String,
        _word_under_cursor: String,
        start_time: std::time::Instant,
        handle: SubshellHandle<String>,
    },
    TabCompletionFlycompResult {
        command_word: String,
        error_message: String,
    },
}

pub(crate) struct App<'a> {
    pub(super) long_lived: &'a mut LongLived,
    pub(super) terminal:
        ratatui::Terminal<ratatui::backend::TerminaBackend<termina::PlatformTerminal>>,
    pub(super) mode: AppRunningState,
    pub(super) buffer: TextBuffer,
    pub(super) formatted_buffer_cache: FormattedBuffer,
    /// Cached annotated tokens from the last dparser run, including `is_auto_inserted` flags.
    pub(super) dparser_tokens_cache: Vec<AnnotatedToken>,
    pub(super) cursor: Cursor,
    /// Whether the terminal currently has focus. Used to control cursor animation intensity.
    pub(super) term_has_focus: bool,
    pub(super) unfinished_from_prev_command: bool,
    pub(super) prompt_manager: PromptManager,
    pub(super) buffer_before_history_navigation: Option<String>,
    pub(super) inline_history_suggestion: Option<(HistoryEntry, String)>,
    /// Buffer contents at the time the user last dismissed the inline suggestion.
    /// While the buffer equals this value the suggestion is suppressed.
    pub(super) dismissed_inline_suggestion_buffer: Option<String>,
    /// Word-under-cursor at the time the user dismissed tab completion with Escape.
    /// While the new word-under-cursor equals this value, auto-suggest is suppressed.
    pub(super) dismissed_tab_completion_wuc: Option<String>,
    /// Buffer contents at the time the user last dismissed the agent prompts fuzzy history search.
    pub(super) dismissed_agent_mode_buffer: Option<String>,
    pub(super) content_mode: ContentMode,
    pub(super) last_contents: Option<DrawnContent>,
    pub(super) tooltip: Option<String>,
    /// Terminal row (absolute) where the inline viewport starts; used by smart mouse mode.
    /// Timestamp of the last draw operation.
    pub(super) last_draw_time: std::time::Instant,
    pub(super) needs_screen_cleared: bool,
    pub(super) needs_full_redraw: bool,
    /// Last key event, context expression, and action dispatched.
    pub(super) last_key: Option<LastKeyPress>,
    /// Last mouse event received.
    pub(super) last_mouse: Option<LastMouseEvent>,
    /// Last processed key event sequence number for triggers.
    pub(super) last_processed_key_sequence: u64,
    /// Position of the right click popup, if active.
    pub(super) right_click_popup_pos: Option<Coord>,
    /// Target content to copy/cut determined at right-click depress time.
    pub(super) right_click_copy_target: Option<RightClickCopyTarget>,
    pub(super) last_activity_time: std::time::Instant,
    pub(super) leader_key_active_at: Option<std::time::Instant>,
    pub(super) app_start_time: std::time::Instant,
    pub(super) has_run_delayed_startup: bool,
    pub(super) last_resize_time: Option<std::time::Instant>,
    pub(super) path_warming_subshell: Option<SubshellHandle<shell::PathScanPayload>>,
    pub(super) git_warming_subshell: Option<SubshellHandle<Option<crate::git::GitRepoPayload>>>,
}

impl<'a> App<'a> {
    fn new(long_lived: &'a mut LongLived) -> Self {
        let settings = crate::settings();
        long_lived
            .history_manager
            .set_jsonl_history_path(settings.history_jsonl_path.clone());
        let unfinished_from_prev_command = shell::backend().multiline_command_count() > 0;
        let initial_buf_val = settings.initial_buffer.take().unwrap_or_default();
        let buffer = TextBuffer::new(&initial_buf_val);
        let formatted_buffer_cache = FormattedBuffer::default();

        time_it!("startup: reload history", {
            match settings.history_backend {
                crate::settings::HistoryBackend::Bash => {
                    let zsh_history_path = settings.zsh_history_path.clone();
                    long_lived
                        .history_manager
                        .reload_from_bash_history(zsh_history_path.as_deref());
                }
                crate::settings::HistoryBackend::Flyline => {
                    // We dont refresh it here often so that when we press Up
                    // we search through the history entires from this session
                    if long_lived.history_manager.is_empty() {
                        long_lived.history_manager.refresh_jsonl_backend();
                    }
                    long_lived.history_manager.reset_navigation();
                }
            }
        });

        shell::backend().reset_caches();

        time_it!("startup: warm bash caches", {
            shell::backend().warm_completion_caches();
        });

        let path_env = shell::backend().env_var("PATH");
        let path_warming_subshell = subshell_ipc::spawn_subshell(move || {
            Some(shell::ExecutablesOnPath::scan_path_updates(path_env))
        });

        let git_warming_subshell = if settings.git_ref_mtime {
            let cwd_str = shell::backend().cwd();
            log::info!("Spawning background git warming subshell for {:?}", cwd_str);
            subshell_ipc::spawn_subshell(move || {
                if cwd_str.is_empty() {
                    None
                } else {
                    Some(crate::git::scan_git_repo_payload(std::path::Path::new(
                        &cwd_str,
                    )))
                }
            })
        } else {
            None
        };

        let mut terminal = time_it!("startup: terminal setup", {
            let event_reader = GLOBAL_EVENT_READER.clone();
            let mut platform_terminal =
                termina::PlatformTerminal::with_reader(event_reader).unwrap();
            platform_terminal.enter_raw_mode().unwrap();
            platform_terminal.set_panic_hook(|write| restore_terminal(write));
            configure_terminal(settings.enable_extended_key_codes, &settings.mouse_mode);

            let backend = ratatui::backend::TerminaBackend::new(platform_terminal);
            use ratatui::backend::Backend;
            if let Some(width) = backend.size().ok().map(|s| s.width as usize) {
                let (style, reset) = if !termina::style::Stylized::is_ansi_color_disabled() {
                    use termina::escape::csi::{Csi, Sgr, SgrAttributes, SgrModifiers};
                    use termina::style::ColorSpec;
                    (
                        Csi::Sgr(Sgr::Attributes(SgrAttributes {
                            modifiers: SgrModifiers::INTENSITY_BOLD,
                            foreground: Some(ColorSpec::RED),
                            ..Default::default()
                        }))
                        .to_string(),
                        Csi::Sgr(Sgr::Reset).to_string(),
                    )
                } else {
                    (String::new(), String::new())
                };

                use termina::escape::csi::{Csi, Edit, EraseInLine};
                let clear_to_eol =
                    Csi::Edit(Edit::EraseInLine(EraseInLine::EraseToEndOfLine)).to_string();

                const TAG: &str = "[flyline inserted newline]";
                let num_spaces = width.saturating_sub(TAG.len());
                let spaces = " ".repeat(num_spaces);

                let _ =
                    crate::flush_stdout!("{}{}{}{}\r{}", style, TAG, reset, spaces, clear_to_eol);
            }

            ratatui::Terminal::with_options(
                backend,
                TerminalOptions {
                    viewport: Viewport::Inline(0),
                },
            )
            .expect("Failed to create terminal")
        });

        // clear from cursor pos to end of terminal
        terminal.clear().unwrap_or_else(|e| {
            log::error!("Failed to clear terminal on startup: {}", e);
        });

        let mut app = App {
            long_lived,
            terminal,
            mode: AppRunningState::Running,
            buffer,
            formatted_buffer_cache,
            dparser_tokens_cache: Vec::new(),
            cursor: Cursor::new(),
            term_has_focus: true,
            unfinished_from_prev_command,
            prompt_manager: time_it!(
                "startup: prompt manager",
                PromptManager::new(
                    unfinished_from_prev_command,
                    &settings
                        .custom_animations
                        .values()
                        .cloned()
                        .collect::<Vec<_>>(),
                    &settings
                        .custom_prompt_widgets
                        .values()
                        .cloned()
                        .collect::<Vec<_>>(),
                    settings.last_app_closed_at,
                )
            ),
            buffer_before_history_navigation: None,
            inline_history_suggestion: None,
            dismissed_inline_suggestion_buffer: None,
            dismissed_tab_completion_wuc: None,
            dismissed_agent_mode_buffer: None,
            content_mode: ContentMode::Normal,
            last_contents: None,
            tooltip: None,
            last_draw_time: std::time::Instant::now(),
            needs_screen_cleared: false,
            needs_full_redraw: false,
            last_key: None,
            last_mouse: None,
            last_processed_key_sequence: 0,
            right_click_popup_pos: None,
            right_click_copy_target: None,
            last_activity_time: std::time::Instant::now(),
            leader_key_active_at: None,
            app_start_time: std::time::Instant::now(),
            has_run_delayed_startup: false,
            last_resize_time: None,
            path_warming_subshell,
            git_warming_subshell,
        };

        app.on_possible_buffer_change();
        app
    }

    fn sync_viewport_top_from_cpr(&mut self) {
        let req_csi = Csi::Cursor(CsiCursor::RequestActivePositionReport);
        let _ = crate::flush_stdout!("{req_csi}");

        let is_apr_event = |event: &TerminaEvent| {
            matches!(
                event,
                TerminaEvent::Csi(Csi::Cursor(CsiCursor::ActivePositionReport { .. }))
            )
        };

        match GLOBAL_EVENT_READER.poll(Some(Duration::from_millis(500)), is_apr_event) {
            Ok(true) => match GLOBAL_EVENT_READER.read(is_apr_event) {
                Ok(TerminaEvent::Csi(Csi::Cursor(CsiCursor::ActivePositionReport {
                    line,
                    ..
                }))) => {
                    let abs_row = line.get_zero_based();
                    let top = abs_row.saturating_sub(self.terminal.inline_cursor_y());
                    self.terminal.set_viewport_top(top);
                    if let Some(ref mut drawn) = self.last_contents {
                        drawn.viewport_start = Some(top);
                    }
                }
                Ok(other) => {
                    log::error!(
                        "Failed to receive CPR response: unexpected event {:?}",
                        other
                    );
                }
                Err(e) => {
                    log::error!("Failed to read CPR response: {}", e);
                }
            },
            Ok(false) => {
                log::error!("Timed out waiting for CPR response (500ms)");
            }
            Err(e) => {
                log::error!("Failed polling for CPR response: {}", e);
            }
        }
    }

    /// Computes the number of terminal rows occupied by the lines up to the cursor
    /// after a window resize to `new_width` columns, based on `resize_logic`.
    pub fn compute_wrapped_rows_up(
        &mut self,
        new_width: u16,
        resize_logic: settings::ResizeLogic,
    ) -> u16 {
        let inline_cursor_y = self.terminal.inline_cursor_y();
        let inline_cursor_x = self.terminal.inline_cursor_x();
        let buffer = self.terminal.previous_buffer_mut();

        Self::compute_wrapped_rows_up_from_buffer(
            buffer,
            inline_cursor_x,
            inline_cursor_y,
            new_width,
            resize_logic,
        )
    }

    /// Computes the display width of line `y` in `buffer` by finding the rightmost non-empty cell.
    #[allow(dead_code)]
    pub fn compute_line_width_from_buffer(buffer: &ratatui::buffer::Buffer, y: u16) -> u16 {
        Self::compute_line_width_from_buffer_opts(buffer, y, false)
    }

    /// Computes the display width of line `y` in `buffer`, optionally trimming trailing whitespace.
    pub fn compute_line_width_from_buffer_opts(
        buffer: &ratatui::buffer::Buffer,
        y: u16,
        trim_whitespace: bool,
    ) -> u16 {
        let old_width = buffer.area.width;
        let mut line_width = 0u16;
        for x in (0..old_width).rev() {
            if let Some(cell) = buffer.cell(ratatui::layout::Position { x, y }) {
                let is_empty = cell.symbol_opt().map_or(true, |s| {
                    if trim_whitespace {
                        s.trim().is_empty()
                    } else {
                        s.is_empty()
                    }
                });
                if !is_empty {
                    log::info!(
                        "[LineWidth] Line {} (old_width={}, trim_ws={}) rightmost non-empty cell at x={} with symbol {:?}",
                        y,
                        old_width,
                        trim_whitespace,
                        x,
                        cell.symbol_opt()
                    );
                    let sym_width = cell
                        .symbol_opt()
                        .map(|s| unicode_width::UnicodeWidthStr::width(s) as u16)
                        .unwrap_or(1);
                    line_width = (x + sym_width.max(1)).min(old_width);
                    break;
                }
            }
        }
        line_width
    }

    /// Computes the number of terminal rows occupied by the lines up to the cursor
    /// after a window resize to `new_width` columns, based on `resize_logic`.
    pub fn compute_wrapped_rows_up_from_buffer(
        buffer: &ratatui::buffer::Buffer,
        inline_cursor_x: u16,
        inline_cursor_y: u16,
        new_width: u16,
        resize_logic: settings::ResizeLogic,
    ) -> u16 {
        if new_width == 0 {
            return 0;
        }

        if resize_logic == settings::ResizeLogic::DontMoveCursor {
            return 0;
        }

        if resize_logic == settings::ResizeLogic::AutoCleared {
            // ghostty seems to clear the text we wrote but it doesnt move the cursor back to the start.
            // Because the text was cleared, it doesnt wrap.
            return inline_cursor_y;
        }

        let trim_whitespace = resize_logic == settings::ResizeLogic::ReflowedAllWhitespaceTrimmed;

        let mut total_rows = 0u16;

        // Calculate rows for each line above the cursor (y < inline_cursor_y)
        for y in 0..inline_cursor_y {
            let line_width = Self::compute_line_width_from_buffer_opts(buffer, y, trim_whitespace);

            log::info!(
                "Line {} width before resize: {}, new width: {}",
                y,
                line_width,
                new_width
            );

            if line_width == 0 {
                total_rows += 1;
            } else {
                let rows = (line_width + new_width - 1) / new_width;
                total_rows += rows.max(1);
            }
        }

        // Add offset for the cursor line itself and lines below the cursor according to the resize strategy
        match resize_logic {
            settings::ResizeLogic::ReflowedApartFromCursor => {
                // Account for extra wrapped rows generated by lines below the cursor (y > inline_cursor_y).
                // Terminal reflow engines (such as xterm.js) process buffer lines bottom-to-top and increment
                // the cursor screen Y position for each extra row added below the cursor.
                let total_buffer_height = buffer.area.height;
                for y in (inline_cursor_y + 1)..total_buffer_height {
                    let line_width =
                        Self::compute_line_width_from_buffer_opts(buffer, y, trim_whitespace);
                    if line_width > new_width {
                        let extra_rows = (line_width + new_width - 1) / new_width - 1;
                        log::info!(
                            "Line {} below cursor width: {}, extra rows added: {}",
                            y,
                            line_width,
                            extra_rows
                        );
                        total_rows += extra_rows;
                    }
                }
            }
            settings::ResizeLogic::ReflowedAll
            | settings::ResizeLogic::ReflowedAllWhitespaceTrimmed => {
                // Cursor row wraps at new_width; cursor is offset by inline_cursor_x / new_width
                let cursor_row_offset = inline_cursor_x / new_width;
                total_rows += cursor_row_offset;
            }
            settings::ResizeLogic::AutoCleared => {}
            settings::ResizeLogic::DontMoveCursor => {}
            settings::ResizeLogic::Default => {}
        }

        total_rows
    }

    /// Return a mutable reference to the history manager for the given fuzzy source.
    pub(crate) fn select_fuzzy_history_manager_mut(
        &mut self,
        source: &FuzzyHistorySource,
    ) -> &mut HistoryManager {
        match source {
            FuzzyHistorySource::PastCommands => &mut self.long_lived.history_manager,
            FuzzyHistorySource::CancelledCommands => {
                &mut self.long_lived.cancelled_command_history_manager
            }
            FuzzyHistorySource::AgentPrompts => &mut self.long_lived.agent_prompt_history_manager,
        }
    }

    /// Return an immutable reference to the history manager for the given fuzzy source.
    pub(crate) fn select_fuzzy_history_manager(
        &self,
        source: &FuzzyHistorySource,
    ) -> &HistoryManager {
        match source {
            FuzzyHistorySource::PastCommands => &self.long_lived.history_manager,
            FuzzyHistorySource::CancelledCommands => {
                &self.long_lived.cancelled_command_history_manager
            }
            FuzzyHistorySource::AgentPrompts => &self.long_lived.agent_prompt_history_manager,
        }
    }

    pub fn run(mut self) -> ExitState {
        // Send execution finished escape codes (previous command has completed).
        time_it!("startup: escape codes", {
            if crate::settings().send_shell_integration_codes
                == settings::ShellIntegrationLevel::Full
            {
                let last_command_exit_value = shell::backend().last_command_exit_status();
                let hostname = shell::backend().hostname();
                let cwd = shell::backend().cwd();

                shell_integration::write_startup_codes(last_command_exit_value, &hostname, &cwd)
                    .unwrap_or_else(|e| {
                        log::error!("Failed to write execution finished escape codes: {}", e);
                    });
            }
        });

        shell::backend().prep_terminal();

        let event_reader = self.terminal.backend_mut().terminal_mut().event_reader();
        let poll_terminal_event = |event_reader: &termina::EventReader,
                                   timeout: Duration|
         -> std::io::Result<Option<TerminaEvent>> {
            if let Some(reason) = stdin_unavailable_reason() {
                log::error!("Cannot read terminal events: {}", reason);
                return Err(Error::new(ErrorKind::UnexpectedEof, reason));
            }

            if event_reader.poll(Some(timeout), |_| true)? {
                return event_reader.read(|_| true).map(Some);
            }
            Ok(None)
        };

        let mut redraw = true;
        let mut last_terminal_size = self.terminal.size().unwrap();

        'main_loop: loop {
            if let Some(resize_time) = self.last_resize_time {
                // Getting cursor pos can be slow via ssh
                // and resizes can happen in quick succession, so we wait a bit before requesting the cursor pos
                if resize_time.elapsed() >= std::time::Duration::from_millis(150) {
                    self.last_resize_time = None;
                    self.sync_viewport_top_from_cpr();
                    if self.terminal.viewport_top().is_none() {
                        log::warn!("[Resize] CPR returned None for viewport_top");
                    }
                }
            }

            self.handle_delayed_startup();

            if self.poll_agent() {
                redraw = true;
            }
            if self.poll_tab_completion(0) {
                redraw = true;
            }
            if self.poll_flycomp() {
                redraw = true;
            }
            if self.poll_path_warming() {
                redraw = true;
            }
            if self.poll_git_warming() {
                redraw = true;
            }

            if self.leader_key_active_at.map_or(false, |t| {
                t.elapsed() >= std::time::Duration::from_millis(1000)
            }) {
                self.leader_key_active_at = None;
                redraw = true;
            }

            if redraw {
                if self.needs_full_redraw {
                    if let Err(e) = self.terminal.resize(last_terminal_size.into()) {
                        log::error!("Failed to resync inline viewport after bash command: {}", e);
                    }
                }

                let frame_area = self.terminal.get_frame().area();

                let content =
                    self.create_content(frame_area.width, frame_area.y, last_terminal_size.height);

                let was_screen_cleared = self.needs_screen_cleared;
                if self.needs_screen_cleared {
                    self.needs_screen_cleared = false;
                    let _ = self.terminal.clear_screen();
                }
                let desired_height = content.height().min(last_terminal_size.height);

                // This helps to reduce flicker.
                // Each time we call set_viewport_height, there is a chance it flickers.
                if desired_height > frame_area.height {
                    log::info!(
                        "Resizing inline viewport from {} to {} rows",
                        frame_area.height,
                        desired_height
                    );
                    self.terminal
                        .set_viewport_height(desired_height)
                        .unwrap_or_else(|e| {
                            log::error!("Failed to set viewport height: {}", e);
                        });
                }

                let prev_contents = std::mem::take(&mut self.last_contents);
                let show_terminal_cursor = (crate::settings().cursor_config.backend()
                    == crate::cursor::CursorBackend::Terminal
                    || !self.mode.is_running())
                    && !(mouse_state(|m| m.is_left_button_down())
                        && self.buffer.selection_range().is_some()
                        && matches!(
                            mouse_state(|m| m.last_mouse_over_cell_semantic),
                            Some(Tag::Command(_))
                        ));
                let needs_full_redraw = self.needs_full_redraw;
                if self.needs_full_redraw {
                    self.needs_full_redraw = false;
                }

                let set_mode =
                    |code| Csi::Mode(DecMode::SetDecPrivateMode(DecPrivateMode::Code(code)));
                let reset_mode =
                    |code| Csi::Mode(DecMode::ResetDecPrivateMode(DecPrivateMode::Code(code)));

                let current_viewport_top = self.terminal.viewport_top();
                let mut drawn_content: Option<DrawnContent> = None;

                let _ =
                    crate::flush_stdout!("{}", set_mode(DecPrivateModeCode::SynchronizedOutput));

                let draw_result = {
                    let _timer = crate::perf::PerfTimer::start("draw");
                    self.terminal.draw(|f| {
                        drawn_content = Some(Self::ui(
                            f,
                            content,
                            needs_full_redraw,
                            show_terminal_cursor,
                            current_viewport_top,
                        ));
                    })
                };

                self.last_contents = drawn_content;

                match draw_result {
                    Ok(_) => {
                        self.last_draw_time = std::time::Instant::now();

                        if let Some(top) = self.terminal.viewport_top() {
                            if let Some(ref mut drawn) = self.last_contents {
                                drawn.viewport_start = Some(top);
                            }
                        }

                        if matches!(
                            crate::settings().send_shell_integration_codes,
                            settings::ShellIntegrationLevel::OnlyPromptPos
                                | settings::ShellIntegrationLevel::Full
                        ) {
                            let force_resend_prompt_codes = was_screen_cleared || needs_full_redraw;
                            let prev_start = if force_resend_prompt_codes {
                                None
                            } else {
                                prev_contents
                                    .as_ref()
                                    .and_then(|c| c.prompt_start_relative())
                            };
                            let prev_end = if force_resend_prompt_codes {
                                None
                            } else {
                                prev_contents.as_ref().and_then(|c| c.prompt_end_relative())
                            };

                            shell_integration::write_after_rendering_codes(
                                prev_start,
                                prev_end,
                                self.last_contents
                                    .as_ref()
                                    .and_then(|c| c.prompt_start_relative()),
                                self.last_contents
                                    .as_ref()
                                    .and_then(|c| c.prompt_end_relative()),
                                self.mode.is_running(),
                            )
                            .unwrap_or_else(|e| {
                                log::error!("Failed to write prompt position escape codes: {}", e);
                            });
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to draw terminal UI: {}", e);
                        self.mode = AppRunningState::Exiting(ExitState::WithoutCommand);
                    }
                }

                let _ =
                    crate::flush_stdout!("{}", reset_mode(DecPrivateModeCode::SynchronizedOutput));
                self.reevaluate_pointer_shape();
            }

            if !self.mode.is_running() {
                break;
            }

            let is_idle = self.last_activity_time.elapsed() >= IDLE_TIMEOUT;
            let effective_fps = if is_idle {
                crate::settings()
                    .idle_frame_rate
                    .min(crate::settings().frame_rate as f64)
            } else {
                crate::settings().frame_rate as f64
            };
            let min_refresh_rate: Duration = Duration::from_millis((1000.0 / effective_fps) as u64);

            redraw = match poll_terminal_event(&event_reader, min_refresh_rate) {
                Ok(Some(event)) => {
                    let r = match event {
                        TerminaEvent::Key(key) => {
                            self.last_activity_time = std::time::Instant::now();
                            self.handle_key_event(key);
                            true
                        }
                        TerminaEvent::Mouse(mouse) => {
                            self.last_activity_time = std::time::Instant::now();
                            self.on_mouse(mouse)
                        }
                        TerminaEvent::WindowResized(winsize) => {
                            last_terminal_size = Size {
                                width: winsize.cols,
                                height: winsize.rows,
                            };

                            let effective_logic = crate::settings().resize_logic.resolve();

                            log::debug!(
                                "[Resize] Event received: cols={}, rows={}, resize_logic={:?} (resolved={:?})",
                                winsize.cols,
                                winsize.rows,
                                crate::settings().resize_logic,
                                effective_logic
                            );

                            self.terminal.clear_viewport_top();

                            let rows_up =
                                self.compute_wrapped_rows_up(winsize.cols, effective_logic);
                            log::debug!(
                                "[Resize] Moving cursor up by {} rows (resolved_logic={:?})",
                                rows_up,
                                effective_logic
                            );

                            if rows_up > 0 {
                                use termina::OneBased;
                                use termina::escape::csi::{Csi, Cursor};
                                let _ = crate::flush_stdout!(
                                    "{}{}",
                                    Csi::Cursor(Cursor::Up(rows_up as u32)),
                                    Csi::Cursor(Cursor::CharacterAbsolute(
                                        OneBased::from_zero_based(0)
                                    ))
                                );
                            }

                            // Ive noticed that zellij maintains its "is_line_wrapped" state
                            // even when we clear and redraw the lines
                            // This causes miscalculations of number of rows to move up because
                            // zellij combines two lines into one when the terminal is widened
                            // but our logic thinks those two lines are still separate.
                            // I have found issuing delete line commands fixes it.
                            let delete_count = (rows_up as u32 + 1).min(winsize.rows as u32);
                            if delete_count > 0 {
                                use termina::escape::csi::{Csi, Edit};
                                let _ = crate::flush_stdout!(
                                    "{}",
                                    Csi::Edit(Edit::DeleteLine(delete_count))
                                );
                            }

                            self.terminal.reset_inline_cursor();

                            self.terminal.clear().unwrap_or_else(|e| {
                                log::error!("Failed to clear terminal on resize: {}", e);
                            });

                            let final_winsize = winsize;
                            // Now that we have cleared it and we are at the top left as fast as possible,
                            // we could try and coalesce a burst of resize events to stay in sync with the term.
                            // let is_resize_event = |event: &TerminaEvent| {
                            //     matches!(event, TerminaEvent::WindowResized(_))
                            // };
                            // let mut final_winsize = winsize;
                            // while let Ok(true) = GLOBAL_EVENT_READER
                            //     .poll(Some(Duration::from_millis(500)), is_resize_event)
                            // {
                            //     if let Ok(TerminaEvent::WindowResized(new_winsize)) =
                            //         GLOBAL_EVENT_READER.read(is_resize_event)
                            //     {
                            //         log::debug!(
                            //             "[Resize] Coalesced pending resize event: cols={}, rows={}",
                            //             new_winsize.cols,
                            //             new_winsize.rows
                            //         );
                            //         final_winsize = new_winsize;
                            //     }
                            // }

                            // std::thread::sleep(Duration::from_millis(1000));

                            self.terminal
                                .resize(Rect {
                                    x: 0,
                                    y: 0,
                                    width: final_winsize.cols,
                                    height: final_winsize.rows,
                                })
                                .unwrap_or_else(|e| {
                                    log::error!("Failed to resize terminal: {}", e);
                                });
                            self.terminal.set_viewport_height(0).unwrap_or_else(|e| {
                                log::error!("Failed to set viewport height: {}", e);
                            });

                            self.sync_viewport_top_from_cpr();

                            if let Some(viewport_top) = self.terminal.viewport_top() {
                                let desired_height = self
                                    .last_contents
                                    .as_ref()
                                    .map_or(1, |c| c.contents.height())
                                    .min(final_winsize.rows);
                                let available_rows =
                                    final_winsize.rows.saturating_sub(viewport_top);
                                log::debug!(
                                    "[Resize] CPR viewport_top={}, desired_height={}, available_rows={}, term_rows={}",
                                    viewport_top,
                                    desired_height,
                                    available_rows,
                                    final_winsize.rows
                                );
                            } else {
                                log::warn!("[Resize] CPR returned None for viewport_top");
                            }

                            self.last_resize_time = Some(std::time::Instant::now());
                            self.needs_full_redraw = true;
                            true
                        }
                        TerminaEvent::FocusOut => {
                            // log::trace!("Terminal focus lost");
                            self.term_has_focus = false;
                            false
                        }
                        TerminaEvent::FocusIn => {
                            // log::trace!("Terminal focus gained");
                            self.term_has_focus = true;
                            if crate::settings().mouse_mode == MouseMode::Smart {
                                log::debug!(
                                    "Enabling mouse capture due to terminal focus gain in smart mode"
                                );
                                mouse_state(|m| m.enable());
                            }
                            false
                        }
                        TerminaEvent::Paste(pasted) => {
                            log::trace!("Pasted content: {}", pasted);
                            self.buffer.delete_selection();
                            self.buffer.insert_str(&pasted);
                            self.on_possible_buffer_change();
                            true
                        }
                        _ => false,
                    };
                    r
                }
                Ok(None) => true,
                Err(err) => {
                    log::info!(
                        "Terminal input problem, setting mode to exiting with EOF: {}",
                        err
                    );
                    self.mode = AppRunningState::Exiting(ExitState::EOF);
                    break 'main_loop;
                }
            };

            if std::time::Instant::now().duration_since(self.last_draw_time) > min_refresh_rate {
                // redraw periodically to update animations even when no events are occurring
                // (e.g. cursor blinking, matrix animation)
                redraw = true;
            }

            if self.check_and_run_pending_traps() {
                redraw = true;
            }

            // Check if a terminating signal has been received.
            // In bash >= 4.4 (readline 6.0+), rl_signal_event_hook is set when
            // bash receives a terminating signal.
            // But just checking for terminating_signal works on all versions of bash, and is more direct.
            let terminating_signal = shell::backend().read_terminating_signal();

            if terminating_signal != 0 {
                log::info!(
                    "Signal {} received, exiting immediately",
                    signal_to_str(terminating_signal)
                );
                self.mode = AppRunningState::Exiting(ExitState::WithoutCommand);
                break 'main_loop;
            }
        }

        shell::backend().deprep_terminal();

        match self.mode {
            AppRunningState::Exiting(ExitState::WithCommand(cmd)) => {
                if crate::settings().send_shell_integration_codes
                    == settings::ShellIntegrationLevel::Full
                {
                    shell_integration::write_on_exit_codes(Some(&cmd)).unwrap_or_else(|e| {
                        log::error!("Failed to write pre-execution escape codes: {}", e);
                    });
                }

                log::info!("Exiting with command: {}", cmd);
                ExitState::WithCommand(cmd)
            }
            _ => {
                if crate::settings().send_shell_integration_codes
                    == settings::ShellIntegrationLevel::Full
                {
                    shell_integration::write_on_exit_codes(None).unwrap_or_else(|e| {
                        log::error!("Failed to write pre-execution escape codes: {}", e);
                    });
                }

                if matches!(self.mode, AppRunningState::Exiting(ExitState::EOF)) {
                    ExitState::EOF
                } else {
                    ExitState::WithoutCommand
                }
            }
        }
    }

    fn toggle_mouse_state(&mut self) {
        mouse_state(|m| m.toggle());
        if mouse_state(|m| m.is_disabled()) {
            mouse_state(|m| {
                m.last_mouse_over_cell_semantic = None;
                m.last_mouse_over_cell_direct = None;
            });
        }
    }

    /// Execute a closure with the terminal restored to cooked (normal) mode,
    /// and automatically restore raw mode, mouse state, and key codes afterwards.
    pub(crate) fn with_cooked_terminal<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let mouse_enabled = mouse_state(|m| m.is_enabled());
        mouse_state(|m| m.disable());

        let mut stdout = std::io::stdout();
        restore_terminal(&mut stdout);
        if let Err(e) = self
            .terminal
            .backend_mut()
            .terminal_mut()
            .enter_cooked_mode()
        {
            log::error!("Failed to enter cooked mode: {}", e);
        }
        let _ = write!(stdout, "\r");
        let _ = std::io::Write::flush(&mut stdout);

        let result = f();

        if let Err(e) = self.terminal.backend_mut().terminal_mut().enter_raw_mode() {
            log::error!("Failed to re-enter raw mode: {}", e);
        }
        configure_terminal(
            crate::settings().enable_extended_key_codes,
            &crate::settings().mouse_mode,
        );
        if mouse_enabled {
            mouse_state(|m| m.enable());
        }

        self.sync_viewport_top_from_cpr();
        let _ = self.terminal.clear();
        self.needs_full_redraw = true;

        result
    }

    /// Check if host shell has pending signal traps (e.g. SIGUSR1, SIGUSR2, etc.)
    /// and run them in cooked terminal mode. Returns true if traps were executed.
    pub(crate) fn check_and_run_pending_traps(&mut self) -> bool {
        if shell::backend().has_pending_traps() {
            self.with_cooked_terminal(|| {
                shell::backend().run_pending_traps();
            });
            self.on_possible_buffer_change();

            true
        } else {
            false
        }
    }

    /// This is meant to mimic bash_execute_unix_command from bashline.c
    pub(crate) fn run_bash_command(&mut self, cmd: &str) {
        // 1. Export READLINE_* variables before running command
        let selection_was_active = self.buffer.selection_byte().is_some();
        let initial_mark_char_offset = self
            .buffer
            .selection_char_offset()
            .unwrap_or_else(|| self.buffer.cursor_char_offset());

        let current_line = self.buffer.buffer().to_string();
        let current_point = self.buffer.cursor_char_offset().to_string();
        let current_mark = initial_mark_char_offset.to_string();

        let _ = shell::backend().export_env_var("READLINE_LINE", &current_line);
        let _ = shell::backend().export_env_var("READLINE_POINT", &current_point);
        let _ = shell::backend().export_env_var("READLINE_MARK", &current_mark);
        let _ = shell::backend().export_env_var("READLINE_ARGUMENT", "1");

        // 2. Execute command inside cooked terminal block
        self.with_cooked_terminal(|| {
            if let Err(e) = shell::backend().evaluate_shell_string(cmd) {
                log::error!("Failed to execute bash command '{}': {}", cmd, e);
            }
        });

        // 3. Read READLINE_* env vars and set text buffer, cursor, and mark positions
        if let Some(new_line) = shell::backend().env_var("READLINE_LINE") {
            let cleaned_line = new_line.trim_end_matches(['\r', '\n']);
            self.buffer.replace_buffer(cleaned_line);
        }

        let new_point_char_offset =
            if let Some(new_point_str) = shell::backend().env_var("READLINE_POINT") {
                if let Ok(new_point) = new_point_str.parse::<usize>() {
                    let byte_pos = self.buffer.char_to_byte_offset(new_point);
                    self.buffer.try_move_cursor_to_byte_pos(byte_pos, true);
                    new_point
                } else {
                    self.buffer.cursor_char_offset()
                }
            } else {
                self.buffer.cursor_char_offset()
            };

        if let Some(new_mark_str) = shell::backend().env_var("READLINE_MARK") {
            if let Ok(new_mark_char_offset) = new_mark_str.parse::<usize>() {
                if new_mark_char_offset != new_point_char_offset
                    && (selection_was_active || new_mark_char_offset != initial_mark_char_offset)
                {
                    let byte_pos = self.buffer.char_to_byte_offset(new_mark_char_offset);
                    self.buffer.set_selection_anchor(byte_pos);
                } else {
                    self.buffer.clear_selection();
                }
            } else {
                self.buffer.clear_selection();
            }
        } else {
            self.buffer.clear_selection();
        }

        // 4. Unset READLINE_* variables (matching GNU Readline unbind_readline_variables)
        let _ = shell::backend().unset_env_var("READLINE_LINE");
        let _ = shell::backend().unset_env_var("READLINE_POINT");
        let _ = shell::backend().unset_env_var("READLINE_MARK");
        let _ = shell::backend().unset_env_var("READLINE_ARGUMENT");
    }

    /// Compute the [`ButtonState`] of an interactive cell with the given `tag`,
    /// based on whether the mouse is hovering it and whether the left mouse
    /// button is currently held down.
    fn button_state_for(&self, tag: Tag) -> ButtonState {
        if mouse_state(|m| m.last_mouse_over_cell_semantic) != Some(tag) {
            ButtonState::Normal
        } else if mouse_state(|m| m.is_left_button_down()) {
            ButtonState::Depressed
        } else {
            ButtonState::Hovered
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> bool {
        let _timer = crate::perf::PerfTimer::start("on_mouse");
        log::trace!("Mouse event: {:?}", mouse);

        let now = std::time::Instant::now();
        self.last_mouse = Some(LastMouseEvent {
            mouse,
            matches: Vec::new(),
            time: now,
        });
        mouse_state(|m| m.last_mouse_pos = Some((mouse.column, mouse.row)));

        // 1. Resolve tags
        let (direct_tag, mut semantic_tag) = self
            .last_contents
            .as_ref()
            .and_then(|drawn_contents| drawn_contents.get_tagged_cell(mouse.column, mouse.row))
            .map(|(direct, semantic)| (Some(direct), Some(semantic)))
            .unwrap_or((None, None));

        let is_dragging_command = mouse_state(|m| {
            m.drag_start_tag
                .is_some_and(|tag| matches!(tag, Tag::Command(_)))
        }) && matches!(mouse.kind, MouseEventKind::Drag(_));
        if is_dragging_command {
            if let Some(ref drawn) = self.last_contents
                && let Some(content_row) = drawn.term_em_row_to_content_row(mouse.row)
            {
                if content_row >= drawn.contents.buf.len() as isize {
                    semantic_tag = Some(Tag::Command(self.buffer.buffer().len()));
                } else if content_row < 0 || (content_row == 0 && semantic_tag.is_none()) {
                    semantic_tag = Some(Tag::Command(0));
                }
            }
        }
        let clicked_tag = semantic_tag;

        // 2. Update button states and over-cells in mouse_state
        mouse_state(|m| {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    m.set_left_button_down();
                    m.set_left_button_dragging(false);
                    m.drag_start_tag = clicked_tag;
                    if let Some(Tag::Command(byte_pos)) = clicked_tag {
                        m.record_left_click_down(byte_pos);
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    m.set_left_button_up();
                    m.set_left_button_dragging(false);
                    m.drag_start_tag = None;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    m.set_left_button_dragging(true);
                }
                MouseEventKind::Up(MouseButton::Right) => {
                    m.take_right_click_down_pos();
                }
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight => {
                    m.record_scroll();
                }
                _ => {}
            }

            m.last_mouse_over_cell_semantic = semantic_tag;
            m.last_mouse_over_cell_direct = direct_tag;
        });

        // 3. Evaluate context and dispatch declarative mouse action
        use crate::app::actions::mouse::{MouseActionOutput, RedrawUrgency};
        let mut combined_output = MouseActionOutput::default();
        combined_output.redraw_urgency = RedrawUrgency::Soon;

        let mut matches = Vec::new();
        let mut matched_any = false;
        for binding in crate::app::actions::mouse::DEFAULT_MOUSE_BINDINGS.iter() {
            if binding.context.evaluate_direct(self) {
                log::trace!("Matched mouse actions: {:?}", binding.actions);
                matches.push((binding.context.display(), format!("{:?}", binding.actions)));

                for action in &binding.actions {
                    let output = action.run(self, mouse);
                    combined_output.merge(output);
                    matched_any = true;
                }
                break;
            }
        }

        let mut redraw = false;
        if matched_any {
            if combined_output.possible_buffer_change {
                self.on_possible_buffer_change();
            }
            match combined_output.redraw_urgency {
                RedrawUrgency::Now => {
                    self.last_mouse = Some(LastMouseEvent {
                        mouse,
                        matches: matches.clone(),
                        time: now,
                    });
                    redraw = true;
                }
                RedrawUrgency::Soon => {
                    let prev_time = self.last_mouse.as_ref().map(|lm| lm.time);
                    let elapsed = prev_time
                        .map(|t| now.duration_since(t))
                        .unwrap_or(std::time::Duration::from_secs(9999));

                    if elapsed > std::time::Duration::from_millis(15) {
                        self.last_mouse = Some(LastMouseEvent {
                            mouse,
                            matches: matches.clone(),
                            time: now,
                        });
                        redraw = true;
                    } else {
                        self.last_mouse = Some(LastMouseEvent {
                            mouse,
                            matches: matches.clone(),
                            time: prev_time.unwrap_or(now),
                        });
                        redraw = false;
                    }
                }
            }
        } else {
            self.last_mouse = Some(LastMouseEvent {
                mouse,
                matches: vec![("none".to_string(), "none".to_string())],
                time: now,
            });
        }

        redraw
    }

    pub fn reevaluate_pointer_shape(&mut self) {
        if crate::settings().mouse_mode == settings::MouseMode::Disabled {
            mouse_state(|m| m.set_pointer_shape(crate::mouse_state::PointerShape::Default));
            return;
        }

        let (col, row) = match mouse_state(|m| m.last_mouse_pos) {
            Some(pos) => pos,
            None => return,
        };

        let (direct_tag, semantic_tag) = self
            .last_contents
            .as_ref()
            .and_then(|drawn_contents| drawn_contents.get_tagged_cell(col, row))
            .map(|(direct, semantic)| (Some(direct), Some(semantic)))
            .unwrap_or((None, None));

        mouse_state(|m| {
            m.last_mouse_over_cell_semantic = semantic_tag;
            m.last_mouse_over_cell_direct = direct_tag;
        });

        for binding in crate::app::actions::mouse::DEFAULT_POINTER_SHAPE_BINDINGS.iter() {
            if binding.context.evaluate_direct(self) {
                for action in &binding.actions {
                    if let crate::app::actions::mouse::MouseEventAction::SetPointer(shape) = action
                    {
                        mouse_state(|m| m.set_pointer_shape(*shape));
                        return;
                    }
                }
            }
        }
    }

    fn copy_to_clipboard(&self, text: &[u8]) -> bool {
        let text_str = std::str::from_utf8(text).unwrap_or_default();
        match crate::flush_stdout!(
            "{}",
            termina::escape::osc::Osc::SetSelection(
                termina::escape::osc::Selection::CLIPBOARD,
                text_str
            )
        ) {
            Ok(()) => true,
            Err(e) => {
                log::error!("Failed to copy to clipboard via OSC 52: {}", e);
                false
            }
        }
    }

    fn accept_fuzzy_history_search(&mut self) {
        let source = match &self.content_mode {
            ContentMode::FuzzyHistorySearch(s) => s.clone(),
            _ => return,
        };
        if let Some(entry) = self
            .select_fuzzy_history_manager(&source)
            .accept_fuzzy_search_result()
            .cloned()
        {
            let new_command = entry.command.clone();
            self.buffer.replace_buffer(new_command.as_str());
        }
        self.content_mode = ContentMode::Normal;
    }

    fn accept_fuzzy_history_search_agent_command(&mut self) {
        if let ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts) =
            &self.content_mode
        {
            let entry = self
                .long_lived
                .agent_prompt_history_manager
                .accept_fuzzy_search_result()
                .cloned();

            if let Some(entry) = entry {
                self.buffer.replace_buffer(&entry.command);

                if let Some(raw_output) = entry.raw_output() {
                    match parse_ai_output(raw_output) {
                        Ok(parsed) => {
                            self.content_mode =
                                ContentMode::AgentOutputSelection(AiOutputSelection::new(
                                    parsed,
                                    &crate::settings().colour_palette,
                                    self.buffer.buffer(),
                                ));
                            return;
                        }
                        Err(e) => {
                            log::warn!("Failed to parse cached AI output: {}", e);
                            self.dismissed_agent_mode_buffer =
                                Some(self.buffer.buffer().to_string());
                            self.content_mode = ContentMode::AgentError {
                                message: format!("Failed to parse cached AI output: {}", e),
                                raw_output: raw_output.to_string(),
                                suggested_setup_command: None,
                            };
                            return;
                        }
                    }
                }
                self.content_mode = ContentMode::Normal;
            } else {
                if let Some((agent_cmd, buffer)) = self.resolve_agent_command(false) {
                    self.start_agent_mode(agent_cmd, &buffer);
                } else {
                    self.show_agent_mode_not_configured_error();
                }
            }
        }
    }

    /// Poll the AI background task; returns `true` if a redraw is needed.
    fn poll_agent(&mut self) -> bool {
        let ai_result: Option<Result<String, (String, String)>> =
            if let ContentMode::AgentModeWaiting { ref mut child, .. } = self.content_mode {
                match child.0.try_wait() {
                    Ok(Some(status)) => {
                        // Process has exited; drain the pipes synchronously.
                        // This is safe because the child has exited (all write
                        // ends of the pipes are closed) so read_to_string returns
                        // immediately after consuming the buffered data.
                        let stdout = child.0.stdout.take().map_or_else(String::new, |mut out| {
                            let mut buf = String::new();
                            let _ = std::io::Read::read_to_string(&mut out, &mut buf);
                            buf
                        });
                        let stdout = stdout.trim().to_string();
                        if status.success() {
                            Some(Ok(stdout))
                        } else {
                            let stderr =
                                child.0.stderr.take().map_or_else(String::new, |mut err| {
                                    let mut buf = String::new();
                                    let _ = std::io::Read::read_to_string(&mut err, &mut buf);
                                    buf
                                });
                            let stderr = stderr.trim().to_string();
                            log::warn!("AI command exited with {}: {}", status, stderr);
                            Some(Err((
                                format!("AI command exited with {}", status),
                                format!("stdout: {}\nstderr: {}", stdout, stderr),
                            )))
                        }
                    }
                    Ok(None) => None,
                    Err(e) => {
                        log::warn!("AI task: try_wait error: {}", e);
                        Some(Err((format!("AI task failed: {}", e), String::new())))
                    }
                }
            } else {
                None
            };
        if let Some(result) = ai_result {
            match result {
                Ok(raw_output) => {
                    self.long_lived
                        .agent_prompt_history_manager
                        .set_last_raw_output(raw_output.clone());
                    match parse_ai_output(&raw_output) {
                        Ok(parsed) => {
                            self.content_mode =
                                ContentMode::AgentOutputSelection(AiOutputSelection::new(
                                    parsed,
                                    &crate::settings().colour_palette,
                                    self.buffer.buffer(),
                                ));
                        }
                        Err(e) => {
                            log::warn!("AI command returned no suggestions: {}", e);
                            self.dismissed_agent_mode_buffer =
                                Some(self.buffer.buffer().to_string());
                            self.content_mode = ContentMode::AgentError {
                                message: format!("Failed to parse AI output: {}", e),
                                raw_output,
                                suggested_setup_command: None,
                            };
                        }
                    }
                }
                Err((msg, raw_output)) => {
                    log::error!("AI command failed: {}", msg);
                    self.long_lived
                        .agent_prompt_history_manager
                        .set_last_raw_output(raw_output.clone());
                    self.dismissed_agent_mode_buffer = Some(self.buffer.buffer().to_string());
                    self.content_mode = ContentMode::AgentError {
                        message: msg,
                        raw_output,
                        suggested_setup_command: None,
                    };
                }
            }
            return true;
        }
        false
    }

    /// Poll the tab-completion subshell; returns `true` if a redraw is needed.
    pub(crate) fn poll_tab_completion(&mut self, timeout_ms: u16) -> bool {
        if let ContentMode::TabCompletionWaiting {
            ref handle,
            ref wuc_substring,
            auto_started,
            ..
        } = self.content_mode
        {
            use subshell_ipc::IpcStatus;
            match handle.receiver.poll_status_timeout(timeout_ms) {
                IpcStatus::Ready(completion_res) => {
                    log::trace!(
                        "Tab completion subshell PID {} delivered payload",
                        handle.pid
                    );
                    let wuc = wuc_substring.clone();
                    self.content_mode = ContentMode::Normal;

                    if let Some((builder, elapsed)) = completion_res {
                        self.finish_tab_complete(builder, wuc, elapsed, auto_started);
                        self.on_possible_buffer_change();
                    }
                    return true;
                }
                IpcStatus::Disconnected => {
                    log::error!(
                        "Tab completion subshell PID {} disconnected without sending valid payload; resetting to Normal mode",
                        handle.pid
                    );
                    self.content_mode = ContentMode::Normal;
                    return true;
                }
                IpcStatus::Empty => match waitpid(handle.pid, Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(_, code)) => {
                        log::error!(
                            "Tab completion subshell PID {} exited with code {} before sending payload",
                            handle.pid,
                            code
                        );
                        self.content_mode = ContentMode::Normal;
                        return true;
                    }
                    Ok(WaitStatus::Signaled(_, sig, _)) => {
                        log::error!(
                            "Tab completion subshell PID {} killed by signal {:?} before sending payload",
                            handle.pid,
                            sig
                        );
                        self.content_mode = ContentMode::Normal;
                        return true;
                    }
                    _ => {}
                },
            }
        }
        false
    }

    fn poll_flycomp(&mut self) -> bool {
        if let ContentMode::TabCompletionRunningFlycomp {
            ref command_word,
            ref handle,
            ..
        } = self.content_mode
        {
            match handle.receiver.poll_status() {
                IpcStatus::Ready(script) => {
                    let cmd_word = command_word.clone();
                    log::info!("flycomp succeeded for command '{}'", cmd_word);
                    let output_dir = crate::settings().flycomp.output_dir();
                    let _ = shell::backend()
                        .resolve_and_write_completion_script(&cmd_word, &script, output_dir);
                    let _ = shell::backend().evaluate_shell_string(&script);
                    self.content_mode = ContentMode::Normal;
                    self.start_tab_complete(false, None);
                    return true;
                }
                IpcStatus::Disconnected => {
                    log::error!(
                        "flycomp subshell PID {} disconnected without sending script for command '{}'",
                        handle.pid,
                        command_word
                    );
                    self.content_mode = ContentMode::TabCompletionFlycompResult {
                        command_word: command_word.clone(),
                        error_message: "flycomp subshell exited without payload".to_string(),
                    };
                    return true;
                }
                IpcStatus::Empty => match waitpid(handle.pid, Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(_, code)) if code != 0 => {
                        log::error!(
                            "flycomp subshell PID {} exited with non-zero code {}",
                            handle.pid,
                            code
                        );
                        self.content_mode = ContentMode::TabCompletionFlycompResult {
                            command_word: command_word.clone(),
                            error_message: format!("flycomp failed with exit code {}", code),
                        };
                        return true;
                    }
                    Ok(WaitStatus::Signaled(_, sig, _)) => {
                        log::error!(
                            "flycomp subshell PID {} killed by signal {:?}",
                            handle.pid,
                            sig
                        );
                        self.content_mode = ContentMode::TabCompletionFlycompResult {
                            command_word: command_word.clone(),
                            error_message: format!("flycomp killed by signal {:?}", sig),
                        };
                        return true;
                    }
                    _ => {}
                },
            }
        }
        false
    }

    fn poll_path_warming(&mut self) -> bool {
        if let Some(ref handle) = self.path_warming_subshell {
            match handle.receiver.poll_status() {
                IpcStatus::Ready(infos) => {
                    shell::ExecutablesOnPath::apply_updates(infos);
                    log::debug!("Path warming subshell finished successfully");
                    self.path_warming_subshell = None;
                    return true;
                }
                IpcStatus::Disconnected => {
                    log::warn!("Path warming subshell disconnected without payload");
                    self.path_warming_subshell = None;
                    return true;
                }
                IpcStatus::Empty => {}
            }
        }
        false
    }

    fn poll_git_warming(&mut self) -> bool {
        if let Some(ref handle) = self.git_warming_subshell {
            match handle.receiver.poll_status() {
                IpcStatus::Ready(payload) => {
                    if let Some(payload) = payload {
                        let duration = payload.duration();
                        let is_updated =
                            matches!(payload, crate::git::GitRepoPayload::Updated { .. });
                        crate::git::apply_git_repo_payload(payload);
                        let ref_count = crate::git::get_cached_ref_count();
                        if is_updated {
                            log::info!(
                                "Git warming subshell finished in {:?} (found {} refs)",
                                duration,
                                ref_count
                            );
                        } else {
                            log::info!(
                                "Git warming subshell finished in {:?} (repo unchanged, kept {} refs)",
                                duration,
                                ref_count
                            );
                        }
                    } else {
                        crate::git::reset_cache();
                        log::info!("Git warming subshell finished (not in a git repository)");
                    }
                    self.git_warming_subshell = None;
                    return true;
                }
                IpcStatus::Disconnected => {
                    log::warn!("Git warming subshell disconnected without payload");
                    self.git_warming_subshell = None;
                    return true;
                }
                IpcStatus::Empty => {}
            }
        }
        false
    }

    fn handle_delayed_startup(&mut self) {
        if self.has_run_delayed_startup {
            return;
        }

        let delayed_startup_duration =
            std::time::Duration::from_millis(crate::settings().delayed_startup_ms);
        let long_enough_since_startup = self.app_start_time.elapsed() >= delayed_startup_duration;

        if !long_enough_since_startup {
            return;
        }

        self.has_run_delayed_startup = true;
        log::debug!("Running delayed startup initialization");
        time_it!("delayed startup", {
            let _ = crate::term_info::get_term_info(&GLOBAL_EVENT_READER);
            self.sync_viewport_top_from_cpr();

            if self.terminal.viewport_top().is_none() {
                log::warn!("[Startup] CPR returned None for viewport_top");
            }

            let set_mode = |code| Csi::Mode(DecMode::SetDecPrivateMode(DecPrivateMode::Code(code)));
            let _ = crate::flush_stdout!("{}", set_mode(DecPrivateModeCode::FocusTracking));
        });
    }

    pub(crate) fn run_flycomp(&mut self, command_word: String, word_under_cursor: String) {
        let poss_alias = shell::backend().find_alias(&command_word);
        let alias_def = poss_alias
            .as_deref()
            .filter(|alias| !alias.is_empty())
            .unwrap_or(&command_word);
        let alias_expanded_command_word = alias_def
            .split_whitespace()
            .next()
            .unwrap_or(alias_def)
            .to_string();

        let mut cmd_word = alias_expanded_command_word;
        if cmd_word.starts_with('~') || cmd_word.contains('/') {
            let expanded = shell::backend().expand_path(&cmd_word);
            if !expanded.is_empty() {
                cmd_word = expanded;
            }
        }
        let start_time = std::time::Instant::now();
        let flycomp_settings = crate::settings().flycomp.clone();

        if let Some(handle) = subshell_ipc::spawn_subshell(move || {
            crate::reset_sigchld();
            flycomp::generate_completion_output_with_settings(
                &cmd_word,
                flycomp::OutputFormat::Bash,
                &flycomp_settings,
            )
            .ok()
        }) {
            self.content_mode = ContentMode::TabCompletionRunningFlycomp {
                command_word,
                _word_under_cursor: word_under_cursor,
                start_time,
                handle,
            };
        }
    }

    fn show_agent_mode_not_configured_error(&mut self) {
        let (message, suggested_setup_command) = {
            // No agent configured at all — try to find a suitable one from the example file.
            let setup_cmd = crate::agent_mode::parse_example_agent_commands()
                .into_iter()
                .find(|(cmd_name, _)| shell::backend().command_info(cmd_name).is_known())
                .map(|(_, flyline_cmd)| flyline_cmd);

            match setup_cmd {
                Some(cmd) => (
                    "Agent mode is not configured. However, flyline can set it up for you:".to_string(),
                    Some(cmd),
                ),
                None => (
                    "Agent mode is not configured. Run `flyline set-agent-mode --help` or see https://github.com/HalFrgrd/flyline#agent-mode".to_string(),
                    setup_cmd,
                )
            }
        };
        self.dismissed_agent_mode_buffer = Some(self.buffer.buffer().to_string());
        self.content_mode = ContentMode::AgentError {
            message,
            raw_output: String::new(),
            suggested_setup_command,
        };
    }

    /// Resolve which agent command to use for Alt+Enter.
    /// First tries to find a trigger-prefix match, then falls back to the `None`-keyed default and then any available command if prefix is not required.
    fn resolve_agent_command(
        &self,
        needs_prefix: bool,
    ) -> Option<(settings::AgentModeCommand, String)> {
        if let Some((agent_cmd, stripped)) = self.buffer_starts_with_agent_command_prefix() {
            return Some((agent_cmd.clone(), stripped.trim_start().to_string()));
        }

        if needs_prefix {
            return None;
        }

        let buf = self.buffer.buffer();
        let none_prefix_cmd = crate::settings()
            .agent_commands
            .get(&None)
            .map(|cmd| (cmd.clone(), buf.to_string()));

        if none_prefix_cmd.is_some() {
            return none_prefix_cmd;
        }
        // Ignore the prefixing and just get any command.
        crate::settings()
            .agent_commands
            .values()
            .next()
            .map(|cmd| (cmd.clone(), buf.to_string()))
    }

    fn buffer_starts_with_agent_command_prefix(
        &self,
    ) -> Option<(&settings::AgentModeCommand, &str)> {
        let buf = self.buffer.buffer();
        for (prefix_key, agent_cmd) in &crate::settings().agent_commands {
            if let Some(prefix) = prefix_key
                && let Some(stripped) = buf.strip_prefix(prefix.as_str())
            {
                return Some((agent_cmd, stripped));
            }
        }
        None
    }

    /// Spawn the configured AI command as a child process and transition to `AgentModeWaiting`.
    /// Words that contain a space are quoted with single quotes in the display string.
    /// If `buffer_str` is empty, opens the agent-prompts fuzzy history search instead.
    fn start_agent_mode(&mut self, agent_cmd: settings::AgentModeCommand, buffer_str: &str) {
        // TODO: think through UX for running agent mode with an empty buffer
        // (e.g. opening the agent-prompts fuzzy history search). For now we
        // always push the (possibly empty) buffer and spawn the command.
        self.long_lived
            .agent_prompt_history_manager
            .push_entry(self.buffer.buffer().to_string());
        let cmd_args = agent_cmd.command;
        let final_arg = match agent_cmd.system_prompt.as_deref() {
            Some(prompt) => format!("{}\n{}", prompt, buffer_str),
            None => buffer_str.to_string(),
        };
        // Build a human-readable representation of the full command being run.
        // Any word that contains a space is wrapped in single quotes, with any
        // embedded single quotes escaped using the shell '\'' idiom.
        let command_display = {
            let mut parts = cmd_args.clone();
            parts.push(final_arg.clone());
            parts
                .iter()
                .map(|p| {
                    if p.contains(' ') {
                        format!("'{}'", p.replace('\'', "'\\''"))
                    } else {
                        p.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        log::info!("Running AI command: {}", command_display);
        // Safety: the guard `!ai_command.is_empty()` at the call site ensures
        // cmd_args is non-empty, so split_first() always returns Some.
        let (prog, args) = cmd_args.split_first().expect("ai_command is non-empty");
        crate::reset_sigchld();
        match std::process::Command::new(prog)
            .args(args)
            .arg(&final_arg)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => {
                self.content_mode = ContentMode::AgentModeWaiting {
                    child: KillOnDropChild::new(child),
                    command_display,
                    start_time: std::time::Instant::now(),
                };
            }
            Err(e) => {
                log::error!("Failed to spawn AI command: {}", e);
                self.dismissed_agent_mode_buffer = Some(self.buffer.buffer().to_string());
                self.content_mode = ContentMode::AgentError {
                    message: format!("Failed to run AI command: {}", e),
                    raw_output: String::new(),
                    suggested_setup_command: None,
                };
            }
        }
    }

    /// Submit the current buffer if bash would accept it, otherwise insert a newline.
    fn try_submit_current_buffer(&mut self) {
        let complete_command = command_acceptance::will_bash_accept_buffer(self.buffer.buffer());
        if self.unfinished_from_prev_command || complete_command {
            self.mode =
                AppRunningState::Exiting(ExitState::WithCommand(self.buffer.buffer().to_string()));
        } else {
            self.buffer.insert_newline();
        }
    }

    fn on_possible_buffer_change(&mut self) {
        if let ContentMode::AgentOutputSelection(ref mut selection) = self.content_mode {
            let current_buf = self.buffer.buffer();
            if current_buf != selection.last_buffer_content {
                selection.selected_idx = None;
                selection.last_buffer_content = current_buf.to_string();
            }
        }
        let is_fresh = if let Some(last_key) = &self.last_key {
            let fresh = last_key.sequence_number > self.last_processed_key_sequence;
            self.last_processed_key_sequence = last_key.sequence_number;
            fresh
        } else {
            false
        };

        // Exit PromptCwdEdit mode if the cursor has moved away from position 0,
        // which happens when a buffer-modifying normal action fires (e.g. insert_char).
        if matches!(self.content_mode, ContentMode::PromptDirSelect(_))
            && self.buffer.cursor_byte_pos() != 0
        {
            self.content_mode = ContentMode::Normal;
        }

        let navigated_history = if let Some(last_key) = &self.last_key {
            last_key.actions.iter().any(|action| {
                matches!(
                    action,
                    KeyEventAction::PrevHistoryEntry
                        | KeyEventAction::NextHistoryEntry
                        | KeyEventAction::FuzzyHistoryAcceptEntry
                        | KeyEventAction::FuzzyHistoryAcceptAndEdit
                        | KeyEventAction::FuzzyHistoryAcceptAndRun
                )
            })
        } else {
            false
        };

        let current_buf = self.buffer.buffer().to_string();
        if self
            .dismissed_agent_mode_buffer
            .as_deref()
            .is_some_and(|b| b != current_buf)
        {
            self.dismissed_agent_mode_buffer = None;
        }

        if matches!(self.content_mode, ContentMode::AgentError { .. })
            && self.dismissed_agent_mode_buffer.is_none()
        {
            self.content_mode = ContentMode::Normal;
        }

        if !navigated_history && matches!(self.content_mode, ContentMode::Normal) {
            if self.dismissed_agent_mode_buffer.is_none()
                && let Some((_agent_cmd, _stripped)) =
                    self.buffer_starts_with_agent_command_prefix()
            {
                self.long_lived
                    .agent_prompt_history_manager
                    .warm_fuzzy_search_cache(self.buffer.buffer(), None);
                self.content_mode =
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts);
            }
        } else if matches!(
            self.content_mode,
            ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts)
        ) {
            if self.buffer_starts_with_agent_command_prefix().is_none() {
                self.content_mode = ContentMode::Normal;
            }
        }

        let is_tab_completion_active = matches!(
            self.content_mode,
            ContentMode::TabCompletion(_) | ContentMode::TabCompletionWaiting { .. }
        );

        if (crate::settings().auto_suggest || is_tab_completion_active) && self.last_key.is_some() {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            enum CompletionAction {
                Keep,
                // carry_over: do we want to use the current suggestions as a placeholder?
                // if the new suggestions will be similar to the old ones, we can use them
                // while the new ones load to avoid a flicker of no suggestions.
                Restart { carry_over: bool },
                Discard,
                Update,
            }

            let get_action =
                |app: &Self,
                 new_completion_context: &tab_completion_context::CompletionContext<'_>|
                 -> Option<CompletionAction> {
                    let new_wuc = &new_completion_context.word_under_cursor;
                    None
                    .or_else(|| {
                        mouse_state(|m| m.is_left_button_dragging())
                            // If we're dragging the mouse, we dont want to have tab completions
                            .then_some(CompletionAction::Discard)
                    })
                    // pressing up and down when navigating history. so dont let suggestions get in the way
                    .or_else(|| {
                        (navigated_history || app.buffer.buffer().is_empty())
                            .then_some(CompletionAction::Discard)
                    })
                    // If we have dismissed suggestions for this wuc, keep them dismissed
                    .or_else(|| {
                        let is_wuc_identical =
                            app.dismissed_tab_completion_wuc.as_deref() == Some(new_wuc.s.as_str());
                        is_wuc_identical.then_some(CompletionAction::Keep)
                    })
                    // restart auto tab completion if the last key was a trigger character
                    // typing / when completing a path should restart completions so we can tab complete the next folder
                    // typing - often starts the `--flag` style completions instead of default filename completions, so we want to restart completions when typing -
                    // similar ideas for other trigger chars
                    .or_else(|| {
                        let is_tab_completion_auto_started = match &app.content_mode {
                            ContentMode::TabCompletionWaiting { auto_started, .. } => *auto_started,
                            ContentMode::TabCompletion(active_suggestions) => active_suggestions.auto_started,
                            _ => false,
                        };

                        let is_trigger_active = is_tab_completion_auto_started
                            && matches!(
                                app.content_mode,
                                ContentMode::Normal
                                    | ContentMode::TabCompletionWaiting { .. }
                                    | ContentMode::TabCompletion(_)
                            );

                        if is_trigger_active {
                            let last_char_is_trigger = is_fresh
                                .then(|| app.last_key.as_ref())
                                .flatten()
                                .and_then(|k| match k.key.code {
                                    KeyCode::Char(c)
                                        if !k
                                            .key
                                            .modifiers
                                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                                    {
                                        let is_trigger = c == '/'
                                            || c == '$'
                                            || c == '~'
                                            || c == '.'
                                            || c == '+'
                                            || c == '='
                                            || (c == '-' && new_wuc.s.chars().all(|ch| ch == '-'));
                                        is_trigger.then_some(c)
                                    }
                                    _ => None,
                                });

                            last_char_is_trigger.map(|c| {
                                let carry_over = c == '-' || c == '.';
                                CompletionAction::Restart { carry_over }
                            })
                        } else {
                            None
                        }
                    })
                    // Lets get the auto suggestionns going!
                    .or_else(|| {
                        (crate::settings().auto_suggest && matches!(app.content_mode, ContentMode::Normal))
                            .then_some(CompletionAction::Restart { carry_over: false })
                    })
                    // This block is more about refining the tab completions when active and knowing when to discard them (e.g. moved cursor to another word)
                    .or_else(|| {
                        match &app.content_mode {
                            ContentMode::TabCompletionWaiting {
                                wuc_substring,
                                auto_started,
                                ..
                            } => {
                                let old_wuc = &wuc_substring.s;
                                if *auto_started && new_wuc.s.chars().count() < old_wuc.chars().count() {
                                    log::debug!(
                                        "Word under cursor became shorter than waiting wuc ('{}' -> '{}') during automatic tab completion",
                                        old_wuc,
                                        new_wuc.s
                                    );
                                    Some(CompletionAction::Restart { carry_over: true })
                                } else if !new_wuc.s.starts_with(old_wuc)
                                    && !old_wuc.starts_with(&new_wuc.s)
                                {
                                    if crate::settings().auto_suggest {
                                        Some(CompletionAction::Restart { carry_over: false })
                                    } else {
                                        Some(CompletionAction::Discard)
                                    }
                                } else {
                                    None
                                }
                            }
                            ContentMode::TabCompletion(active_suggestions) => {
                                let orig_wuc = &active_suggestions.original_word_under_cursor.s;
                                let current_wuc = &active_suggestions.word_under_cursor;

                                if active_suggestions.auto_started
                                    && new_wuc.s.chars().count() < orig_wuc.chars().count()
                                {
                                    log::debug!(
                                        "Word under cursor became shorter than original wuc ('{}' -> '{}')",
                                        orig_wuc,
                                        new_wuc.s
                                    );
                                    Some(CompletionAction::Restart { carry_over: true })
                                } else if *new_wuc != *current_wuc && active_suggestions.auto_started && new_completion_context.comp_types().contains(&tab_completion_context::CompType::GlobExpansion) {
                                    log::debug!(
                                        "Word under cursor changed ('{:?}') and new completion context contains glob expansion so lets just restart to pick it up.",
                                        new_wuc
                                    );
                                    Some(CompletionAction::Restart { carry_over: true })
                                } else if *new_wuc == *current_wuc {
                                    log::debug!(
                                        "Word under cursor unchanged ('{:?}'), keeping existing tab completion suggestions",
                                        new_wuc
                                    );
                                    Some(CompletionAction::Keep)
                                } else if new_wuc.s.is_empty() && !orig_wuc.is_empty() {
                                    log::debug!(
                                        "Word under cursor cleared, discarding tab completion suggestions"
                                    );
                                    Some(CompletionAction::Discard)
                                } else if new_wuc.overlaps_with(current_wuc) {
                                    let old_len = current_wuc.s.chars().count();
                                    let new_len = new_wuc.s.chars().count();
                                    if old_len.abs_diff(new_len) > 1 {
                                        log::debug!(
                                            "Word under cursor changed slightly but by multiple characters ('{}' -> '{}')",
                                            current_wuc.s,
                                            new_wuc.s
                                        );
                                        Some(CompletionAction::Restart { carry_over: true })
                                    } else {
                                        Some(CompletionAction::Update)
                                    }
                                } else {
                                    log::debug!(
                                        "Word under cursor changed significantly ('{:?}' -> '{:?}'), discarding tab completion suggestions",
                                        current_wuc,
                                        new_wuc
                                    );
                                    if crate::settings().auto_suggest {
                                        Some(CompletionAction::Restart { carry_over: false })
                                    } else {
                                        Some(CompletionAction::Discard)
                                    }
                                }
                            }
                            _ => None,
                        }
                    })
                };

            let new_completion_context = self.get_completion_context();
            let action =
                get_action(self, &new_completion_context).unwrap_or(CompletionAction::Keep);
            let new_wuc = new_completion_context.word_under_cursor;

            match action {
                CompletionAction::Keep => {}
                CompletionAction::Discard => {
                    self.take_active_suggestions();
                    self.dismissed_tab_completion_wuc = None;
                }
                CompletionAction::Update => {
                    self.dismissed_tab_completion_wuc = None;
                    if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                        log::debug!(
                            "Word under cursor changed slightly ('{}' -> '{}'), applying fuzzy filter to tab completion suggestions",
                            active_suggestions.word_under_cursor.s,
                            new_wuc.s
                        );
                        active_suggestions.update_word_under_cursor(&new_wuc);
                    }
                }
                CompletionAction::Restart { carry_over } => {
                    self.dismissed_tab_completion_wuc = None;
                    let previous_suggestions = self.take_active_suggestions();
                    self.start_tab_complete(
                        true,
                        if carry_over {
                            previous_suggestions
                        } else {
                            None
                        },
                    );
                }
            }
        }

        let new_tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(
            self.buffer.buffer(),
            &self.dparser_tokens_cache,
        );
        // for token in &new_tokens {
        //     log::info!("Parsed token '{:#?}", token);
        // }

        self.dparser_tokens_cache = new_tokens;

        let history_buffer = self.buffer.buffer();

        // If the buffer has changed since the user dismissed the suggestion, re-enable it.
        if self
            .dismissed_inline_suggestion_buffer
            .as_deref()
            .is_some_and(|b| b != history_buffer)
        {
            self.dismissed_inline_suggestion_buffer = None;
        }

        self.inline_history_suggestion = if !crate::settings().show_inline_history
            || history_buffer.is_empty()
            || self.dismissed_inline_suggestion_buffer.is_some()
        {
            None
        } else {
            self.long_lived
                .history_manager
                .get_command_suggestion_suffix(history_buffer)
        };

        self.formatted_buffer_cache = if matches!(
            self.content_mode,
            ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts)
                | ContentMode::AgentError { .. }
                | ContentMode::AgentOutputSelection { .. }
                | ContentMode::AgentModeWaiting { .. }
        ) {
            format_agent_buffer(
                &self.dparser_tokens_cache,
                self.buffer.cursor_byte_pos(),
                self.buffer.selection_byte(),
                self.buffer.buffer().len(),
                &crate::settings().colour_palette,
                crate::settings().enable_easter_eggs,
            )
        } else {
            format_buffer(
                &self.dparser_tokens_cache,
                self.buffer.cursor_byte_pos(),
                self.buffer.selection_byte(),
                self.buffer.buffer().len(),
                self.mode.is_running(),
                &crate::settings().colour_palette,
                crate::settings().enable_easter_eggs,
            )
        };

        let cursor_byte_pos = self.buffer.cursor_byte_pos();
        self.tooltip = self
            .formatted_buffer_cache
            .parts
            .iter()
            .rev()
            .find_map(|part| {
                if part
                    .token
                    .token
                    .byte_range()
                    .to_inclusive()
                    .contains(&cursor_byte_pos)
                {
                    part.tooltip.clone()
                } else {
                    None
                }
            });
    }
}

pub fn signal_to_str(sig: libc::c_int) -> &'static str {
    match sig {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGILL => "SIGILL",
        libc::SIGTRAP => "SIGTRAP",
        libc::SIGABRT => "SIGABRT",
        libc::SIGBUS => "SIGBUS",
        libc::SIGFPE => "SIGFPE",
        libc::SIGKILL => "SIGKILL",
        libc::SIGUSR1 => "SIGUSR1",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGUSR2 => "SIGUSR2",
        libc::SIGPIPE => "SIGPIPE",
        libc::SIGALRM => "SIGALRM",
        libc::SIGTERM => "SIGTERM",
        _ => "Unknown signal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_compute_line_width_from_buffer_and_wrapped_rows() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 5));

        // Prompt example:
        // Line 0: "(0 26ms) hal-itx-pc …al/projects/flyline better_resizing(1844)" (62 chars)
        let line0 = "(0 26ms) hal-itx-pc …al/projects/flyline better_resizing(1844)";
        for (x, ch) in line0.chars().enumerate() {
            let s = ch.to_string();
            buffer[(x as u16, 0)].set_symbol(&s);
        }

        // Line 1: "(+26-3) " (8 chars, includes trailing space)
        let line1 = "(+26-3) ";
        for (x, ch) in line1.chars().enumerate() {
            let s = ch.to_string();
            buffer[(x as u16, 1)].set_symbol(&s);
        }

        // Line 2: "> " (2 chars)
        let line2 = "> ";
        for (x, ch) in line2.chars().enumerate() {
            let s = ch.to_string();
            buffer[(x as u16, 2)].set_symbol(&s);
        }

        // Verify line widths
        assert_eq!(App::compute_line_width_from_buffer(&buffer, 0), 62);
        assert_eq!(App::compute_line_width_from_buffer(&buffer, 1), 8);
        assert_eq!(App::compute_line_width_from_buffer(&buffer, 2), 2);
        assert_eq!(App::compute_line_width_from_buffer(&buffer, 3), 0);

        // 1. AutoCleared -> inline_cursor_y (2)
        assert_eq!(
            App::compute_wrapped_rows_up_from_buffer(
                &buffer,
                2,
                2,
                30,
                settings::ResizeLogic::AutoCleared
            ),
            2
        );

        // 2. ReflowedApartFromCursor at new_width = 30:
        // Line 0 (62 cols) -> (62 + 30 - 1) / 30 = 3 rows
        // Line 1 (8 cols) -> 1 row
        // Total rows up = 3 + 1 = 4
        assert_eq!(
            App::compute_wrapped_rows_up_from_buffer(
                &buffer,
                2,
                2,
                30,
                settings::ResizeLogic::ReflowedApartFromCursor
            ),
            4
        );

        // 3. ReflowedAll at new_width = 30 with cursor at x = 45 on Line 2:
        // Line 0 (62 cols) -> 3 rows
        // Line 1 (8 cols) -> 1 row
        // Cursor line (x = 45) -> 45 / 30 = 1 extra row
        // Total rows up = 3 + 1 + 1 = 5
        assert_eq!(
            App::compute_wrapped_rows_up_from_buffer(
                &buffer,
                45,
                2,
                30,
                settings::ResizeLogic::ReflowedAll
            ),
            5
        );

        // 4. ReflowedApartFromCursor at new_width = 20:
        // Line 0 (62 cols) -> (62 + 20 - 1) / 20 = 4 rows
        // Line 1 (8 cols) -> 1 row
        // Total rows up = 4 + 1 = 5
        assert_eq!(
            App::compute_wrapped_rows_up_from_buffer(
                &buffer,
                2,
                2,
                20,
                settings::ResizeLogic::ReflowedApartFromCursor
            ),
            5
        );

        // 5. Test lines below cursor wrapping:
        // Set Line 2 to 62 cols (long line below cursor).
        for (x, ch) in line0.chars().enumerate() {
            let s = ch.to_string();
            buffer[(x as u16, 2)].set_symbol(&s);
        }
        // If cursor is at y = 1:
        // Line 0 (above cursor, 62 cols) -> 3 rows at new_width 30.
        // Line 2 (below cursor, 62 cols) -> (62 + 30 - 1)/30 - 1 = 2 extra rows.
        // Total rows up = 3 (for Line 0) + 2 (extra rows from Line 2 below) = 5 rows.
        assert_eq!(
            App::compute_wrapped_rows_up_from_buffer(
                &buffer,
                2,
                1,
                30,
                settings::ResizeLogic::ReflowedApartFromCursor
            ),
            5
        );
    }

    #[test]
    fn test_compute_wrapped_rows_up_reflowed_all_whitespace_trimmed() {
        let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 100, 4));

        let line0 = "(0 20ms) hal-itx-pc";
        for (x, ch) in line0.chars().enumerate() {
            let s = ch.to_string();
            buffer[(x as u16, 0)].set_symbol(&s);
        }
        for x in line0.chars().count()..50 {
            buffer[(x as u16, 0)].set_symbol(" ");
        }

        let line1 = ">";
        for (x, ch) in line1.chars().enumerate() {
            let s = ch.to_string();
            buffer[(x as u16, 1)].set_symbol(&s);
        }

        assert_eq!(App::compute_line_width_from_buffer(&buffer, 0), 50);
        assert_eq!(
            App::compute_line_width_from_buffer_opts(&buffer, 0, true),
            19
        );

        assert_eq!(
            App::compute_wrapped_rows_up_from_buffer(
                &buffer,
                1,
                1,
                20,
                settings::ResizeLogic::ReflowedAll
            ),
            3
        );

        assert_eq!(
            App::compute_wrapped_rows_up_from_buffer(
                &buffer,
                1,
                1,
                20,
                settings::ResizeLogic::ReflowedAllWhitespaceTrimmed
            ),
            1
        );
    }
}
