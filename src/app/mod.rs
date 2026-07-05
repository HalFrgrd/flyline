pub(crate) mod actions;
pub(crate) mod auto_close;
pub(crate) mod formatted_buffer;
mod tab_completion;
mod ui;
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
    HistoryEntry(String),
    Cwd(String),
}

use crate::active_suggestions::{ActiveSuggestions, ActiveSuggestionsBuilder, COLUMN_PADDING};
use crate::agent_mode::{AiOutputSelection, parse_ai_output};
use crate::app::actions::KeyEventAction;
use crate::app::formatted_buffer::{FormattedBuffer, format_agent_buffer, format_buffer};
use crate::content_builder::{Contents, SpanTag, Tag, TaggedLine, TaggedSpan};
use crate::cursor::{Cursor, CursorBackend};
use crate::dparser::{AnnotatedToken, ToInclusiveRange};
use crate::history::{HistoryEntry, HistoryEntryFormatted, HistoryManager};
use crate::iter_first_last::FirstLast;
use crate::kill_on_drop_child::KillOnDropChild;
use crate::mouse_state::{MouseState, PointerShape, XtShiftEscape};
use crate::palette::{ButtonState, Palette};
use crate::prompt_manager::PromptManager;
use crate::settings::{self, MatrixAnimation, MouseMode, Settings};
use crate::shell_integration;
use crate::text_buffer::{SubString, TextBuffer};
use crate::{bash_funcs, dparser};
use crate::{bash_symbols, command_acceptance};
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use flash::lexer::TokenKind;
use itertools::Itertools;
use ratatui::prelude::*;
use ratatui::text::StyledGrapheme;
use ratatui::{TerminalOptions, Viewport};
use std::boxed::Box;
use std::io::{Error, ErrorKind, IsTerminal};
use std::time::Duration;
use std::vec;

/// After this duration of inactivity the frame rate drops to 0.2 fps and the
/// cursor is rendered in the unfocused (dim, non-animated) state.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Frame rate (fps) used when the user has been idle for longer than [`IDLE_TIMEOUT`].
const IDLE_FRAME_RATE: f64 = 0.2;

fn restore_terminal(extended_key_codes: bool) {
    crossterm::terminal::disable_raw_mode().unwrap_or_else(|e| {
        // Likely from the master pty fd being closed.
        log::error!("Failed to disable raw mode: {}", e);
    });
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableBracketedPaste,
        crossterm::event::DisableFocusChange,
        crossterm::event::DisableMouseCapture,
        XtShiftEscape::Disable,
        PointerShape::Default,
    )
    .unwrap_or_else(|e| {
        log::error!("Failed to restore terminal features: {}", e);
    });
    if extended_key_codes {
        crossterm::execute!(
            std::io::stdout(),
            crossterm::event::PopKeyboardEnhancementFlags
        )
        .unwrap_or_else(|e| {
            log::error!("Failed to pop keyboard enhancement flags: {}", e);
        });
    }
}

fn set_panic_hook(extended_key_codes: bool) {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal(extended_key_codes);
        log::error!("Panic: {}", info);
        hook(info);
    }));
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

fn poll_terminal_event(timeout: Duration) -> std::io::Result<Option<CrosstermEvent>> {
    if let Some(reason) = stdin_unavailable_reason() {
        log::error!("Cannot read terminal events: {}", reason);
        return Err(Error::new(ErrorKind::UnexpectedEof, reason));
    }

    if event::poll(timeout)? {
        event::read().map(Some)
    } else {
        Ok(None)
    }
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

pub fn get_command(settings: &mut Settings) -> ExitState {
    // If stdin is closed, bash expects us to just return EOF a few times
    if let Some(reason) = stdin_unavailable_reason() {
        log::error!(
            "Standard input is not available: {}. Exiting without command.",
            reason
        );

        return ExitState::EOF;
    }

    let extended_key_codes = settings.enable_extended_key_codes;
    set_panic_hook(extended_key_codes);

    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();

    // Set up terminal features. Mouse capture is handled separately inside
    // MouseState::initialize (called in App::new) based on the configured mode.
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableFocusChange,
    )
    .unwrap_or_else(|e| {
        log::error!("Failed to set terminal features: {}", e);
    });
    if extended_key_codes {
        // Enabling REPORT_ALL_KEYS_AS_ESCAPE_CODES causes Ctrl+C to not copy to clipboard in VS Code with default settings
        // because it causes the press of Ctrl to be sent as a key code thus clearing the selection before 'c' is pressed.
        // https://blog.fsck.com/releases/2026/02/26/terminal-keyboard-protocol/ is a good reference for understanding the terminal key code problem.
        crossterm::execute!(
            std::io::stdout(),
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            )
        )
        .unwrap_or_else(|e| {
            log::error!("Failed to push keyboard enhancement flags: {}", e);
        });
    }

    let app = time_it!("startup: app creation", App::new(settings));

    let end_state = app.run();

    restore_terminal(extended_key_codes);

    log::debug!("Final state: {:?}", end_state);
    end_state
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FuzzyHistorySource {
    PastCommands,
    // CancelledCommands / AgentPrompts are not currently constructed (the
    // entry points that would do so are gated behind TODOs about UX). Allow
    // dead_code so the supporting machinery elsewhere is preserved for when
    // those entry points are wired up.
    CancelledCommands,
    #[allow(dead_code)]
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
pub(crate) struct TabCompletionHandle {
    receiver: std::sync::mpsc::Receiver<Option<(ActiveSuggestionsBuilder, std::time::Duration)>>,
    pid: Option<libc::pid_t>,
}

impl std::fmt::Debug for TabCompletionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabCompletionHandle").finish()
    }
}

impl Drop for TabCompletionHandle {
    fn drop(&mut self) {
        if let Some(pid) = self.pid.take() {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                // We don't need to wait for the pid here.
                // The tab completion thread will wait for it and we wait for the thread when we unload the app.
            }
        }
    }
}

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
        handle: TabCompletionHandle,
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
        sandbox: Option<String>,
        dump_path: String,
    },
    TabCompletionRunningFlycomp {
        command_word: String,
        _word_under_cursor: String,
        start_time: std::time::Instant,
        thread_handle: crate::threads::SharedJoinHandle<anyhow::Result<String>>,
    },
    TabCompletionFlycompResult {
        command_word: String,
        error_message: String,
    },
}

pub(crate) struct App<'a> {
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
    /// Parsed bash history available at startup.
    pub(super) history_manager: HistoryManager,
    pub(super) buffer_before_history_navigation: Option<String>,
    pub(super) inline_history_suggestion: Option<(HistoryEntry, String)>,
    /// Buffer contents at the time the user last dismissed the inline suggestion.
    /// While the buffer equals this value the suggestion is suppressed.
    pub(super) dismissed_inline_suggestion_buffer: Option<String>,
    /// Word-under-cursor at the time the user dismissed tab completion with Escape.
    /// While the new word-under-cursor equals this value, auto-suggest is suppressed.
    pub(super) dismissed_tab_completion_wuc: Option<String>,
    /// Buffer contents at the time the user last dismissed the agent prompts fuzzy history search.
    pub(super) dismissed_agent_prompts_buffer: Option<String>,
    pub(super) mouse_state: MouseState,
    pub(super) content_mode: ContentMode,
    pub(super) last_contents: Option<DrawnContent>,
    pub(super) tooltip: Option<String>,
    pub(super) settings: &'a mut Settings,
    /// Terminal row (absolute) where the inline viewport starts; used by smart mouse mode.
    /// Timestamp of the last draw operation.
    pub(super) last_draw_time: std::time::Instant,
    pub(super) needs_screen_cleared: bool,
    /// Last key event, context expression, and action dispatched.
    pub(super) last_key: Option<LastKeyPress>,
    /// Last mouse event received.
    pub(super) last_mouse: Option<LastMouseEvent>,
    /// Last processed key event sequence number for triggers.
    pub(super) last_processed_key_sequence: u64,
    /// Position of the right click popup, if active.
    pub(super) right_click_popup_pos: Option<crate::content_builder::Coord>,
    /// Target content to copy/cut determined at right-click depress time.
    pub(super) right_click_copy_target: Option<RightClickCopyTarget>,
    /// Timestamp of the last keypress or mouse event; used for idle-based matrix animation.
    pub(super) last_activity_time: std::time::Instant,
}

impl<'a> App<'a> {
    fn new(settings: &'a mut Settings) -> Self {
        let unfinished_from_prev_command =
            unsafe { crate::bash_symbols::current_command_line_count } > 0;
        let initial_buf_val = settings.initial_buffer.take().unwrap_or_default();
        let buffer = TextBuffer::new(&initial_buf_val);
        let formatted_buffer_cache = FormattedBuffer::default();

        bash_funcs::reset_caches();

        // Join any previous warming thread to prevent multiple active warming threads
        crate::threads::join_bash_func_threads();

        let _ = crate::threads::spawn_thread(crate::threads::ThreadTag::Warming, || {
            let _timer = crate::perf::PerfTimer::start("warming_thread");
            let start = std::time::Instant::now();
            crate::bash_funcs::warm_completion_caches();
            log::info!("Warming thread finished in {:?}", start.elapsed());
        });

        let mut app = App {
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
            history_manager: time_it!("startup: history manager", HistoryManager::new(settings)),
            buffer_before_history_navigation: None,
            inline_history_suggestion: None,
            dismissed_inline_suggestion_buffer: None,
            dismissed_tab_completion_wuc: None,
            dismissed_agent_prompts_buffer: None,
            mouse_state: time_it!(
                "startup: mouse state",
                MouseState::initialize(&settings.mouse_mode)
            ),
            content_mode: ContentMode::Normal,
            last_contents: None,
            tooltip: None,
            settings,
            last_draw_time: std::time::Instant::now(),
            needs_screen_cleared: false,
            last_key: None,
            last_mouse: None,
            last_processed_key_sequence: 0,
            right_click_popup_pos: None,
            right_click_copy_target: None,
            last_activity_time: std::time::Instant::now(),
        };

        app.on_possible_buffer_change();
        app
    }

    /// Return a mutable reference to the history manager for the given fuzzy source.
    pub(crate) fn select_fuzzy_history_manager_mut(
        &mut self,
        source: &FuzzyHistorySource,
    ) -> &mut HistoryManager {
        match source {
            FuzzyHistorySource::PastCommands => &mut self.history_manager,
            FuzzyHistorySource::CancelledCommands => {
                &mut self.settings.cancelled_command_history_manager
            }
            FuzzyHistorySource::AgentPrompts => &mut self.settings.agent_prompt_history_manager,
        }
    }

    /// Return an immutable reference to the history manager for the given fuzzy source.
    pub(crate) fn select_fuzzy_history_manager(
        &self,
        source: &FuzzyHistorySource,
    ) -> &HistoryManager {
        match source {
            FuzzyHistorySource::PastCommands => &self.history_manager,
            FuzzyHistorySource::CancelledCommands => {
                &self.settings.cancelled_command_history_manager
            }
            FuzzyHistorySource::AgentPrompts => &self.settings.agent_prompt_history_manager,
        }
    }

    pub fn run(mut self) -> ExitState {
        // Send execution finished escape codes (previous command has completed).
        time_it!("startup: escape codes", {
            if self.settings.send_shell_integration_codes == settings::ShellIntegrationLevel::Full {
                let last_command_exit_value =
                    unsafe { crate::bash_symbols::last_command_exit_value };
                let hostname = bash_funcs::get_hostname();
                let cwd = bash_funcs::get_cwd();

                shell_integration::write_startup_codes(last_command_exit_value, &hostname, &cwd)
                    .unwrap_or_else(|e| {
                        log::error!("Failed to write execution finished escape codes: {}", e);
                    });
            }
        });

        let mut terminal = time_it!("startup: terminal setup", {
            crossterm::terminal::enable_raw_mode().unwrap();

            let terminal = match ratatui::Terminal::with_options(
                ratatui::backend::CrosstermBackend::new(std::io::stdout()),
                TerminalOptions {
                    viewport: Viewport::Inline(0),
                },
            ) {
                Ok(terminal) => terminal,
                Err(err)
                    if err.to_string().contains(
                        "The cursor position could not be read within a normal duration",
                    ) =>
                {
                    // We could just bomb out here.
                    // I sometimes get this when running flyline in zellij.
                    log::error!(
                        "Inline viewport startup failed ({}); falling back to fullscreen viewport",
                        err
                    );

                    crossterm::execute!(
                        std::io::stdout(),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0)
                    )
                    .unwrap_or_else(|e| {
                        log::error!("Failed to clear terminal: {}", e);
                    });

                    // The cursor is often still messed up here.
                    ratatui::Terminal::with_options(
                        ratatui::backend::CrosstermBackend::new(std::io::stdout()),
                        TerminalOptions {
                            viewport: Viewport::Fullscreen,
                        },
                    )
                    .expect("Failed to create terminal with fullscreen viewport")
                }
                Err(err) => panic!("Failed to create terminal: {}", err),
            };

            bash_symbols::set_readline_state(bash_symbols::RL_STATE_TERMPREPPED);
            terminal
        });

        let mut redraw = true;
        let mut last_terminal_size = terminal.size().unwrap();

        'main_loop: loop {
            if self.poll_agent() {
                redraw = true;
            }
            if self.poll_tab_completion() {
                redraw = true;
            }
            if self.poll_flycomp() {
                redraw = true;
            }

            if redraw {
                let frame_area = terminal.get_frame().area();

                let content =
                    self.create_content(frame_area.width, frame_area.y, last_terminal_size.height);

                let desired_height = if self.needs_screen_cleared {
                    self.needs_screen_cleared = false;
                    last_terminal_size.height
                } else {
                    content.height().min(last_terminal_size.height)
                };

                terminal
                    .set_viewport_height(desired_height)
                    .unwrap_or_else(|e| {
                        log::error!("Failed to set viewport height: {}", e);
                    });

                let prev_contents = std::mem::take(&mut self.last_contents);
                let draw_result = {
                    let _timer = crate::perf::PerfTimer::start("draw");
                    terminal.draw(|f| self.ui(f, content))
                };
                match draw_result {
                    Ok(_) => {
                        self.last_draw_time = std::time::Instant::now();

                        if matches!(
                            self.settings.send_shell_integration_codes,
                            settings::ShellIntegrationLevel::OnlyPromptPos
                                | settings::ShellIntegrationLevel::Full
                        ) {
                            shell_integration::write_after_rendering_codes(
                                prev_contents
                                    .as_ref()
                                    .and_then(|c| c.term_em_prompt_start()),
                                prev_contents.as_ref().and_then(|c| c.term_em_prompt_end()),
                                self.last_contents
                                    .as_ref()
                                    .and_then(|c| c.term_em_prompt_start()),
                                self.last_contents
                                    .as_ref()
                                    .and_then(|c| c.term_em_prompt_end()),
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
            }

            if !self.mode.is_running() {
                break;
            }

            let is_idle = self.last_activity_time.elapsed() >= IDLE_TIMEOUT;
            let effective_fps = if is_idle {
                IDLE_FRAME_RATE.min(self.settings.frame_rate as f64)
            } else {
                self.settings.frame_rate as f64
            };
            let min_refresh_rate: Duration = Duration::from_millis((1000.0 / effective_fps) as u64);

            redraw = match poll_terminal_event(min_refresh_rate) {
                Ok(Some(event)) => {
                    let r = match event {
                        CrosstermEvent::Key(key) => {
                            self.last_activity_time = std::time::Instant::now();
                            self.handle_key_event(key);
                            true
                        }
                        CrosstermEvent::Mouse(mouse) => {
                            self.last_activity_time = std::time::Instant::now();
                            self.on_mouse(mouse)
                        }
                        CrosstermEvent::Resize(new_cols, new_rows) => {
                            // log::trace!("Terminal resized to {}x{}", new_cols, new_rows);
                            last_terminal_size = Size {
                                width: new_cols,
                                height: new_rows,
                            };
                            true
                        }
                        CrosstermEvent::FocusLost => {
                            // log::trace!("Terminal focus lost");
                            self.term_has_focus = false;
                            false
                        }
                        CrosstermEvent::FocusGained => {
                            // log::trace!("Terminal focus gained");
                            self.term_has_focus = true;
                            if self.settings.mouse_mode == MouseMode::Smart {
                                log::debug!(
                                    "Enabling mouse capture due to terminal focus gain in smart mode"
                                );
                                self.mouse_state.enable();
                            }
                            false
                        }
                        CrosstermEvent::Paste(pasted) => {
                            log::trace!("Pasted content: {}", pasted);
                            self.buffer.delete_selection();
                            self.buffer.insert_str(&pasted);
                            self.on_possible_buffer_change();
                            true
                        }
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

            // Check if a terminating signal has been received.
            // In bash >= 4.4 (readline 6.0+), rl_signal_event_hook is set when
            // bash receives a terminating signal.
            // But just checking for terminating_signal works on all versions of bash, and is more direct.
            let terminating_signal = bash_funcs::read_terminating_signal();

            if terminating_signal != 0 {
                log::info!(
                    "Signal {} received, exiting immediately",
                    signal_to_str(terminating_signal)
                );
                self.mode = AppRunningState::Exiting(ExitState::WithoutCommand);
                break 'main_loop;
            }
        }

        bash_symbols::clear_readline_state(bash_symbols::RL_STATE_TERMPREPPED);

        match self.mode {
            AppRunningState::Exiting(ExitState::WithCommand(cmd)) => {
                if self.settings.send_shell_integration_codes
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
                if self.settings.send_shell_integration_codes
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
        self.mouse_state.toggle();
        if !self.mouse_state.is_enabled() {
            self.mouse_state.last_mouse_over_cell_semantic = None;
            self.mouse_state.last_mouse_over_cell_direct = None;
        }
    }

    /// Compute the [`ButtonState`] of an interactive cell with the given `tag`,
    /// based on whether the mouse is hovering it and whether the left mouse
    /// button is currently held down.
    fn button_state_for(&self, tag: Tag) -> ButtonState {
        if self.mouse_state.last_mouse_over_cell_semantic != Some(tag) {
            ButtonState::Normal
        } else if self.mouse_state.is_left_button_down() {
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

        // 1. Resolve tags
        let (direct_tag, mut semantic_tag) = self
            .last_contents
            .as_ref()
            .and_then(|drawn_contents| drawn_contents.get_tagged_cell(mouse.column, mouse.row))
            .map(|(direct, semantic)| (Some(direct), Some(semantic)))
            .unwrap_or((None, None));

        let is_dragging_command = self
            .mouse_state
            .drag_start_tag
            .is_some_and(|tag| matches!(tag, Tag::Command(_)))
            && matches!(mouse.kind, MouseEventKind::Drag(_));
        if is_dragging_command {
            if let Some(ref drawn) = self.last_contents {
                let content_row = drawn.term_em_row_to_content_row(mouse.row);
                if content_row >= drawn.contents.buf.len() as isize {
                    semantic_tag = Some(Tag::Command(self.buffer.buffer().len()));
                } else if content_row < 0 || (content_row == 0 && semantic_tag.is_none()) {
                    semantic_tag = Some(Tag::Command(0));
                }
            }
        }
        let clicked_tag = semantic_tag;

        // 2. Update button states and over-cells in mouse_state
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.mouse_state.set_left_button_down();
                self.mouse_state.set_left_button_dragging(false);
                self.mouse_state.drag_start_tag = clicked_tag;
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    self.mouse_state.record_left_click_down(byte_pos);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_state.set_left_button_up();
                self.mouse_state.set_left_button_dragging(false);
                self.mouse_state.drag_start_tag = None;
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.mouse_state.set_left_button_dragging(true);
            }
            MouseEventKind::Up(MouseButton::Right) => {
                self.mouse_state.take_right_click_down_pos();
            }
            _ => {}
        }

        self.mouse_state.last_mouse_over_cell_semantic = semantic_tag;
        self.mouse_state.last_mouse_over_cell_direct = direct_tag;

        // 3. Evaluate context and dispatch declarative mouse action
        use crate::app::actions::mouse::{MouseActionOutput, RedrawUrgency};
        let mut combined_output = MouseActionOutput::default();
        combined_output.redraw_urgency = RedrawUrgency::Soon;

        let mut matches = Vec::new();
        let mut matched_any = false;
        let mut has_executed_non_pointer = false;
        for binding in crate::app::actions::mouse::DEFAULT_MOUSE_BINDINGS.iter() {
            if binding.context.evaluate_direct(self) {
                let is_pointer_action = matches!(
                    binding.action,
                    crate::app::actions::mouse::MouseEventAction::SetPointer(_)
                );
                if has_executed_non_pointer && !is_pointer_action {
                    continue;
                }
                log::trace!("Matched mouse action: {:?}", binding.action);
                matches.push((binding.context.display(), format!("{:?}", binding.action)));

                let output = binding.action.run(self, mouse);
                combined_output.merge(output);
                matched_any = true;
                if !is_pointer_action {
                    has_executed_non_pointer = true;
                }
            }
        }

        let mut redraw = false;
        if matched_any {
            if let Some(shape) = combined_output.desired_pointer_shape {
                let is_click_event =
                    matches!(mouse.kind, MouseEventKind::Down(_) | MouseEventKind::Up(_));
                self.mouse_state.set_pointer_shape(shape, is_click_event);
            }
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

    fn copy_to_clipboard(&self, text: &[u8]) -> bool {
        match crossterm::execute!(
            std::io::stdout(),
            crossterm::clipboard::CopyToClipboard::to_clipboard_from(text)
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
                .settings
                .agent_prompt_history_manager
                .accept_fuzzy_search_result()
                .cloned();

            if let Some(entry) = entry {
                self.buffer.replace_buffer(&entry.command);

                if let Some(raw_output) = &entry.raw_output {
                    match parse_ai_output(raw_output) {
                        Ok(parsed) => {
                            self.content_mode =
                                ContentMode::AgentOutputSelection(AiOutputSelection::new(
                                    parsed,
                                    &self.settings.colour_palette,
                                    self.buffer.buffer(),
                                ));
                            return;
                        }
                        Err(e) => {
                            log::warn!("Failed to parse cached AI output: {}", e);
                            self.content_mode = ContentMode::AgentError {
                                message: format!("Failed to parse cached AI output: {}", e),
                                raw_output: raw_output.clone(),
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
                    self.settings
                        .agent_prompt_history_manager
                        .set_last_raw_output(raw_output.clone());
                    match parse_ai_output(&raw_output) {
                        Ok(parsed) => {
                            self.content_mode =
                                ContentMode::AgentOutputSelection(AiOutputSelection::new(
                                    parsed,
                                    &self.settings.colour_palette,
                                    self.buffer.buffer(),
                                ));
                        }
                        Err(e) => {
                            log::warn!("AI command returned no suggestions: {}", e);
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
                    self.settings
                        .agent_prompt_history_manager
                        .set_last_raw_output(raw_output.clone());
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

    /// Poll the tab-completion background thread; returns `true` if a redraw is needed.
    fn poll_tab_completion(&mut self) -> bool {
        if let ContentMode::TabCompletionWaiting {
            ref handle,
            auto_started,
            ..
        } = self.content_mode
        {
            match handle.receiver.try_recv() {
                Ok(Some((builder, elapsed))) => {
                    // Take ownership of wuc_substring from the waiting state.
                    let (wuc, mut handle) =
                        match std::mem::replace(&mut self.content_mode, ContentMode::Normal) {
                            ContentMode::TabCompletionWaiting {
                                wuc_substring,
                                handle,
                                ..
                            } => (wuc_substring, handle),
                            _ => unreachable!(),
                        };
                    handle.pid = None; // defuse
                    self.finish_tab_complete(builder, wuc, elapsed, auto_started);
                    self.on_possible_buffer_change();
                    return true;
                }
                Ok(None) => {
                    // No suggestions generated.
                    if let ContentMode::TabCompletionWaiting { mut handle, .. } =
                        std::mem::replace(&mut self.content_mode, ContentMode::Normal)
                    {
                        handle.pid = None; // defuse
                    }
                    self.content_mode = ContentMode::Normal;
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still waiting; keep TabCompletionWaiting mode.
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::warn!("Tab completion thread disconnected unexpectedly");
                    self.content_mode = ContentMode::Normal;
                    return true;
                }
            }
        }
        false
    }

    fn poll_flycomp(&mut self) -> bool {
        let finished = if let ContentMode::TabCompletionRunningFlycomp {
            ref thread_handle, ..
        } = self.content_mode
        {
            thread_handle.is_finished()
        } else {
            false
        };

        if finished {
            let mode = std::mem::replace(&mut self.content_mode, ContentMode::Normal);
            if let ContentMode::TabCompletionRunningFlycomp {
                command_word,
                thread_handle,
                ..
            } = mode
            {
                let join_res = thread_handle.join_value();

                if let Some(res) = join_res {
                    match res {
                        Ok(Ok(script)) => {
                            log::info!("flycomp succeeded for command '{}'", command_word);
                            let output_dir = self.settings.flycomp_output.as_deref();
                            match crate::bash_funcs::resolve_and_write_completion_script(
                                &command_word,
                                &script,
                                output_dir,
                            ) {
                                Ok(write_path) => {
                                    log::info!(
                                        "Wrote synthesized completion script to '{}'",
                                        write_path.display()
                                    );
                                }
                                Err(e) => {
                                    log::error!("Failed to write completion script: {}", e);
                                }
                            }

                            match crate::bash_funcs::evaluate_shell_string(&script) {
                                Ok(_) => {
                                    log::info!(
                                        "Successfully evaluated synthesized completion script for '{}'",
                                        command_word
                                    );
                                    self.start_tab_complete(false, None);
                                }
                                Err(e) => {
                                    log::error!(
                                        "Failed to evaluate synthesized completion script: {:?}",
                                        e
                                    );
                                    let error_message = format!(
                                        "Failed to load script:\n  - {}",
                                        e.chain()
                                            .map(|c| c.to_string())
                                            .collect::<Vec<_>>()
                                            .join("\n  - ")
                                    );
                                    self.content_mode = ContentMode::TabCompletionFlycompResult {
                                        command_word,
                                        error_message,
                                    };
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            log::warn!("flycomp failed for command '{}': {:?}", command_word, e);
                            let error_message = e
                                .chain()
                                .map(|c| c.to_string())
                                .collect::<Vec<_>>()
                                .join("\n  - ");
                            self.content_mode = ContentMode::TabCompletionFlycompResult {
                                command_word,
                                error_message,
                            };
                        }
                        Err(join_err) => {
                            log::error!("flycomp thread panicked: {:?}", join_err);
                            self.content_mode = ContentMode::TabCompletionFlycompResult {
                                command_word,
                                error_message: "Thread panicked".to_string(),
                            };
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    pub(crate) fn run_flycomp(
        &mut self,
        command_word: String,
        word_under_cursor: String,
        use_sandbox: bool,
    ) {
        let poss_alias = crate::bash_funcs::find_alias(&command_word);
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
            let expanded = crate::bash_funcs::fully_expand_path(&cmd_word);
            if !expanded.is_empty() {
                cmd_word = expanded;
            }
        }
        let start_time = std::time::Instant::now();
        let shared_handle =
            crate::threads::spawn_thread(crate::threads::ThreadTag::Flycomp, move || {
                unsafe {
                    libc::signal(libc::SIGCHLD, libc::SIG_DFL);
                }
                flycomp::generate_completion_output(
                    &cmd_word,
                    flycomp::OutputFormat::Bash,
                    flycomp::SynthesisStrategy::ManPageOrRunHelp,
                    use_sandbox, // sandbox
                    5000,        // timeout_ms
                    2,           // recurse_limit
                )
            });
        self.content_mode = ContentMode::TabCompletionRunningFlycomp {
            command_word,
            _word_under_cursor: word_under_cursor,
            start_time,
            thread_handle: shared_handle,
        };
    }

    fn show_agent_mode_not_configured_error(&mut self) {
        let (message, suggested_setup_command) = {
            // No agent configured at all — try to find a suitable one from the example file.
            let setup_cmd = crate::agent_mode::parse_example_agent_commands()
                .into_iter()
                .find(|(cmd_name, _)| bash_funcs::get_command_info(cmd_name).is_known())
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
        let none_prefix_cmd = self
            .settings
            .agent_commands
            .get(&None)
            .map(|cmd| (cmd.clone(), buf.to_string()));

        if none_prefix_cmd.is_some() {
            return none_prefix_cmd;
        }
        // Ignore the prefixing and just get any command.
        self.settings
            .agent_commands
            .values()
            .next()
            .map(|cmd| (cmd.clone(), buf.to_string()))
    }

    fn buffer_starts_with_agent_command_prefix(
        &self,
    ) -> Option<(&settings::AgentModeCommand, &str)> {
        let buf = self.buffer.buffer();
        for (prefix_key, agent_cmd) in &self.settings.agent_commands {
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
        self.settings
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
        // SIGCHLD was already set to SIG_DFL by `Flyline::get()` before calling
        // `app::get_command`, so no per-process signal manipulation is needed.
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
            .dismissed_agent_prompts_buffer
            .as_deref()
            .is_some_and(|b| b != current_buf)
        {
            self.dismissed_agent_prompts_buffer = None;
        }

        if !navigated_history && matches!(self.content_mode, ContentMode::Normal) {
            if self.dismissed_agent_prompts_buffer.is_none()
                && let Some((_agent_cmd, _stripped)) =
                    self.buffer_starts_with_agent_command_prefix()
            {
                self.settings
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

        if (self.settings.auto_suggest || is_tab_completion_active) && self.last_key.is_some() {
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

            let get_action = |app: &Self, new_wuc: &SubString| -> Option<CompletionAction> {
                None
                    .or_else(|| {
                        app.mouse_state.is_left_button_dragging()
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
                        (app.settings.auto_suggest && matches!(app.content_mode, ContentMode::Normal))
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
                                    if app.settings.auto_suggest {
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
                                    if app.settings.auto_suggest {
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

            let new_wuc = self.completion_context().word_under_cursor;
            let action = get_action(self, &new_wuc).unwrap_or(CompletionAction::Keep);

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

        self.inline_history_suggestion = if !self.settings.show_inline_history
            || history_buffer.is_empty()
            || self.dismissed_inline_suggestion_buffer.is_some()
        {
            None
        } else {
            self.history_manager
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
                &self.settings.colour_palette,
            )
        } else {
            format_buffer(
                &self.dparser_tokens_cache,
                self.buffer.cursor_byte_pos(),
                self.buffer.selection_byte(),
                self.buffer.buffer().len(),
                self.mode.is_running(),
                &self.settings.colour_palette,
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
