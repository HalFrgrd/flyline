pub(crate) mod actions;
mod auto_close;
pub(crate) mod formatted_buffer;
mod tab_completion;

use crate::active_suggestions::{ActiveSuggestions, COLUMN_PADDING, MaybeProcessedSuggestion};
use crate::agent_mode::{AiOutputSelection, parse_ai_output};
use crate::app::formatted_buffer::{FormattedBuffer, format_buffer};
use crate::content_builder::{Contents, SpanTag, Tag, TaggedLine, TaggedSpan};
use crate::content_utils::{split_line_to_terminal_rows, ts_to_timeago_string_5chars};
use crate::cursor::{Cursor, CursorBackend};
use crate::dparser::{AnnotatedToken, ToInclusiveRange};
use crate::history::{HistoryEntry, HistoryEntryFormatted, HistoryManager};
use crate::iter_first_last::FirstLast;
use crate::kill_on_drop_child::KillOnDropChild;
use crate::mouse_state::MouseState;
use crate::palette::Palette;
use crate::prompt_manager::PromptManager;
use crate::settings::{self, MatrixAnimation, MouseMode, Settings};
use crate::text_buffer::{SubString, TextBuffer};
use crate::{bash_funcs, dparser, tutorial};
use crate::{bash_symbols, command_acceptance};
use crate::{shell_integration, tab_completion_context};
use crossterm::event::{self, Event as CrosstermEvent, MouseEvent, MouseEventKind};
use flash::lexer::TokenKind;
use itertools::Itertools;
use ratatui::prelude::*;
use ratatui::text::StyledGrapheme;
use ratatui::{Frame, TerminalOptions, Viewport, text::Line};
use std::boxed::Box;
use std::time::Duration;
use std::vec;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// After this duration of inactivity the frame rate drops to 0.2 fps and the
/// cursor is rendered in the unfocused (dim, non-animated) state.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Frame rate (fps) used when the user has been idle for longer than [`IDLE_TIMEOUT`].
const IDLE_FRAME_RATE: f64 = 0.2;

/// Encode `data` as standard base64 (RFC 4648, no line breaks).
/// Used to build OSC 52 clipboard sequences.
fn osc52_base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3F) as usize]);
        out.push(TABLE[((n >> 12) & 0x3F) as usize]);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3F) as usize]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3F) as usize]
        } else {
            b'='
        });
    }
    // SAFETY: `out` contains only bytes from `TABLE`, which is an ASCII
    // slice, so it is always valid UTF-8.
    String::from_utf8(out).unwrap()
}

fn build_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

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

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ExitState {
    WithCommand(String),
    WithoutCommand,
    EOF,
}

#[derive(PartialEq, Eq, Debug, Clone)]
enum AppRunningState {
    Running,
    Exiting(ExitState),
}

impl AppRunningState {
    pub fn is_running(&self) -> bool {
        *self == AppRunningState::Running
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum LastKeyPressAction {
    InsertedAutoClosing { char: char, byte_pos: usize },
}

pub fn get_command(settings: &mut Settings) -> ExitState {
    let extended_key_codes = settings.enable_extended_key_codes;
    set_panic_hook(extended_key_codes);

    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();

    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());

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
        crossterm::execute!(
            std::io::stdout(),
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        )
        .unwrap_or_else(|e| {
            log::error!("Failed to push keyboard enhancement flags: {}", e);
        });
    }

    let runtime = build_runtime();

    let t_app_create = std::time::Instant::now();
    let app = App::new(settings);
    log::trace!("startup: app creation: {:?}", t_app_create.elapsed());

    let end_state = runtime.block_on(app.run(backend));

    restore_terminal(extended_key_codes);

    log::debug!("Final state: {:?}", end_state);
    end_state
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Guard that owns the tab-completion background thread and the result channel.
/// Joining the thread (on drop) ensures it does not outlive the app.
struct TabCompletionHandle {
    receiver: std::sync::mpsc::Receiver<Option<Vec<MaybeProcessedSuggestion>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for TabCompletionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabCompletionHandle").finish()
    }
}

impl Drop for TabCompletionHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.thread.take() {
            if let Err(e) = handle.join() {
                log::warn!("Tab completion thread panicked: {:?}", e);
            }
        }
    }
}

#[derive(Debug)]
enum ContentMode {
    Normal,
    FuzzyHistorySearch(FuzzyHistorySource),
    TabCompletion(Box<ActiveSuggestions>),
    /// Tab completion is running in a background thread.  The handle owns both
    /// the result channel receiver and the thread join-handle so that cleanup
    /// happens automatically when the mode transitions.
    TabCompletionWaiting {
        handle: TabCompletionHandle,
        wuc_substring: SubString,
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
    /// When `suggested_buffer` is set, the error is a "no default agent but prefix-only config"
    /// case: pressing Enter will launch agent mode with that buffer instead of running help.
    /// When `suggested_setup_command` is set, an agent from the example file was found on PATH;
    /// pressing Enter will run that `flyline set-agent-mode ...` command to configure it.
    AgentError {
        message: String,
        raw_output: String,
        suggested_buffer: Option<String>,
        suggested_setup_command: Option<String>,
    },
    /// User is navigating the CWD path segments displayed in the prompt.
    /// The inner value is the currently highlighted segment index (0 = rightmost/current dir).
    PromptDirSelect(usize),
}

struct DrawnContent {
    contents: Contents,
    /// The terminal row (absolute) where the content starts. Used for translating mouse coordinates.
    viewport_start: u16,
    content_visible_row_range: std::ops::Range<u16>,
}

impl DrawnContent {
    pub fn content_row_to_term_em_row(&self, content_row: u16) -> u16 {
        content_row.saturating_sub(self.content_visible_row_range.start) + self.viewport_start
    }

    pub fn term_em_row_to_content_row(&self, term_em_row: u16) -> u16 {
        term_em_row.saturating_sub(self.viewport_start) + self.content_visible_row_range.start
    }

    pub fn term_em_cursor_pos(&self) -> Option<Position> {
        self.contents.term_cursor_pos.map(|cursor_pos| Position {
            x: cursor_pos.col,
            y: self.content_row_to_term_em_row(cursor_pos.row),
        })
    }

    pub fn term_em_prompt_start(&self) -> Option<Position> {
        self.contents.prompt_start.map(|prompt_start| Position {
            x: prompt_start.col,
            y: self.content_row_to_term_em_row(prompt_start.row),
        })
    }

    pub fn term_em_prompt_end(&self) -> Option<Position> {
        self.contents.prompt_end.map(|prompt_end| Position {
            x: prompt_end.col,
            y: self.content_row_to_term_em_row(prompt_end.row),
        })
    }

    pub fn get_tagged_cell(&self, term_em_x: u16, term_em_y: u16) -> Option<(Tag, bool)> {
        let content_row = self.term_em_row_to_content_row(term_em_y);

        let content_buf_row = self.contents.buf.get(content_row as usize)?;

        let direct_contact = content_buf_row.get(term_em_x as usize);

        if direct_contact.is_some_and(|cell| {
            matches!(
                cell.tag,
                Tag::Command(_)
                    | Tag::Suggestion(_)
                    | Tag::HistoryResult(_)
                    | Tag::AiResult(_)
                    | Tag::TutorialPrev
                    | Tag::TutorialNext
                    | Tag::Clipboard(_)
            )
        }) {
            return direct_contact.map(|cell| (cell.tag, true));
        }

        content_buf_row
            .iter()
            .enumerate()
            .rev()
            .find(|(col_idx, tagged_cell)| {
                *col_idx <= term_em_x as usize && matches!(tagged_cell.tag, Tag::Command(_))
            })
            .map(|(_, cell)| (cell.tag, false))
    }
}

pub(crate) struct App<'a> {
    mode: AppRunningState,
    buffer: TextBuffer,
    formatted_buffer_cache: FormattedBuffer,
    /// Cached annotated tokens from the last dparser run, including `is_auto_inserted` flags.
    dparser_tokens_cache: Vec<AnnotatedToken>,
    cursor: Cursor,
    /// Whether the terminal currently has focus. Used to control cursor animation intensity.
    term_has_focus: bool,
    unfinished_from_prev_command: bool,
    prompt_manager: PromptManager,
    /// Parsed bash history available at startup.
    history_manager: HistoryManager,
    buffer_before_history_navigation: Option<String>,
    inline_history_suggestion: Option<(HistoryEntry, String)>,
    mouse_state: MouseState,
    content_mode: ContentMode,
    last_contents: Option<DrawnContent>,
    last_mouse_over_cell: Option<Tag>,
    tooltip: Option<String>,
    settings: &'a mut Settings,
    /// Terminal row (absolute) where the inline viewport starts; used by smart mouse mode.
    /// Timestamp of the last draw operation.
    last_draw_time: std::time::Instant,
    needs_screen_cleared: bool,
    last_keypress_action: Option<LastKeyPressAction>,
    /// Timestamp of the last keypress or mouse event; used for idle-based matrix animation.
    last_activity_time: std::time::Instant,
}

impl<'a> App<'a> {
    fn new(settings: &'a mut Settings) -> Self {
        // log::info!("fully_expand_path test:");
        // log::info!(
        //     "fully_expand_path(\"$PWD\") = {}",
        //     tab_completion::fully_expand_path("$PWD")
        // );
        // log::info!(
        //     "fully_expand_path($(pwd)) = {}",
        //     tab_completion::fully_expand_path("$(pwd)")
        // );
        // log::info!(
        //     "fully_expand_path($(pwd)$HOME) = {}",
        //     tab_completion::fully_expand_path("$(pwd)$HOME")
        // );
        // log::info!(
        //     "fully_expand_path(\"~/Doc\") = {}",
        //     tab_completion::fully_expand_path("~/Doc")
        // );

        let unfinished_from_prev_command =
            unsafe { crate::bash_symbols::current_command_line_count } > 0;

        let buffer = TextBuffer::new("");
        let formatted_buffer_cache = FormattedBuffer::default();

        bash_funcs::reset_caches();

        let mut app = App {
            mode: AppRunningState::Running,
            buffer,
            formatted_buffer_cache,
            dparser_tokens_cache: Vec::new(),
            cursor: Cursor::new(),
            term_has_focus: true,
            unfinished_from_prev_command,
            prompt_manager: PromptManager::new(
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
            ),
            history_manager: HistoryManager::new(settings),
            buffer_before_history_navigation: None,
            inline_history_suggestion: None,
            mouse_state: MouseState::initialize(&settings.mouse_mode),
            content_mode: ContentMode::Normal,
            last_contents: None,
            last_mouse_over_cell: None,
            tooltip: None,
            settings,
            last_draw_time: std::time::Instant::now(),
            needs_screen_cleared: false,
            last_keypress_action: None,
            last_activity_time: std::time::Instant::now(),
        };

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

    pub async fn run(
        mut self,
        backend: ratatui::backend::CrosstermBackend<std::io::Stdout>,
    ) -> ExitState {
        #[cfg(feature = "integration-tests")]
        if self.settings.run_tab_completion_tests {
            self.test_tab_completions();
            return ExitState::WithoutCommand;
        }

        let t_run = std::time::Instant::now();

        // Send execution finished escape codes (previous command has completed).
        let t_escape = std::time::Instant::now();
        if self.settings.send_shell_integration_codes == settings::ShellIntegrationLevel::Full {
            let last_command_exit_value = unsafe { crate::bash_symbols::last_command_exit_value };
            let hostname = bash_funcs::get_hostname();
            let cwd = bash_funcs::get_cwd();

            shell_integration::write_startup_codes(last_command_exit_value, &hostname, &cwd)
                .unwrap_or_else(|e| {
                    log::error!("Failed to write execution finished escape codes: {}", e);
                });
        }
        log::trace!("startup: escape codes: {:?}", t_escape.elapsed());

        let t_terminal_setup = std::time::Instant::now();
        crossterm::terminal::enable_raw_mode().unwrap();

        let options = TerminalOptions {
            viewport: Viewport::Inline(0),
        };
        let mut terminal =
            ratatui::Terminal::with_options(backend, options).expect("Failed to create terminal");

        bash_symbols::set_readline_state(bash_symbols::RL_STATE_TERMPREPPED);
        log::trace!("startup: terminal setup: {:?}", t_terminal_setup.elapsed());

        let mut redraw = true;
        let mut last_terminal_size = terminal.size().unwrap();
        let mut initial_render_logged = false;
        let mut t_after_first_render: Option<std::time::Instant> = None;

        'main_loop: loop {
            // Poll AI background task: check if the child process has finished.
            let ai_result: Option<Result<String, (String, String)>> =
                if let ContentMode::AgentModeWaiting { ref mut child, .. } = self.content_mode {
                    match child.0.try_wait() {
                        Ok(Some(status)) => {
                            // Process has exited; drain the pipes synchronously.
                            // This is safe because the child has exited (all write
                            // ends of the pipes are closed) so read_to_string returns
                            // immediately after consuming the buffered data.
                            let stdout =
                                child.0.stdout.take().map_or_else(String::new, |mut out| {
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
                    Ok(raw_output) => match parse_ai_output(&raw_output) {
                        Ok(parsed) => {
                            self.content_mode = ContentMode::AgentOutputSelection(
                                AiOutputSelection::new(parsed, &self.settings.color_palette),
                            );
                        }
                        Err(e) => {
                            log::warn!("AI command returned no suggestions: {}", e);
                            self.content_mode = ContentMode::AgentError {
                                message: format!("Failed to parse AI output: {}", e),
                                raw_output,
                                suggested_buffer: None,
                                suggested_setup_command: None,
                            };
                        }
                    },
                    Err((msg, raw_output)) => {
                        log::error!("AI command failed: {}", msg);
                        self.content_mode = ContentMode::AgentError {
                            message: msg,
                            raw_output,
                            suggested_buffer: None,
                            suggested_setup_command: None,
                        };
                    }
                }
                redraw = true;
            }

            // Poll tab-completion background thread: check if results have arrived.
            if let ContentMode::TabCompletionWaiting { ref handle, .. } = self.content_mode {
                match handle.receiver.try_recv() {
                    Ok(Some(sugs)) => {
                        // Take ownership of wuc_substring from the waiting state.
                        let wuc =
                            match std::mem::replace(&mut self.content_mode, ContentMode::Normal) {
                                ContentMode::TabCompletionWaiting { wuc_substring, .. } => {
                                    wuc_substring
                                }
                                _ => unreachable!(),
                            };
                        self.finish_tab_complete(sugs, wuc);
                        redraw = true;
                    }
                    Ok(None) => {
                        // No suggestions generated.
                        self.content_mode = ContentMode::Normal;
                        redraw = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        // Still waiting; keep TabCompletionWaiting mode.
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        log::warn!("Tab completion thread disconnected unexpectedly");
                        self.content_mode = ContentMode::Normal;
                        redraw = true;
                    }
                }
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

                // Only time the very first draw; subsequent redraws are not startup.
                let t_draw = if !initial_render_logged {
                    Some(std::time::Instant::now())
                } else {
                    None
                };
                let prev_contents = std::mem::take(&mut self.last_contents);
                match terminal.draw(|f| self.ui(f, content)) {
                    Ok(_) => {
                        self.last_draw_time = std::time::Instant::now();

                        if let Some(t) = t_draw {
                            log::trace!("startup: initial render: {:?}", t.elapsed());
                            t_after_first_render = Some(std::time::Instant::now());
                            initial_render_logged = true;
                        }

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
                IDLE_FRAME_RATE
            } else {
                self.settings.frame_rate as f64
            };
            let min_refresh_rate: Duration = Duration::from_millis((1000.0 / effective_fps) as u64);

            if let Some(t) = t_after_first_render.take() {
                log::trace!("startup: until waiting on stdin: {:?}", t.elapsed());
                log::trace!("startup: total: {:?}", t_run.elapsed());
            }

            redraw = if event::poll(min_refresh_rate).unwrap() {
                match event::read().unwrap() {
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
                        if self.settings.mouse_mode == MouseMode::Smart
                            && !self.mouse_state.is_explicitly_disabled_by_user()
                        {
                            self.mouse_state.enable("smart mode: focus gained");
                        }
                        false
                    }
                    CrosstermEvent::Paste(pasted) => {
                        log::trace!("Pasted content: {}", pasted);
                        log::trace!("Pasted content as bytes: {:?}", pasted.as_bytes());
                        self.buffer.insert_str(&pasted);
                        self.on_possible_buffer_change();
                        true
                    }
                }
            } else {
                true
            };

            if std::time::Instant::now().duration_since(self.last_draw_time) > min_refresh_rate {
                // redraw periodically to update animations even when no events are occurring
                // (e.g. cursor blinking, matrix animation)
                redraw = true;
            }

            unsafe {
                // TODO: I might be able to get away with just checking terminating_signals for both versions
                // Check if a terminating signal has been received.
                // In bash >= 4.4 (readline 6.0+), rl_signal_event_hook is set when
                // bash receives a terminating signal. In older versions, we fall
                // back to checking the terminating_signal global directly.
                #[cfg(not(feature = "pre_bash_4_4"))]
                let got_signal = (&raw const crate::bash_symbols::rl_signal_event_hook)
                    .read()
                    .is_some();
                #[cfg(feature = "pre_bash_4_4")]
                let got_signal = (&raw const crate::bash_symbols::terminating_signal).read() != 0;

                if got_signal {
                    let sig = (&raw const crate::bash_symbols::terminating_signal).read();

                    log::info!(
                        "Signal {} received, exiting immediately",
                        signal_to_str(sig)
                    );

                    self.mode = AppRunningState::Exiting(ExitState::WithoutCommand);
                    break 'main_loop;
                }
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

    fn toggle_mouse_state(&mut self, reason: &str) {
        self.mouse_state.toggle(reason);
        if !self.mouse_state.enabled() {
            self.last_mouse_over_cell = None;
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> bool {
        log::trace!("Mouse event: {:?}", mouse);

        // Smart mode: check if a scroll event occurred or the mouse is above the viewport.
        if self.settings.mouse_mode == MouseMode::Smart {
            match mouse.kind {
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight => {
                    self.mouse_state
                        .disable("smart mode: scroll event detected");
                    self.last_mouse_over_cell = None;
                    return false;
                }
                _ => {}
            }
            if self
                .last_contents
                .as_ref()
                .is_some_and(|contents| mouse.row < contents.viewport_start)
            {
                // Only disable mouse capture when the user clicks above the viewport,
                // indicating intent to interact with terminal content above (e.g. select text).
                // Mere mouse movement above the viewport does not disable capture.
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    self.mouse_state
                        .disable("smart mode: click above the viewport");
                }
                self.last_mouse_over_cell = None;
                return false;
            }
        }

        let mut cursor_directly_on_cell = true;

        match self
            .last_contents
            .as_ref()
            .and_then(|drawn_contents| drawn_contents.get_tagged_cell(mouse.column, mouse.row))
        {
            Some((tag @ Tag::Suggestion(idx), true)) => {
                self.last_mouse_over_cell = Some(tag);
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.set_selected_by_idx(idx);
                }
            }
            Some((tag @ Tag::HistoryResult(idx), true)) => {
                self.last_mouse_over_cell = Some(tag);
                if let ContentMode::FuzzyHistorySearch(ref source) = self.content_mode {
                    let source = source.clone();
                    self.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_set_idx(idx);
                }
            }
            Some((tag @ Tag::AiResult(idx), true)) => {
                self.last_mouse_over_cell = Some(tag);
                if let ContentMode::AgentOutputSelection(selection) = &mut self.content_mode {
                    selection.set_selected_by_idx(idx);
                }
            }
            Some((tag @ Tag::Command(byte_pos), direct)) => {
                cursor_directly_on_cell = direct;
                self.last_mouse_over_cell = Some(tag);
                log::trace!("Mouse over command at byte position {}", byte_pos);
                if let Some(part) = self.formatted_buffer_cache.get_part_from_byte_pos(byte_pos)
                    && let Some(tooltip) = part.tooltip.as_ref()
                {
                    self.tooltip = Some(tooltip.clone());
                }
            }
            Some((tag @ Tag::TutorialPrev, true)) => {
                self.last_mouse_over_cell = Some(tag);
            }
            Some((tag @ Tag::TutorialNext, true)) => {
                self.last_mouse_over_cell = Some(tag);
            }
            Some((tag @ Tag::Clipboard(_), true)) => {
                self.last_mouse_over_cell = Some(tag);
            }
            Some((tag @ Tag::Ps1PromptCwd(_), _)) => {
                self.last_mouse_over_cell = Some(tag);
            }

            t => {
                log::trace!("Mouse over  {:?}", t);
                self.last_mouse_over_cell = None;
                // Exit PromptDirSelect mode when clicking on a non-CWD cell
                // that is within the terminal viewport (not above scrollback).
                if matches!(mouse.kind, MouseEventKind::Down(_))
                    && matches!(self.content_mode, ContentMode::PromptDirSelect(_))
                    && !matches!(t, Some((Tag::Ps1PromptCwd(_), _)))
                    && self
                        .last_contents
                        .as_ref()
                        .is_some_and(|c| mouse.row >= c.viewport_start)
                {
                    self.content_mode = ContentMode::Normal;
                }
            }
        }

        let mut update_buffer = false;

        match self.last_mouse_over_cell {
            Some(Tag::Suggestion(idx)) => {
                if matches!(mouse.kind, MouseEventKind::Up(_))
                    && let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode
                {
                    active_suggestions.set_selected_by_idx(idx);
                    active_suggestions.accept_selected_filtered_item(&mut self.buffer);
                    self.content_mode = ContentMode::Normal;
                    update_buffer = true;
                }
            }
            Some(Tag::HistoryResult(idx)) => {
                if matches!(mouse.kind, MouseEventKind::Up(_))
                    && matches!(self.content_mode, ContentMode::FuzzyHistorySearch(_))
                {
                    let source = match &self.content_mode {
                        ContentMode::FuzzyHistorySearch(s) => s.clone(),
                        _ => unreachable!(),
                    };
                    self.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_set_idx(idx);
                    self.accept_fuzzy_history_search();
                    update_buffer = true;
                }
            }
            Some(Tag::AiResult(idx)) => {
                if matches!(mouse.kind, MouseEventKind::Up(_))
                    && let ContentMode::AgentOutputSelection(selection) = &mut self.content_mode
                {
                    selection.set_selected_by_idx(idx);
                    if let Some(cmd) = selection.selected_command() {
                        let cmd = cmd.to_string();
                        self.buffer.replace_buffer(&cmd);
                        update_buffer = true;
                    }
                    self.content_mode = ContentMode::Normal;
                }
            }
            Some(Tag::Command(byte_pos)) => {
                if matches!(
                    mouse.kind,
                    MouseEventKind::Up(_) | MouseEventKind::Down(_) | MouseEventKind::Drag(_)
                ) {
                    self.buffer
                        .try_move_cursor_to_byte_pos(byte_pos, !cursor_directly_on_cell);
                    update_buffer = true;
                }
            }
            Some(Tag::TutorialPrev) => {
                if matches!(mouse.kind, MouseEventKind::Up(_)) {
                    self.settings.tutorial_step.prev();
                    log::info!(
                        "Tutorial navigated to prev: {:?}",
                        self.settings.tutorial_step
                    );
                    return true;
                }
            }
            Some(Tag::TutorialNext) => {
                if matches!(mouse.kind, MouseEventKind::Up(_)) {
                    self.settings.tutorial_step.next();
                    log::info!(
                        "Tutorial navigated to next: {:?}",
                        self.settings.tutorial_step
                    );
                    if !self.settings.tutorial_step.is_active() {
                        // Tutorial finished — but we can't set run_tutorial here since settings is &.
                        // The tutorial_step being NotRunning is sufficient.
                    }
                    return true;
                }
            }
            Some(Tag::Ps1PromptCwd(idx)) => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    self.content_mode = ContentMode::PromptDirSelect(idx);
                }
            }
            Some(Tag::Clipboard(clipboard_type)) => {
                if matches!(mouse.kind, MouseEventKind::Up(_)) {
                    if let Some(text) = self
                        .last_contents
                        .as_ref()
                        .and_then(|c| c.contents.clipboards.get(&clipboard_type))
                    {
                        let text = text.clone();
                        let encoded = osc52_base64(text.as_bytes());
                        use std::io::Write;
                        print!("\x1b]52;c;{}\x07", encoded);
                        std::io::stdout().flush().ok();
                        log::info!("Copied to clipboard via OSC 52 ({:?})", clipboard_type);
                        self.buffer.replace_buffer(&text);
                        update_buffer = true;
                    }
                }
            }
            _ => {}
        }

        if update_buffer {
            self.on_possible_buffer_change();
            true
        } else {
            false
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
        {
            let new_command = entry.command.clone();
            self.buffer.replace_buffer(new_command.as_str());
        }
        self.content_mode = ContentMode::Normal;
    }

    /// Show an error explaining that agent mode is not configured, with links to help resources.
    /// If the user has agent mode configured with a trigger prefix but no default (None-keyed)
    /// command, offer to prepend that prefix to the current buffer and launch agent mode.
    /// If no agent is configured at all, search the example file for a command that is available
    /// on the system and offer to run the corresponding `flyline set-agent-mode` command.
    fn show_agent_mode_not_configured_error(&mut self) {
        // Find a trigger-prefix-based command (a Some(prefix) key) if any exists.
        // Sort prefixes for deterministic selection.
        let prefix = self
            .settings
            .agent_commands
            .keys()
            .filter_map(|k| k.as_deref())
            .min();

        let (message, suggested_buffer, suggested_setup_command) = if let Some(prefix) = prefix {
            let suggested_buf = format!("{} {}", prefix, self.buffer.buffer());
            (
                format!(
                    "No default agent mode configured, but you have agent mode configured with trigger prefix \"{}\".",
                    prefix
                ),
                Some(suggested_buf),
                None,
            )
        } else {
            // No agent configured at all — try to find a suitable one from the example file.
            let setup_cmd = crate::agent_mode::parse_example_agent_commands()
                .into_iter()
                .find(|(cmd_name, _)| {
                    bash_funcs::get_command_info(cmd_name).0 != bash_funcs::CommandType::Unknown
                })
                .map(|(_, flyline_cmd)| flyline_cmd);

            (
                "Agent mode is not configured. Run `flyline set-agent-mode --help` or see https://github.com/HalFrgrd/flyline#agent-mode".to_string(),
                None,
                setup_cmd,
            )
        };
        self.content_mode = ContentMode::AgentError {
            message,
            raw_output: String::new(),
            suggested_buffer,
            suggested_setup_command,
        };
    }

    /// Resolve which agent command to use for Alt+Enter.
    /// First tries to find a trigger-prefix match, then falls back to the `None`-keyed default.
    fn resolve_agent_command(
        &self,
        needs_prefix: bool,
    ) -> Option<(settings::AgentModeCommand, String)> {
        log::info!(
            "Resolving agent command for buffer: '{}'",
            self.buffer.buffer()
        );
        let buf = self.buffer.buffer();
        for (prefix_key, agent_cmd) in &self.settings.agent_commands {
            if let Some(prefix) = prefix_key
                && let Some(stripped) = buf.strip_prefix(prefix.as_str())
            {
                return Some((agent_cmd.clone(), stripped.to_string()));
            }
        }
        if needs_prefix {
            return None;
        }

        let buf = self.buffer.buffer().to_string();
        self.settings
            .agent_commands
            .get(&None)
            .map(|cmd| (cmd.clone(), buf))
    }

    /// Spawn the configured AI command as a child process and transition to `AgentModeWaiting`.
    /// Words that contain a space are quoted with single quotes in the display string.
    /// If `buffer_str` is empty, opens the agent-prompts fuzzy history search instead.
    fn start_agent_mode(&mut self, agent_cmd: settings::AgentModeCommand, buffer_str: &str) {
        if false && buffer_str.is_empty() {
            // TODO think through UX for this
            // Warm with "" to display all agent prompts regardless of the current buffer.
            self.settings
                .agent_prompt_history_manager
                .warm_fuzzy_search_cache("");
            self.content_mode = ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts);
            return;
        }
        self.settings
            .agent_prompt_history_manager
            .push_entry(buffer_str.to_string());
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
                    suggested_buffer: None,
                    suggested_setup_command: None,
                };
            }
        }
    }

    /// Submit the current buffer if bash would accept it, otherwise insert a newline.
    /// If it's a single line complete command, exit.
    /// If it's a multi-line complete command, cursor needs to be at end to exit.
    fn try_submit_current_buffer(&mut self) {
        let should_submit_normally = ((self.buffer.lines_with_cursor().len() == 1)
            || self.buffer.is_cursor_at_trimmed_end())
            && command_acceptance::will_bash_accept_buffer(self.buffer.buffer());
        if self.unfinished_from_prev_command || should_submit_normally {
            self.mode =
                AppRunningState::Exiting(ExitState::WithCommand(self.buffer.buffer().to_string()));
        } else {
            self.buffer.insert_newline();
        }
    }

    fn on_possible_buffer_change(&mut self) {
        // Exit PromptCwdEdit mode if the cursor has moved away from position 0,
        // which happens when a buffer-modifying normal action fires (e.g. insert_char).
        if matches!(self.content_mode, ContentMode::PromptDirSelect(_))
            && self.buffer.cursor_byte_pos() != 0
        {
            self.content_mode = ContentMode::Normal;
        }

        // Cancel a pending tab-completion background thread when the word under
        // cursor has changed in a way that invalidates the in-flight completion.
        // Keep waiting if the new word is a prefix of the old one or vice-versa
        // (the user is just typing more characters or deleting some).
        if let ContentMode::TabCompletionWaiting {
            ref wuc_substring, ..
        } = self.content_mode
        {
            let buffer: &str = self.buffer.buffer();
            let completion_context = tab_completion_context::get_completion_context(
                buffer,
                self.buffer.cursor_byte_pos(),
            );
            let new_wuc = completion_context.word_under_cursor.s;
            let old_wuc = &wuc_substring.s;
            if !new_wuc.starts_with(old_wuc.as_str()) && !old_wuc.starts_with(&new_wuc) {
                self.content_mode = ContentMode::Normal;
            }
        }

        // Apply fuzzy filtering to active tab completion suggestions
        if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
            let buffer: &str = self.buffer.buffer();
            let completion_context = tab_completion_context::get_completion_context(
                buffer,
                self.buffer.cursor_byte_pos(),
            );
            let word_under_cursor = completion_context.word_under_cursor;
            if word_under_cursor.overlaps_with(&active_suggestions.word_under_cursor) {
                log::debug!(
                    "Word under cursor changed slightly ('{}' -> '{}'), applying fuzzy filter to tab completion suggestions",
                    active_suggestions.word_under_cursor.s,
                    word_under_cursor.s
                );
                active_suggestions.update_word_under_cursor(word_under_cursor);
            } else {
                log::debug!(
                    "Word under cursor changed significantly ('{:?}' -> '{:?}'), discarding tab completion suggestions",
                    active_suggestions.word_under_cursor,
                    word_under_cursor
                );
                // If the word under cursor has changed significantly, discard suggestions
                self.content_mode = ContentMode::Normal;
            }
        }

        let mut parser = dparser::DParser::from(self.buffer.buffer());
        parser.walk_to_end();
        let mut new_tokens = parser.into_tokens();
        if let Some(LastKeyPressAction::InsertedAutoClosing { char, byte_pos }) =
            self.last_keypress_action
        {
            // If the last keypress inserted an auto-closing char, mark the corresponding token in the new cache as auto-inserted.
            Self::mark_auto_inserted_closing(&mut new_tokens, char, byte_pos);
        }

        dparser::DParser::transfer_auto_inserted_flags(&self.dparser_tokens_cache, &mut new_tokens);
        // for token in &new_tokens {
        //     log::trace!("Parsed token '{:#?}", token);
        // }

        self.dparser_tokens_cache = new_tokens;

        let history_buffer = self.buffer_for_history().to_owned();
        self.inline_history_suggestion =
            if !self.settings.show_inline_history || history_buffer.is_empty() {
                None
            } else {
                self.history_manager
                    .get_command_suggestion_suffix(&history_buffer)
            };

        self.formatted_buffer_cache = format_buffer(
            &self.dparser_tokens_cache,
            self.buffer.cursor_byte_pos(),
            self.buffer.buffer().len(),
            self.mode.is_running(),
            &self.settings.color_palette,
        );

        self.tooltip = None;
        for part in self.formatted_buffer_cache.parts.iter() {
            if part
                .token
                .token
                .byte_range()
                .to_inclusive()
                .contains(&self.buffer.cursor_byte_pos())
                && let Some(tooltip) = part.tooltip.as_ref()
            {
                self.tooltip = Some(tooltip.clone());
            }
        }

        // log::debug!("Formatted buffer cache updated:\n{:#?}", self.formatted_buffer_cache);
    }

    /// Returns the buffer string with any trailing auto-inserted closing tokens stripped.
    /// This is the string that should be used when searching history.
    fn buffer_for_history(&self) -> &str {
        // TODO: figure out good UX for this
        // dparser::DParser::buffer_without_auto_inserted_suffix(
        //     &self.dparser_tokens_cache,
        //     self.buffer.buffer(),
        // )
        self.buffer.buffer()
    }

    /// Build the display lines for a single fuzzy-history entry.
    ///
    /// Returns one `Line` per terminal row. The first line combines the
    /// header prefix (index / score / timeago / indicator) with the first
    /// command row; subsequent lines carry the continuation prefix.
    fn get_lines_for_history_entry(
        formatted_entry: &HistoryEntryFormatted,
        entries: &[HistoryEntry],
        entry_idx: usize,
        fuzzy_search_index: usize,
        num_digits_for_index: usize,
        num_digits_for_score: usize,
        header_prefix_width: usize,
        available_cols: u16,
        palette: &Palette,
    ) -> Vec<Line<'static>> {
        let is_selected = fuzzy_search_index == entry_idx;

        let entry = &entries[formatted_entry.entry_index];
        let timeago_str = entry
            .timestamp
            .map(ts_to_timeago_string_5chars)
            .unwrap_or_else(|| "     ".to_string());

        let indicator_span = || {
            if is_selected {
                Span::styled(
                    "▐",
                    palette
                        .matching_char()
                        .remove_modifier(Modifier::UNDERLINED),
                )
            } else {
                Span::styled(" ", palette.secondary_text())
            }
        };

        let formatted_text = formatted_entry.command_spans(entries, palette);

        let total_logical_lines = formatted_text.len();
        let mut all_display_rows: Vec<(bool, usize, Line<'static>)> = vec![];
        for (logical_idx, logical_line) in formatted_text.iter().enumerate() {
            let terminal_rows = split_line_to_terminal_rows(logical_line, available_cols);
            for (sub_idx, terminal_row) in terminal_rows.into_iter().enumerate() {
                all_display_rows.push((sub_idx == 0, logical_idx, terminal_row));
            }
        }

        let total_display_rows = all_display_rows.len();
        let max_display_rows = if is_selected { 4 } else { 1 };
        let has_more = total_display_rows > max_display_rows;
        let rows_to_show = total_display_rows.min(max_display_rows);

        let mut result: Vec<Line<'static>> = Vec::with_capacity(rows_to_show);

        for (display_idx, (is_start_of_logical, logical_idx, display_line)) in all_display_rows
            .into_iter()
            .take(max_display_rows)
            .enumerate()
        {
            let mut row_spans: Vec<Span<'static>> = if display_idx == 0 {
                vec![
                    Span::styled(
                        format!("{:>num_digits_for_index$} ", entry.index + 1),
                        palette.secondary_text(),
                    ),
                    Span::styled(
                        format!("{:>num_digits_for_score$} ", formatted_entry.score),
                        palette.secondary_text(),
                    ),
                    Span::styled(timeago_str.clone(), palette.secondary_text()),
                    indicator_span(),
                ]
            } else {
                let indent_prefix = if is_start_of_logical {
                    let line_num_str = format!("{}/{}", logical_idx + 1, total_logical_lines);
                    format!("{:>width$}", line_num_str, width = header_prefix_width - 1)
                } else {
                    " ".repeat(header_prefix_width - 1)
                };
                vec![
                    Span::styled(indent_prefix, palette.secondary_text()),
                    indicator_span(),
                ]
            };

            let cmd_display_width: usize = display_line
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();

            let mut cmd_spans: Vec<Span<'static>> = display_line
                .spans
                .into_iter()
                .map(|span| {
                    if is_selected {
                        Span::styled(span.content, Palette::convert_to_selected(span.style))
                    } else {
                        span
                    }
                })
                .collect();

            // Append ellipsis on the last displayed row when more content exists.
            // If the command row fills available_cols, trim the last grapheme to
            // make space; otherwise just append.
            if display_idx + 1 == rows_to_show && has_more {
                let ellipsis_style = if is_selected {
                    Palette::convert_to_selected(palette.secondary_text())
                } else {
                    palette.secondary_text()
                };
                if cmd_display_width >= available_cols as usize {
                    'trim: loop {
                        match cmd_spans.last_mut() {
                            None => break 'trim,
                            Some(last) => {
                                let s = last.content.as_ref();
                                match s.grapheme_indices(true).next_back() {
                                    None => {
                                        cmd_spans.pop();
                                    }
                                    Some((byte_idx, _)) => {
                                        let trimmed = s[..byte_idx].to_string();
                                        let style = last.style;
                                        if trimmed.is_empty() {
                                            cmd_spans.pop();
                                        } else {
                                            *last = Span::styled(trimmed, style);
                                        }
                                        break 'trim;
                                    }
                                }
                            }
                        }
                    }
                }
                cmd_spans.push(Span::styled("…", ellipsis_style));
            }

            row_spans.extend(cmd_spans);
            result.push(Line::from(row_spans));
        }

        result
    }

    fn create_content(&mut self, width: u16, viewport_top: u16, terminal_height: u16) -> Contents {
        // Basically build the entire frame in a Content first
        // Then figure out how to fit that into the actual frame area
        let mut content = Contents::new(width);

        // When terminal log streaming is enabled, show the last 20 log lines at
        // the top of the content before anything else.
        if crate::logging::is_terminal_streaming() {
            let log_lines = crate::logging::last_n_logs(20);
            for line_text in log_lines {
                let tagged_line = TaggedLine::from(vec![TaggedSpan::new(
                    ratatui::text::Span::raw(line_text),
                    Tag::Normal,
                )]);
                content.write_tagged_line(&tagged_line, true);
            }
        }

        // Render tutorial text above the prompt when a tutorial step is active.
        if self.mode.is_running() {
            if self.settings.tutorial_step == tutorial::TutorialStep::Welcome {
                // Welcome step: draw the large block-art logo, then overlay the
                // animated action prompt in the lower-right of the logo.
                let logo_lines = crate::tutorial::generate_welcome_logo_lines(width);
                for line in logo_lines {
                    content.write_tagged_line(&TaggedLine::from_line(line, Tag::Tutorial), true);
                }

                // Move to the second-to-last logo row, column 30, and overwrite
                // with the wave-animated "Press enter to start the tutorial" text.
                let second_to_last = content.height().saturating_sub(2);
                content.move_cursor_to(second_to_last, 30);
                let action_line = crate::tutorial::generate_welcome_action_line();
                content
                    .write_tagged_line(&TaggedLine::from_line(action_line, Tag::Tutorial), false);

                content.move_to_final_line();
                content.newline();
            } else if let Some(tutorial_tagged_lines) = crate::tutorial::generate_tutorial_text(
                self.settings.tutorial_step,
                &self.settings.color_palette,
            ) {
                const BUTTON_HEIGHT: u16 = 30;

                let layout = Layout::horizontal([
                    Constraint::Min(7),
                    Constraint::Percentage(90),
                    Constraint::Min(7),
                ]);

                let tutorial_start_row = content.height();

                let [prev_block, text_block_outer, next_block] = Rect {
                    x: 0,
                    y: tutorial_start_row,
                    width,
                    height: BUTTON_HEIGHT,
                }
                .layout(&layout);

                let text_block = text_block_outer.inner(Margin {
                    horizontal: 2,
                    vertical: 0,
                });

                // Allocate rows for the buttons before drawing them.
                // `increase_buf_single_row` grows the buffer one row at a time, which is
                // its existing public API for incremental allocation.
                while content.buf.len() < (tutorial_start_row + BUTTON_HEIGHT) as usize {
                    content.increase_buf_single_row();
                }

                // Draw prev and next buttons first.
                content.render_block(
                    prev_block,
                    "prev",
                    Tag::TutorialPrev,
                    self.last_mouse_over_cell == Some(Tag::TutorialPrev),
                );
                content.render_block(
                    next_block,
                    "next",
                    Tag::TutorialNext,
                    self.last_mouse_over_cell == Some(Tag::TutorialNext),
                );

                // Collect clipboard content from tagged spans.
                for tagged_line in &tutorial_tagged_lines {
                    for tagged_span in &tagged_line.spans {
                        if let SpanTag::Constant(Tag::Clipboard(cb_type)) = &tagged_span.tag {
                            content.setup_clipboard(*cb_type, tagged_span.span.content.to_string());
                        }
                    }
                }

                // Move cursor to the start of the text area and write tutorial
                // lines using overwrite=false so the text sits between the buttons.
                content.move_cursor_to(tutorial_start_row, text_block.x);

                let mut text_end_row = tutorial_start_row;
                for tagged_line in &tutorial_tagged_lines {
                    for tagged_span in &tagged_line.spans {
                        // If the mouse is hovering over a clipboard-tagged span,
                        // apply the highlight (reversed) style to it.
                        let is_hovered = if let (
                            SpanTag::Constant(Tag::Clipboard(span_cb)),
                            Some(Tag::Clipboard(hover_cb)),
                        ) = (&tagged_span.tag, self.last_mouse_over_cell)
                        {
                            span_cb == &hover_cb
                        } else {
                            false
                        };
                        if is_hovered {
                            content.write_tagged_span_dont_overwrite(
                                &tagged_span.clone().convert_to_highlighted(),
                                None,
                            );
                        } else {
                            content.write_tagged_span_dont_overwrite(tagged_span, None);
                        }
                    }
                    text_end_row = content.cursor_position().row;
                    content.newline();
                    content.set_cursor_col(text_block.x);
                }

                // Delete the empty rows between where the text ends and where
                // the buttons end. We keep the last row of the button area
                // (BUTTON_HEIGHT - 1) because it holds the bottom border of
                // the prev/next blocks, making them look visually complete.
                let drain_start = (text_end_row + 1) as usize;
                let buttons_bottom_border = (tutorial_start_row + BUTTON_HEIGHT - 1) as usize;
                if drain_start < buttons_bottom_border {
                    content.buf.drain(drain_start..buttons_bottom_border);
                }

                content.move_to_final_line();
                content.newline();
            }
        }

        content.prompt_start = Some(content.cursor_position());

        let (mut lprompt, rprompt, fill_span) = self
            .prompt_manager
            .get_ps1_lines(self.settings.show_animations, self.mouse_state.enabled());

        // When in PromptCwdEdit mode, highlight the selected CWD path segment.
        if let ContentMode::PromptDirSelect(cwd_index) = self.content_mode {
            for line in &mut lprompt {
                for span in &mut line.spans {
                    if span.tag == SpanTag::Constant(Tag::Ps1PromptCwd(cwd_index)) {
                        span.span.style = Palette::convert_to_selected(span.span.style);
                    }
                }
            }
        }

        let empty_tagged_line = TaggedLine::default();
        for (_, is_last, either_or_both) in
            lprompt.iter().zip_longest(rprompt.iter()).flag_first_last()
        {
            let (tagged_l, tagged_r) = either_or_both.or(&empty_tagged_line, &empty_tagged_line);
            if is_last {
                content.write_tagged_line_lrjustified(
                    tagged_l,
                    &TaggedLine::from_line(Line::from(" "), Tag::Ps1Prompt),
                    tagged_r,
                    true,
                );
            } else {
                content.write_tagged_line_lrjustified(tagged_l, &fill_span, tagged_r, false);
            }
            if !is_last {
                content.newline();
            }
        }

        content.prompt_end = Some(content.cursor_position());

        let mut line_idx = 0;
        let mut cursor_pos_maybe = None;

        let now = std::time::Instant::now();

        for part in self.formatted_buffer_cache.parts.iter() {
            let span_to_draw = if part.token.token.kind == TokenKind::Newline {
                // For newlines, draw a space instead so that we can have a place to put the cursor
                Span::from(" ")
            } else if self.mode.is_running() && self.settings.show_animations {
                part.get_possible_animated_span(now)
            } else {
                part.normal_span().clone()
            };

            let graph_idx_to_tag: Vec<Tag> = part
                .normal_span()
                .content
                .graphemes(true)
                .scan(part.token.token.byte_range().start, |acc, graph| {
                    let tag = Tag::Command(*acc);
                    *acc += graph.len();
                    Some(tag)
                })
                .collect();

            let poss_cursor_anim_pos = content.write_tagged_span_dont_overwrite(
                &TaggedSpan::per_grapheme(span_to_draw, graph_idx_to_tag),
                part.cursor_grapheme_idx,
            );
            if cursor_pos_maybe.is_none() {
                cursor_pos_maybe = poss_cursor_anim_pos;
            }

            if part.token.token.kind == TokenKind::Newline {
                line_idx += 1;
                content.newline();
                let ps2 = Span::styled(
                    format!("{}∙", line_idx + 1),
                    self.settings.color_palette.secondary_text(),
                );
                content.write_tagged_span(&TaggedSpan::new(ps2, Tag::Ps2Prompt));
            }
        }
        if self.formatted_buffer_cache.draw_cursor_at_end {
            let space = StyledGrapheme::new(" ", Style::default());
            content.move_to_next_insertion_point(&space, false);
            cursor_pos_maybe = Some(content.cursor_position());
        }

        if matches!(
            self.mode,
            AppRunningState::Exiting(ExitState::WithoutCommand)
        ) {
            content.write_tagged_span(&TaggedSpan::new(
                Span::styled(
                    "^C",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Tag::Normal,
            ));
        }

        if self.mode.is_running()
            && let Some(cursor_pos) = cursor_pos_maybe
        {
            self.cursor.update_logical_pos(cursor_pos);
            let cursor_render_pos = if self.settings.show_animations {
                self.cursor.get_render_pos(&self.settings.cursor_config)
            } else {
                cursor_pos
            };
            let cursor_style = {
                if self.settings.cursor_config.backend == CursorBackend::Terminal {
                    None
                } else if self.settings.show_animations {
                    let focused = self.term_has_focus
                        && !matches!(self.content_mode, ContentMode::PromptDirSelect(_))
                        && self.last_activity_time.elapsed() < IDLE_TIMEOUT;
                    self.cursor.get_style(focused, &self.settings.cursor_config)
                } else {
                    Some(Palette::cursor_style(255))
                }
            };

            content.set_term_cursor_pos(cursor_render_pos, cursor_style);
        }

        if let Some((sug, suf)) = &self.inline_history_suggestion
            && self.mode.is_running()
        {
            suf.lines()
                .collect::<Vec<_>>()
                .iter()
                .flag_first_last()
                .for_each(|(is_first, is_last, line)| {
                    if !is_first {
                        content.newline();
                    }

                    content.write_tagged_span_dont_overwrite(
                        &TaggedSpan::new(
                            Span::from(line.to_owned())
                                .style(self.settings.color_palette.secondary_text()),
                            Tag::HistorySuggestion,
                        ),
                        None,
                    );

                    if is_last {
                        let mut extra_info_text = format!(" #idx={}", sug.index);
                        if let Some(ts) = sug.timestamp {
                            let time_ago_str = ts_to_timeago_string_5chars(ts);
                            extra_info_text.push_str(&format!(" {}", time_ago_str.trim_start()));
                        }

                        content.write_tagged_span_dont_overwrite(
                            &TaggedSpan::new(
                                Span::from(extra_info_text)
                                    .style(self.settings.color_palette.inline_suggestion()),
                                Tag::HistorySuggestion,
                            ),
                            None,
                        );

                        if self.settings.run_tutorial {
                            content.write_tagged_span_dont_overwrite(
                                &TaggedSpan::new(
                                    Span::styled(
                                        " 💡 Press → or End to accept",
                                        self.settings.color_palette.tutorial_hint(),
                                    ),
                                    Tag::Tutorial,
                                ),
                                None,
                            );
                        }
                    }
                });
        }

        let rows_before = content.cursor_position().row;
        let rows_left_before_end_of_screen: u16 = terminal_height.saturating_sub(rows_before + 1);

        // Pre-extract the fuzzy history source (owned) before the mutable match below,
        // so we can still access other fields (e.g. individual history managers) inside
        // the FuzzyHistorySearch arm without borrow-checker conflicts.
        let fuzzy_source_for_render: Option<FuzzyHistorySource> = match &self.content_mode {
            ContentMode::FuzzyHistorySearch(s) => Some(s.clone()),
            _ => None,
        };

        match &mut self.content_mode {
            ContentMode::TabCompletion(active_suggestions) if self.mode.is_running() => {
                content.newline();

                // Early exit when there are no suggestions to display.
                if active_suggestions.filtered_suggestions_len() == 0 {
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(
                            "No suggestions",
                            self.settings.color_palette.secondary_text(),
                        ),
                        Tag::TabSuggestion,
                    ));
                } else {
                    let grid_start_row = content.cursor_position().row;
                    let num_rows_for_suggestions = rows_left_before_end_of_screen.clamp(2, 15);

                    let mut selected_grid_row: Option<u16> = None;

                    let grid = active_suggestions.into_grid(
                        num_rows_for_suggestions as usize,
                        width as usize,
                        &self.settings.color_palette,
                    );

                    for row_idx in 0..grid[0].items.len() {
                        for (is_first, _, col) in grid.iter().flag_first_last() {
                            if let Some((formatted, is_selected)) = col.items.get(row_idx) {
                                if !is_first {
                                    content.write_tagged_span(&TaggedSpan::new(
                                        Span::raw(" ".repeat(COLUMN_PADDING)),
                                        Tag::TabSuggestion,
                                    ));
                                }
                                let formatted_suggestion =
                                    formatted.render(col.width, *is_selected);
                                let tag = Tag::Suggestion(formatted.suggestion_idx);
                                for span in formatted_suggestion {
                                    content.write_tagged_span(&TaggedSpan::new(span, tag));
                                }
                                if *is_selected && selected_grid_row.is_none() {
                                    selected_grid_row = Some(row_idx as u16);
                                }
                            }
                        }
                        content.newline();
                    }

                    if let Some(sel_row) = selected_grid_row {
                        content.set_focus_row(grid_start_row + sel_row);
                    }
                }
            }
            ContentMode::TabCompletionWaiting { .. } if self.mode.is_running() => {
                content.newline();
                content.write_tagged_span(&TaggedSpan::new(
                    Span::styled(
                        "Loading completions…",
                        self.settings.color_palette.secondary_text(),
                    ),
                    Tag::Normal,
                ));
            }
            ContentMode::FuzzyHistorySearch(_) if self.mode.is_running() => {
                let source = fuzzy_source_for_render.as_ref().unwrap();
                let num_rows_footer = 1;
                let num_rows_for_results = rows_left_before_end_of_screen
                    .saturating_sub(num_rows_footer)
                    .clamp(2, 30);

                let history_buffer = self.buffer_for_history().to_owned();
                // Use explicit field borrows instead of `select_fuzzy_history_manager_mut` to allow
                // split-borrowing: `fuzzy_results` borrows only the specific manager field while
                // `self.settings.color_palette` (a different field) remains accessible below.
                let (entries, fuzzy_results, fuzzy_search_index, num_results, num_searched) =
                    match source {
                        FuzzyHistorySource::PastCommands => &mut self.history_manager,
                        FuzzyHistorySource::CancelledCommands => {
                            &mut self.settings.cancelled_command_history_manager
                        }
                        FuzzyHistorySource::AgentPrompts => {
                            &mut self.settings.agent_prompt_history_manager
                        }
                    }
                    .get_fuzzy_search_results(&history_buffer, num_rows_for_results as usize);

                let starting_row = content.cursor_position().row;

                let num_digits_for_index = num_searched.to_string().len();
                let num_digits_for_score = 3.max(
                    fuzzy_results
                        .iter()
                        .map(|r| r.score.to_string().len())
                        .max()
                        .unwrap_or(0),
                );
                let timeago_width = 5; // ts_to_timeago_string_5chars always returns 5 chars
                let indicator_width = 1; // "▐" or " "
                // Width of the header prefix: "{index} {score} {timeago}{indicator}"
                let header_prefix_width = (num_digits_for_index + 1)
                    + (num_digits_for_score + 1)
                    + timeago_width
                    + indicator_width;
                let available_cols = content.width.saturating_sub(header_prefix_width as u16);
                'outer: for formatted_entry in fuzzy_results.iter() {
                    let entry_idx = formatted_entry.idx_in_cache.unwrap_or(0);
                    let is_selected = fuzzy_search_index == entry_idx;
                    if is_selected {
                        content.set_focus_row(content.cursor_position().row);
                    }
                    for line in Self::get_lines_for_history_entry(
                        formatted_entry,
                        entries,
                        entry_idx,
                        fuzzy_search_index,
                        num_digits_for_index,
                        num_digits_for_score,
                        header_prefix_width,
                        available_cols,
                        &self.settings.color_palette,
                    ) {
                        content.newline();
                        content.write_tagged_line(
                            &TaggedLine::from_line(line, Tag::HistoryResult(entry_idx)),
                            false,
                        );
                        content.fill_line(Tag::HistoryResult(entry_idx));
                        if content.cursor_position().row.saturating_sub(starting_row)
                            >= num_rows_for_results
                        {
                            break 'outer;
                        }
                    }
                }
                content.newline();
                content.write_tagged_span(&TaggedSpan::new(
                    Span::styled(
                        format!("# {}: {}/{}", source.label(), num_results, num_searched),
                        self.settings.color_palette.secondary_text(),
                    ),
                    Tag::FuzzySearch,
                ));
            }
            ContentMode::Normal if self.mode.is_running() => {
                if let Some(tooltip) = &self.tooltip {
                    content.newline();
                    let tooltip_line = Line::from(Span::styled(
                        tooltip.clone(),
                        self.settings.color_palette.secondary_text(),
                    ));

                    let max_tool_tip_rows: u16 = 3;

                    let rows = split_line_to_terminal_rows(&tooltip_line, content.width);
                    let truncated = rows.len() > max_tool_tip_rows as usize;
                    for (i, row) in rows
                        .into_iter()
                        .take(max_tool_tip_rows as usize)
                        .enumerate()
                    {
                        if i > 0 {
                            content.newline();
                        }
                        for span in &row.spans {
                            content.write_tagged_span(&TaggedSpan::new(span.clone(), Tag::Tooltip));
                        }
                    }
                    if truncated && max_tool_tip_rows > 0 {
                        let last_col = content.width.saturating_sub(1);
                        if content.cursor_position().col >= last_col {
                            content.set_cursor_col(last_col);
                        }
                        content.write_tagged_span(&TaggedSpan::new(
                            Span::styled("…", self.settings.color_palette.secondary_text()),
                            Tag::Tooltip,
                        ));
                    }
                }
            }
            ContentMode::AgentModeWaiting {
                command_display,
                start_time,
                ..
            } if self.mode.is_running() => {
                content.newline();
                let elapsed_secs = start_time.elapsed().as_secs();
                content.write_tagged_span(&TaggedSpan::new(
                    Span::styled(
                        format!("Running: {} [{}s]", command_display, elapsed_secs),
                        self.settings.color_palette.secondary_text(),
                    ),
                    Tag::Normal,
                ));
            }
            ContentMode::AgentOutputSelection(selection) if self.mode.is_running() => {
                content.newline();
                for line in &selection.header_text {
                    content
                        .write_tagged_line(&TaggedLine::from_line(line.clone(), Tag::Normal), true);
                }
                for (row_idx, suggestion) in selection.suggestions.iter().enumerate() {
                    let is_selected = selection.selected_idx == row_idx;
                    if is_selected {
                        content.set_focus_row(content.cursor_position().row);
                    }
                    let indicator = if is_selected { "▐" } else { " " };
                    let indicator_style = if is_selected {
                        self.settings
                            .color_palette
                            .matching_char()
                            .remove_modifier(Modifier::UNDERLINED)
                    } else {
                        self.settings.color_palette.secondary_text()
                    };
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(indicator, indicator_style),
                        Tag::AiResult(row_idx),
                    ));
                    // Description line
                    let desc_style = if is_selected {
                        Palette::convert_to_selected(self.settings.color_palette.secondary_text())
                    } else {
                        self.settings.color_palette.secondary_text()
                    };
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(suggestion.description.clone(), desc_style),
                        Tag::AiResult(row_idx),
                    ));
                    content.fill_line(Tag::AiResult(row_idx));
                    content.newline();
                    // Command line: gutter char + syntax-highlighted command via dparser
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(indicator, indicator_style),
                        Tag::AiResult(row_idx),
                    ));
                    let cmd = &suggestion.command;
                    let mut parser = dparser::DParser::from(cmd.as_str());
                    parser.walk_to_end();
                    let tokens = parser.tokens().to_vec();
                    // cursor_byte_pos=cmd.len() (past end), buffer_byte_length=cmd.len(),
                    // app_is_running=false (no cursor/pair highlighting).
                    let formatted_cmd = format_buffer(
                        &tokens,
                        cmd.len(),
                        cmd.len(),
                        false,
                        &self.settings.color_palette,
                    );
                    for part in &formatted_cmd.parts {
                        if matches!(part.token.token.kind, TokenKind::Newline) {
                            continue;
                        }
                        let span = part.normal_span();
                        let styled_span = if is_selected {
                            Span::styled(
                                span.content.clone(),
                                Palette::convert_to_selected(span.style),
                            )
                        } else {
                            span.clone()
                        };
                        content.write_tagged_span(&TaggedSpan::new(
                            styled_span,
                            Tag::AiResult(row_idx),
                        ));
                    }
                    content.fill_line(Tag::AiResult(row_idx));
                    content.newline();
                }
                for line in &selection.footer_text {
                    content
                        .write_tagged_line(&TaggedLine::from_line(line.clone(), Tag::Normal), true);
                }
            }
            ContentMode::AgentError {
                message,
                raw_output,
                suggested_buffer,
                suggested_setup_command,
            } if self.mode.is_running() => {
                content.newline();
                content.write_tagged_span(&TaggedSpan::new(
                    Span::styled(message.clone(), Style::default().fg(Color::Red)),
                    Tag::Normal,
                ));
                if let Some(suggested) = suggested_buffer {
                    content.newline();
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(
                            format!("Buffer with prefix: {}", suggested),
                            self.settings.color_palette.secondary_text(),
                        ),
                        Tag::Normal,
                    ));
                    content.newline();
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(
                            "Press Enter to launch agent mode with this buffer.",
                            self.settings.color_palette.secondary_text(),
                        ),
                        Tag::Blank,
                    ));
                } else {
                    if !raw_output.is_empty() {
                        for line in raw_output.lines().take(5) {
                            content.newline();
                            content.write_tagged_span(&TaggedSpan::new(
                                Span::styled(
                                    line.to_string(),
                                    self.settings.color_palette.secondary_text(),
                                ),
                                Tag::Normal,
                            ));
                        }
                    }
                    content.newline();
                    let hint = if let Some(setup_cmd) = suggested_setup_command {
                        format!("Press Enter to run `{}`.", setup_cmd)
                    } else {
                        "Press Enter to run `flyline set-agent-mode --help`.".to_string()
                    };
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(hint, self.settings.color_palette.secondary_text()),
                        Tag::Blank,
                    ));
                }
            }
            _ => {}
        }

        let show_matrix = self.mode.is_running()
            && match &self.settings.matrix_animation {
                MatrixAnimation::Off => false,
                MatrixAnimation::On => true,
                MatrixAnimation::IdleSecs(secs) => {
                    self.last_activity_time.elapsed().as_secs() >= *secs
                }
            };
        if show_matrix {
            content.apply_matrix_anim(now, viewport_top, terminal_height);
        }

        if !self.mode.is_running() {
            content.move_to_final_line();
            content.newline();
            let cursor_pos = content.cursor_position();
            content.set_term_cursor_pos(cursor_pos, None);
            content.set_focus_row(cursor_pos.row);
        }

        content
    }

    fn ui(&mut self, frame: &mut Frame, content: Contents) {
        let frame_area = frame.area();
        frame.buffer_mut().reset();

        let content_visible_row_range = content.get_row_range_to_show(frame_area.height);

        for row_idx in 0..frame_area.height {
            match content
                .buf
                .get((content_visible_row_range.start + row_idx) as usize)
            {
                Some(row) => {
                    for (x, tagged_cell) in row.iter().enumerate() {
                        if x < frame_area.width as usize {
                            frame.buffer_mut().content
                                [row_idx as usize * frame_area.width as usize + x] =
                                tagged_cell.cell.clone();
                        }
                    }
                }
                None => break,
            };
        }

        let drawn_content = DrawnContent {
            contents: content,
            viewport_start: frame_area.y,
            content_visible_row_range,
        };

        if let Some(term_em_cursor) = drawn_content.term_em_cursor_pos()
            && (self.settings.cursor_config.backend == CursorBackend::Terminal
                || !self.mode.is_running())
        {
            frame.set_cursor_position(term_em_cursor);
        }

        self.last_contents = Some(drawn_content);
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
