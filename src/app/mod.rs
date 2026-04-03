pub(crate) mod actions;
mod auto_close;
mod formated_buffer;
mod tab_completion;

use crate::active_suggestions::{ActiveSuggestions, COLUMN_PADDING};
use crate::agent_mode::{AiOutputSelection, parse_ai_output};
use crate::app::formated_buffer::{FormattedBuffer, format_buffer};
use crate::content_builder::{Contents, Tag, split_line_to_terminal_rows};
use crate::cursor_animation::CursorAnimation;
use crate::dparser::{AnnotatedToken, ToInclusiveRange};
use crate::history::{HistoryEntry, HistoryEntryFormatted, HistoryManager};
use crate::iter_first_last::FirstLast;
use crate::mouse_state::MouseState;
use crate::palette::Palette;
use crate::prompt_manager::PromptManager;
use crate::settings::{self, MouseMode, Settings};
use crate::text_buffer::{SubString, TextBuffer};
use crate::{bash_funcs, dparser};
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

const TUTORIAL_FUZZY_SEARCH_HINT: &str = "💡 Type to search, press arrow keys / Page Up/Down to browse, Enter to run the command, Alt+Enter to accept the command for editing";
const TUTORIAL_HISTORY_PREFIX_HINT: &str =
    "💡 ↑/↓ to scroll through history entries whose prefix matches your current command";
const TUTORIAL_DISABLE_HINT: &str =
    "💡 Run `flyline --tutorial-mode false` to disable tutorial mode";

fn build_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn restore_terminal() {
    crossterm::terminal::disable_raw_mode().unwrap();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableBracketedPaste,
        crossterm::event::DisableFocusChange,
        crossterm::event::DisableMouseCapture,
        crossterm::event::PopKeyboardEnhancementFlags
    );
}

fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
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
    set_panic_hook();

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
        crossterm::event::PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
    .unwrap();

    let runtime = build_runtime();

    let end_state = runtime.block_on(App::new(settings).run(backend));

    restore_terminal();

    log::debug!("Final state: {:?}", end_state);
    end_state
}

#[derive(Debug)]
enum ContentMode {
    Normal,
    FuzzyHistorySearch,
    TabCompletion(Box<ActiveSuggestions>),
    /// AI command is running in the background. Stores the channel receiver and the
    /// human-readable representation of the command being executed.
    AgentModeWaiting {
        receiver: std::sync::mpsc::Receiver<Result<String, (String, String)>>,
        command_display: String,
        start_time: std::time::Instant,
    },
    /// AI output has been parsed; user is selecting a suggestion from the list.
    AgentOutputSelection(AiOutputSelection),
    /// AI command or JSON parsing failed; stores the error message and any raw output.
    /// When `suggested_buffer` is set, the error is a "no default agent but prefix-only config"
    /// case: pressing Enter will launch agent mode with that buffer instead of running help.
    AgentError {
        message: String,
        raw_output: String,
        suggested_buffer: Option<String>,
    },
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
                Tag::Command(_) | Tag::Suggestion(_) | Tag::HistoryResult(_) | Tag::AiResult(_)
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
    cursor_animation: CursorAnimation,
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
    settings: &'a Settings,
    /// Terminal row (absolute) where the inline viewport starts; used by smart mouse mode.
    /// Timestamp of the last draw operation.
    last_draw_time: std::time::Instant,
    needs_screen_cleared: bool,
    last_keypress_action: Option<LastKeyPressAction>,
}

impl<'a> App<'a> {
    fn new(settings: &'a Settings) -> Self {
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

        App {
            mode: AppRunningState::Running,
            buffer,
            formatted_buffer_cache,
            dparser_tokens_cache: Vec::new(),
            cursor_animation: CursorAnimation::new(),
            unfinished_from_prev_command,
            prompt_manager: PromptManager::new(
                unfinished_from_prev_command,
                &settings
                    .custom_animations
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

        // Send execution finished escape codes (previous command has completed).
        if self.settings.send_shell_integration_codes {
            let last_command_exit_value = unsafe { crate::bash_symbols::last_command_exit_value };
            let hostname = bash_funcs::get_hostname();
            let cwd = bash_funcs::get_cwd();

            shell_integration::write_startup_codes(last_command_exit_value, &hostname, &cwd)
                .unwrap_or_else(|e| {
                    log::error!("Failed to write execution finished escape codes: {}", e);
                });
        }

        crossterm::terminal::enable_raw_mode().unwrap();

        let options = TerminalOptions {
            viewport: Viewport::Inline(0),
        };
        let mut terminal =
            ratatui::Terminal::with_options(backend, options).expect("Failed to create terminal");

        bash_symbols::set_readline_state(bash_symbols::RL_STATE_TERMPREPPED);

        let mut redraw = true;
        let mut last_terminal_size = terminal.size().unwrap();

        'main_loop: loop {
            // Poll AI background task: check if a result has arrived without blocking.
            let ai_result =
                if let ContentMode::AgentModeWaiting { ref receiver, .. } = self.content_mode {
                    match receiver.try_recv() {
                        Ok(result) => Some(result),
                        Err(std::sync::mpsc::TryRecvError::Empty) => None,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            log::warn!("AI task channel disconnected unexpectedly");
                            Some(Err(("AI task disconnected".to_string(), String::new())))
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
                            };
                        }
                    },
                    Err((msg, raw_output)) => {
                        log::error!("AI command failed: {}", msg);
                        self.content_mode = ContentMode::AgentError {
                            message: msg,
                            raw_output,
                            suggested_buffer: None,
                        };
                    }
                }
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
                match terminal.draw(|f| self.ui(f, content)) {
                    Ok(_) => {
                        self.last_draw_time = std::time::Instant::now();

                        if self.settings.send_shell_integration_codes {
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

            let min_refresh_rate: Duration =
                Duration::from_millis((1000.0 / (self.settings.frame_rate as f64)) as u64);

            redraw = if event::poll(min_refresh_rate).unwrap() {
                match event::read().unwrap() {
                    CrosstermEvent::Key(key) => {
                        self.handle_key_event(key);
                        true
                    }
                    CrosstermEvent::Mouse(mouse) => self.on_mouse(mouse),
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
                        self.cursor_animation.term_has_focus = false;
                        false
                    }
                    CrosstermEvent::FocusGained => {
                        // log::trace!("Terminal focus gained");
                        self.cursor_animation.term_has_focus = true;
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
                // Bash will set this to a function when it receives a terminating signal.
                // The function is readline specific so we don't call it here.
                // But the act of it being set is a signal that we should exit immediately
                if let Some(_) = crate::bash_symbols::rl_signal_event_hook {
                    let sig = crate::bash_symbols::terminating_signal;

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
                if self.settings.send_shell_integration_codes {
                    shell_integration::write_on_exit_codes(Some(&cmd)).unwrap_or_else(|e| {
                        log::error!("Failed to write pre-execution escape codes: {}", e);
                    });
                }

                log::info!("Exiting with command: {}", cmd);
                ExitState::WithCommand(cmd)
            }
            _ => {
                if self.settings.send_shell_integration_codes {
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
                if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) {
                    self.history_manager.fuzzy_search_set_idx(idx);
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

            t => {
                log::trace!("Mouse over  {:?}", t);
                self.last_mouse_over_cell = None;
            }
        }

        let mut update_buffer = false;

        match self.last_mouse_over_cell {
            Some(Tag::Suggestion(idx)) => {
                if matches!(mouse.kind, MouseEventKind::Up(_))
                    && let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode
                {
                    active_suggestions.set_selected_by_idx(idx);
                    active_suggestions.accept_currently_selected(&mut self.buffer);
                    self.content_mode = ContentMode::Normal;
                    update_buffer = true;
                }
            }
            Some(Tag::HistoryResult(idx)) => {
                if matches!(mouse.kind, MouseEventKind::Up(_))
                    && matches!(self.content_mode, ContentMode::FuzzyHistorySearch)
                {
                    self.history_manager.fuzzy_search_set_idx(idx);
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
        if let Some(entry) = self.history_manager.accept_fuzzy_search_result() {
            let new_command = entry.command.clone();
            self.buffer.replace_buffer(new_command.as_str());
        }
        self.content_mode = ContentMode::Normal;
    }

    /// Show an error explaining that agent mode is not configured, with links to help resources.
    /// If the user has agent mode configured with a trigger prefix but no default (None-keyed)
    /// command, offer to prepend that prefix to the current buffer and launch agent mode.
    fn show_agent_mode_not_configured_error(&mut self) {
        // Find a trigger-prefix-based command (a Some(prefix) key) if any exists.
        // Sort prefixes for deterministic selection.
        let prefix = self
            .settings
            .agent_commands
            .keys()
            .filter_map(|k| k.as_deref())
            .min();

        let (message, suggested_buffer) = if let Some(prefix) = prefix {
            let suggested_buf = format!("{}{}", prefix, self.buffer.buffer());
            (
                format!(
                    "No default agent mode configured, but you have agent mode configured with trigger prefix \"{}\".",
                    prefix
                ),
                Some(suggested_buf),
            )
        } else {
            (
                "Agent mode is not configured. Run `flyline agent-mode --help` or see https://github.com/HalFrgrd/flyline#agent-mode".to_string(),
                None,
            )
        };
        self.content_mode = ContentMode::AgentError {
            message,
            raw_output: String::new(),
            suggested_buffer,
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

    /// Spawn the configured AI command in a background thread and transition to `AiMode`.
    /// Words that contain a space are quoted with single quotes in the display string.
    fn start_agent_mode(&mut self, agent_cmd: settings::AgentModeCommand, buffer_str: &str) {
        let cmd_args = agent_cmd.command;
        let final_arg = match agent_cmd.system_prompt.as_deref() {
            Some(prompt) => format!("{}\n{}", prompt, buffer_str),
            None => buffer_str.to_string(),
        };
        let (tx, rx) = std::sync::mpsc::channel::<Result<String, (String, String)>>();
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
        std::thread::spawn(move || {
            // Safety: the guard `!ai_command.is_empty()` at the call site ensures
            // cmd_args is non-empty, so split_first() always returns Some.
            let (prog, args) = cmd_args.split_first().expect("ai_command is non-empty");

            // Bash sets SIGCHLD to SIG_IGN, causing the kernel to auto-reap child
            // processes. This makes output()'s internal wait() fail with ECHILD
            // (os error 10). Temporarily restore SIG_DFL so we can wait on our
            // child, then put the original disposition back.
            // SAFETY: signal(2) only modifies signal disposition. No other thread
            // in this process depends on SIGCHLD being SIG_IGN at this instant.
            let prev_sigchld = unsafe { libc::signal(libc::SIGCHLD, libc::SIG_DFL) };

            let result: Result<String, (String, String)> = std::process::Command::new(prog)
                .args(args)
                .arg(&final_arg)
                .output()
                .inspect(|_| unsafe {
                    libc::signal(libc::SIGCHLD, prev_sigchld);
                })
                .inspect_err(|_| unsafe {
                    libc::signal(libc::SIGCHLD, prev_sigchld);
                })
                .map_err(|e| (format!("Failed to run AI command: {}", e), String::new()))
                .and_then(|output| {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        log::warn!("AI command exited with {}: {}", output.status, stderr);
                        Err((
                            format!("AI command exited with {}", output.status),
                            format!("stdout: {}\nstderr: {}", stdout, stderr),
                        ))
                    } else {
                        Ok(stdout)
                    }
                });
            if let Err(e) = tx.send(result) {
                log::warn!("AI task: failed to send result (receiver dropped): {}", e);
            }
        });
        self.content_mode = ContentMode::AgentModeWaiting {
            receiver: rx,
            command_display,
            start_time: std::time::Instant::now(),
        };
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
        self.inline_history_suggestion =
            if !self.settings.show_inline_history || self.buffer.buffer().is_empty() {
                None
            } else {
                self.history_manager
                    .get_command_suggestion_suffix(self.buffer.buffer())
            };

        // Apply fuzzy filtering to active tab completion suggestions
        if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
            let buffer: &str = self.buffer.buffer();
            let completion_context = tab_completion_context::get_completion_context(
                buffer,
                self.buffer.cursor_byte_pos(),
            );
            let word_under_cursor_str = completion_context.word_under_cursor;
            if let Ok(word_under_cursor) = SubString::new(buffer, word_under_cursor_str) {
                if word_under_cursor.overlaps_with(&active_suggestions.word_under_cursor) {
                    log::debug!(
                        "Word under cursor changed slightly ('{}' -> '{}'), applying fuzzy filter to tab completion suggestions",
                        active_suggestions.word_under_cursor.s,
                        word_under_cursor.s
                    );
                    active_suggestions.apply_fuzzy_filter(word_under_cursor);
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

        self.formatted_buffer_cache = format_buffer(
            &self.dparser_tokens_cache,
            self.buffer.cursor_byte_pos(),
            self.buffer.buffer().len(),
            self.mode.is_running(),
            Some(Box::new(Self::get_word_info)),
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

    fn get_word_info(token: &dparser::AnnotatedToken) -> Option<formated_buffer::WordInfo> {
        if token.annotations.is_env_var && token.token.kind.is_word() {
            let env_var_name = &token.token.value;

            let tooltip = bash_funcs::format_shell_var(env_var_name);

            return Some(formated_buffer::WordInfo {
                tooltip: Some(tooltip),
                is_recognised_command: false,
            });
        } else if let Some(value) = &token.annotations.command_word {
            let (command_type, description) = bash_funcs::get_command_info(value);
            return Some(formated_buffer::WordInfo {
                tooltip: Some(description.to_string()),
                is_recognised_command: command_type != bash_funcs::CommandType::Unknown,
            });
        } else if token.annotations.is_empty() && token.token.value.starts_with('~') {
            let expanded = bash_funcs::expand_filename(&token.token.value);
            if expanded != token.token.value {
                return Some(formated_buffer::WordInfo {
                    tooltip: Some(format!("{}={}", token.token.value, expanded)),
                    is_recognised_command: false,
                });
            }
        }
        None
    }

    fn ts_to_timeago_string_5chars(ts: u64) -> String {
        let duration = std::time::Duration::from_secs(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(ts),
        );
        let s = timeago::format_5chars(duration);
        format!("{:>5}", s.trim_start_matches('0'))
    }

    /// Build the display lines for a single fuzzy-history entry.
    ///
    /// Returns one `Line` per terminal row. The first line combines the
    /// header prefix (index / score / timeago / indicator) with the first
    /// command row; subsequent lines carry the continuation prefix.
    fn get_lines_for_history_entry(
        formatted_entry: &HistoryEntryFormatted,
        entry_idx: usize,
        fuzzy_search_index: usize,
        num_digits_for_index: usize,
        num_digits_for_score: usize,
        header_prefix_width: usize,
        available_cols: u16,
        palette: &Palette,
    ) -> Vec<Line<'static>> {
        let is_selected = fuzzy_search_index == entry_idx;

        let entry = &formatted_entry.entry;
        let timeago_str = entry
            .timestamp
            .map(Self::ts_to_timeago_string_5chars)
            .unwrap_or_else(|| "     ".to_string());

        let indicator_span = || {
            if is_selected {
                Span::styled("▐", palette.matching_char())
            } else {
                Span::styled(" ", palette.secondary_text())
            }
        };

        let formatted_text = formatted_entry.command_spans(&palette);

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
        let empty_line = Line::from(vec![]);

        content.prompt_start = Some(content.cursor_position());

        let (lprompt, rprompt, fill_span) = self
            .prompt_manager
            .get_ps1_lines(self.settings.show_animations);
        for (_, is_last, either_or_both) in
            lprompt.iter().zip_longest(rprompt.iter()).flag_first_last()
        {
            let (l_line, r_line) = either_or_both.or(&empty_line, &empty_line);
            if is_last {
                content.write_line_lrjustified(
                    l_line,
                    &Line::from(" "),
                    r_line,
                    Tag::Ps1Prompt,
                    true,
                );
            } else {
                content.write_line_lrjustified(l_line, &fill_span, r_line, Tag::Ps1Prompt, false);
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
                &Span::from(" ")
            } else {
                if self.mode.is_running() && self.settings.show_animations {
                    &part.get_possible_animated_span(now)
                } else {
                    part.normal_span()
                }
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

            let poss_cursor_anim_pos = content.write_span_dont_overwrite(
                span_to_draw,
                move |graph_idx| {
                    graph_idx_to_tag
                        .get(graph_idx)
                        .copied()
                        .unwrap_or(Tag::Command(0))
                },
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
                content.write_span(&ps2, Tag::Ps2Prompt);
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
            content.write_span(
                &Span::styled(
                    "^C",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Tag::Normal,
            );
        }

        if self.mode.is_running()
            && let Some(cursor_pos) = cursor_pos_maybe
        {
            self.cursor_animation.update_position(cursor_pos);
            let cursor_anim_pos = if self.settings.show_animations {
                self.cursor_animation.get_position()
            } else {
                cursor_pos
            };
            let cursor_style = {
                if self.settings.use_term_emulator_cursor {
                    None
                } else {
                    let cursor_intensity = if self.settings.show_animations {
                        self.cursor_animation.get_intensity()
                    } else {
                        255
                    };
                    Some(Palette::cursor_style(cursor_intensity))
                }
            };

            content.set_term_cursor_pos(cursor_anim_pos, cursor_style);
        }

        if self.mode.is_running()
            && self.settings.tutorial_mode
            && self.buffer.buffer().is_empty()
            && matches!(self.content_mode, ContentMode::Normal)
        {
            content.write_span_dont_overwrite(
                &Span::styled(
                    " 💡 Start typing or search history with Ctrl+R",
                    self.settings.color_palette.tutorial_hint(),
                ),
                |_| Tag::HistorySuggestion,
                None,
            );
            content.newline();
            content.write_span_dont_overwrite(
                &Span::styled(
                    TUTORIAL_HISTORY_PREFIX_HINT,
                    self.settings.color_palette.tutorial_hint(),
                ),
                |_| Tag::HistorySuggestion,
                None,
            );
            content.newline();
            content.write_span_dont_overwrite(
                &Span::styled(
                    TUTORIAL_DISABLE_HINT,
                    self.settings.color_palette.tutorial_hint(),
                ),
                |_| Tag::HistorySuggestion,
                None,
            );
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

                    content.write_span_dont_overwrite(
                        &Span::from(line.to_owned())
                            .style(self.settings.color_palette.secondary_text()),
                        |_| Tag::HistorySuggestion,
                        None,
                    );

                    if is_last {
                        let mut extra_info_text = format!(" #idx={}", sug.index);
                        if let Some(ts) = sug.timestamp {
                            let time_ago_str = Self::ts_to_timeago_string_5chars(ts);
                            extra_info_text.push_str(&format!(" {}", time_ago_str.trim_start()));
                        }

                        content.write_span_dont_overwrite(
                            &Span::from(extra_info_text)
                                .style(self.settings.color_palette.inline_suggestion()),
                            |_| Tag::HistorySuggestion,
                            None,
                        );

                        if self.settings.tutorial_mode {
                            content.write_span_dont_overwrite(
                                &Span::styled(
                                    " 💡 Press → or End to accept",
                                    self.settings.color_palette.tutorial_hint(),
                                ),
                                |_| Tag::HistorySuggestion,
                                None,
                            );
                        }
                    }
                });
        }

        let rows_before = content.cursor_position().row;
        let rows_left_before_end_of_screen: u16 = terminal_height.saturating_sub(rows_before + 1);

        match &mut self.content_mode {
            ContentMode::TabCompletion(active_suggestions) if self.mode.is_running() => {
                content.newline();

                // Early exit when there are no suggestions to display.
                if active_suggestions.filtered_suggestions_len() == 0 {
                    content.write_span(
                        &Span::styled(
                            "No suggestions",
                            self.settings.color_palette.secondary_text(),
                        ),
                        Tag::TabSuggestion,
                    );
                } else {
                    let grid_start_row = content.cursor_position().row;
                    let num_rows_for_suggestions = rows_left_before_end_of_screen.clamp(2, 15);

                    let mut selected_grid_row: Option<u16> = None;

                    let grid = active_suggestions.into_grid(
                        num_rows_for_suggestions as usize,
                        width as usize,
                        &self.settings.color_palette,
                    );

                    for row_idx in 0..grid[0].0.len() {
                        for (is_first, _, (col, col_width)) in grid.iter().flag_first_last() {
                            if let Some((formatted, is_selected)) = col.get(row_idx) {
                                if !is_first {
                                    content.write_span(
                                        &Span::raw(" ".repeat(COLUMN_PADDING)),
                                        Tag::TabSuggestion,
                                    );
                                }
                                let formatted_suggestion =
                                    formatted.render(*col_width, *is_selected);
                                let tag = Tag::Suggestion(formatted.suggestion_idx);
                                for span in formatted_suggestion {
                                    content.write_span(&span, tag);
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
            ContentMode::FuzzyHistorySearch if self.mode.is_running() => {
                let num_rows_for_instructions = if self.settings.tutorial_mode { 2 } else { 1 };
                let num_rows_for_results = rows_left_before_end_of_screen
                    .saturating_sub(num_rows_for_instructions)
                    .clamp(2, 30);

                let (fuzzy_results, fuzzy_search_index, num_results, num_searched) = self
                    .history_manager
                    .get_fuzzy_search_results(self.buffer.buffer(), num_rows_for_results as usize);

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
                        entry_idx,
                        fuzzy_search_index,
                        num_digits_for_index,
                        num_digits_for_score,
                        header_prefix_width,
                        available_cols,
                        &self.settings.color_palette,
                    ) {
                        content.newline();
                        content.write_line(&line, false, Tag::HistoryResult(entry_idx));
                        content.fill_line(Tag::HistoryResult(entry_idx));
                        if content.cursor_position().row.saturating_sub(starting_row)
                            >= num_rows_for_results
                        {
                            break 'outer;
                        }
                    }
                }
                content.newline();
                content.write_span(
                    &Span::styled(
                        format!("# Fuzzy search: {}/{}", num_results, num_searched),
                        self.settings.color_palette.secondary_text(),
                    ),
                    Tag::FuzzySearch,
                );
                if self.settings.tutorial_mode {
                    content.newline();
                    content.write_span(
                        &Span::styled(
                            TUTORIAL_FUZZY_SEARCH_HINT,
                            self.settings.color_palette.tutorial_hint(),
                        ),
                        Tag::FuzzySearch,
                    );
                }
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
                            content.write_span(span, Tag::Tooltip);
                        }
                    }
                    if truncated && max_tool_tip_rows > 0 {
                        let last_col = content.width.saturating_sub(1);
                        if content.cursor_position().col >= last_col {
                            content.set_cursor_col(last_col);
                        }
                        content.write_span(
                            &Span::styled("…", self.settings.color_palette.secondary_text()),
                            Tag::Tooltip,
                        );
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
                content.write_span(
                    &Span::styled(
                        format!("Running: {} [{}s]", command_display, elapsed_secs),
                        self.settings.color_palette.secondary_text(),
                    ),
                    Tag::Normal,
                );
            }
            ContentMode::AgentOutputSelection(selection) if self.mode.is_running() => {
                content.newline();
                for line in &selection.header_text {
                    content.write_line(line, true, Tag::Normal);
                }
                for (row_idx, suggestion) in selection.suggestions.iter().enumerate() {
                    let is_selected = selection.selected_idx == row_idx;
                    if is_selected {
                        content.set_focus_row(content.cursor_position().row);
                    }
                    let indicator = if is_selected { "▐" } else { " " };
                    let indicator_style = if is_selected {
                        self.settings.color_palette.matching_char()
                    } else {
                        self.settings.color_palette.secondary_text()
                    };
                    content.write_span(
                        &Span::styled(indicator, indicator_style),
                        Tag::AiResult(row_idx),
                    );
                    // Description line
                    let desc_style = if is_selected {
                        Palette::convert_to_selected(self.settings.color_palette.secondary_text())
                    } else {
                        self.settings.color_palette.secondary_text()
                    };
                    content.write_span(
                        &Span::styled(suggestion.description.clone(), desc_style),
                        Tag::AiResult(row_idx),
                    );
                    content.fill_line(Tag::AiResult(row_idx));
                    content.newline();
                    // Command line: gutter char + syntax-highlighted command via dparser
                    content.write_span(
                        &Span::styled(indicator, indicator_style),
                        Tag::AiResult(row_idx),
                    );
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
                        Some(Box::new(Self::get_word_info)),
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
                        content.write_span(&styled_span, Tag::AiResult(row_idx));
                    }
                    content.fill_line(Tag::AiResult(row_idx));
                    content.newline();
                }
                for line in &selection.footer_text {
                    content.write_line(line, true, Tag::Normal);
                }
            }
            ContentMode::AgentError {
                message,
                raw_output,
                suggested_buffer,
            } if self.mode.is_running() => {
                content.newline();
                content.write_span(
                    &Span::styled(message.clone(), Style::default().fg(Color::Red)),
                    Tag::Normal,
                );
                if let Some(suggested) = suggested_buffer {
                    content.newline();
                    content.write_span(
                        &Span::styled(
                            format!("Buffer with prefix: {}", suggested),
                            self.settings.color_palette.secondary_text(),
                        ),
                        Tag::Normal,
                    );
                    content.newline();
                    content.write_span(
                        &Span::styled(
                            "Press Enter to launch agent mode with this buffer.",
                            self.settings.color_palette.secondary_text(),
                        ),
                        Tag::Blank,
                    );
                } else {
                    if !raw_output.is_empty() {
                        for line in raw_output.lines().take(5) {
                            content.newline();
                            content.write_span(
                                &Span::styled(
                                    line.to_string(),
                                    self.settings.color_palette.secondary_text(),
                                ),
                                Tag::Normal,
                            );
                        }
                    }
                    content.newline();
                    content.write_span(
                        &Span::styled(
                            "Press Enter to run `flyline agent-mode --help`.",
                            self.settings.color_palette.secondary_text(),
                        ),
                        Tag::Blank,
                    );
                }
            }
            _ => {}
        }

        if self.mode.is_running() && self.settings.matrix_animation {
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
            && (self.settings.use_term_emulator_cursor || !self.mode.is_running())
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
