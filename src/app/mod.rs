mod formated_buffer;
mod tab_completion;

use crate::active_suggestions::{ActiveSuggestions, COLUMN_PADDING};
use crate::agent_mode::{AiOutputSelection, parse_ai_output};
use crate::app::formated_buffer::{FormattedBuffer, format_buffer};
use crate::content_builder::{Contents, Tag, split_line_to_terminal_rows};
use crate::cursor_animation::CursorAnimation;
use crate::dparser::{AnnotatedToken, ToInclusiveRange};
use crate::history::{HistoryEntry, HistoryEntryFormatted, HistoryManager, HistorySearchDirection};
use crate::iter_first_last::FirstLast;
use crate::mouse_state::MouseState;
use crate::palette::{DefaultMode, Palette};
use crate::prompt_manager::PromptManager;
use crate::settings::{ColorTheme, MouseMode, Settings};
use crate::text_buffer::{SubString, TextBuffer};
use crate::{bash_funcs, dparser};
use crate::{bash_symbols, command_acceptance};
use crate::{shell_integration, tab_completion_context};
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers, ModifierKeyCode, MouseEvent,
    MouseEventKind,
};
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

/// Standard sRGB luminance coefficients (ITU-R BT.709).
const SRGB_RED_COEFF: f32 = 0.2126;
const SRGB_GREEN_COEFF: f32 = 0.7152;
const SRGB_BLUE_COEFF: f32 = 0.0722;
/// Backgrounds with perceived luminance above this threshold are treated as light.
const LUMINANCE_LIGHT_THRESHOLD: f32 = 0.5;

/// Query the terminal background colour and decide whether to use the dark or
/// light palette preset.  Falls back to `Dark` if the query fails or the
/// terminal does not respond.
fn detect_background_mode() -> DefaultMode {
    match crossterm::style::query_background_color() {
        Ok(Some(crossterm::style::Color::Rgb { r, g, b })) => {
            // Perceived luminance using the standard sRGB coefficients.
            let luminance = SRGB_RED_COEFF * (r as f32 / 255.0)
                + SRGB_GREEN_COEFF * (g as f32 / 255.0)
                + SRGB_BLUE_COEFF * (b as f32 / 255.0);
            if luminance > LUMINANCE_LIGHT_THRESHOLD {
                log::debug!("Background RGB({r},{g},{b}) luminance={luminance:.3} → light mode");
                DefaultMode::Light
            } else {
                log::debug!("Background RGB({r},{g},{b}) luminance={luminance:.3} → dark mode");
                DefaultMode::Dark
            }
        }
        Ok(color) => {
            log::debug!(
                "Background color {:?} not an RGB value, defaulting to dark mode",
                color
            );
            DefaultMode::Dark
        }
        Err(e) => {
            log::debug!("Could not query background color: {e}, defaulting to dark mode");
            DefaultMode::Dark
        }
    }
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
pub enum KeyPressReturnType {
    None,
    NeedScreenClear,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum LastKeyPressAction {
    InsertedAutoClosing { char: char, byte_pos: usize },
}

pub fn get_command(settings: &mut Settings) -> ExitState {
    // if let Err(e) = color_eyre::install() {
    //     log::error!("Failed to install color_eyre panic handler: {}", e);
    // }
    set_panic_hook();

    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();

    // Resolve the colour palette before starting the TUI. When the theme is
    // `Auto`, query the terminal background colour and apply the appropriate
    // preset defaults; user-specified overrides inside the palette are preserved.
    if settings.color_theme == ColorTheme::Auto {
        let mode = detect_background_mode();
        settings.color_palette.apply_theme(mode);
        log::info!("Auto theme resolved to {:?}", mode);
    }

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
    AgentMode {
        receiver: std::sync::mpsc::Receiver<Result<String, (String, String)>>,
        command_display: String,
        start_time: std::time::Instant,
    },
    /// AI output has been parsed; user is selecting a suggestion from the list.
    AgentOutputSelection(AiOutputSelection),
    /// AI command or JSON parsing failed; stores the error message and any raw output.
    AgentError {
        message: String,
        raw_output: String,
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

struct App<'a> {
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
    history_suggestion: Option<(HistoryEntry, String)>,
    mouse_state: MouseState,
    content_mode: ContentMode,
    last_contents: Option<DrawnContent>,
    last_mouse_over_cell: Option<Tag>,
    tooltip: Option<String>,
    settings: &'a Settings,
    /// Terminal row (absolute) where the inline viewport starts; used by smart mouse mode.
    /// Timestamp of the last draw operation.
    last_draw_time: std::time::Instant,
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
            history_suggestion: None,
            mouse_state: MouseState::initialize(&settings.mouse_mode),
            content_mode: ContentMode::Normal,
            last_contents: None,
            last_mouse_over_cell: None,
            tooltip: None,
            settings,
            last_draw_time: std::time::Instant::now(),
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
        let mut needs_screen_cleared = false;
        let mut last_terminal_area = terminal.size().unwrap();

        'main_loop: loop {
            // Poll AI background task: check if a result has arrived without blocking.
            let ai_result = if let ContentMode::AgentMode { ref receiver, .. } = self.content_mode {
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
                            };
                        }
                    },
                    Err((msg, raw_output)) => {
                        log::error!("AI command failed: {}", msg);
                        self.content_mode = ContentMode::AgentError {
                            message: msg,
                            raw_output,
                        };
                    }
                }
                redraw = true;
            }

            if redraw {
                let frame_area = terminal.get_frame().area();

                let content =
                    self.create_content(frame_area.width, frame_area.y, last_terminal_area.height);

                let desired_height = if needs_screen_cleared {
                    last_terminal_area.height
                } else {
                    content.height()
                };

                needs_screen_cleared = false;
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
                                log::error!("Failed to write escape codes: {}", e);
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
                        if let KeyPressReturnType::NeedScreenClear = self.on_keypress(key) {
                            needs_screen_cleared = true;
                        }
                        true
                    }
                    CrosstermEvent::Mouse(mouse) => self.on_mouse(mouse),
                    CrosstermEvent::Resize(new_cols, new_rows) => {
                        // log::trace!("Terminal resized to {}x{}", new_cols, new_rows);
                        last_terminal_area = Size {
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
                        self.on_possible_buffer_change(None);
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
            self.on_possible_buffer_change(None);
            true
        } else {
            false
        }
    }

    /// MacOs: https://stackoverflow.com/questions/12827888/what-is-the-representation-of-the-mac-command-key-in-the-terminal
    /// MacOs command keyboard shortcuts are not sent to terminal apps by default.
    /// They are often captured by the terminal emulator itself for various commands
    /// Try `ghostty +list-keybinds --default` on ghostty. Most
    ///
    /// META: this is similar to Alt. How are they different?
    /// SUPER: Windows key or Mac Command key
    /// HYPER: Often as as result of pressing Ctrl + Shift + Alt + Windows/Command key. rarely used.
    ///
    /// https://en.wikipedia.org/wiki/Table_of_keyboard_shortcuts#Command_line_shortcuts
    ///
    /// Meta vs Alt:
    /// On iterm2, there is a seetitng in Porfiles->Keys->Left option key.
    /// Choices are Normal or  (Set high bit (not recommended) or Esc+).
    /// Set high bit gives you a warning: "You have chosen to have an option key as Meta. This is
    /// useful for backward compatibility with old applications. The "Esc+" option is recommended for most users"
    /// In text_buffer.rs, I check if either of them are set for maximal compatibility.
    fn on_keypress(&mut self, key: KeyEvent) -> KeyPressReturnType {
        log::trace!("Key event: {:?}", key);

        let mut keypress_action: Option<LastKeyPressAction> = None;

        // Smart mode: any keypress re-enables mouse capture, unless the user has
        // explicitly disabled it via a toggle action.
        if self.settings.mouse_mode == MouseMode::Smart
            && !self.mouse_state.is_explicitly_disabled_by_user()
        {
            self.mouse_state.enable("smart mode: keypress detected");
        }

        match key {
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } if matches!(self.content_mode, ContentMode::TabCompletion(_)) => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.on_left_arrow();
                }
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } if matches!(self.content_mode, ContentMode::TabCompletion(_)) => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.on_right_arrow();
                }
            }
            // Handle Right/End with history suggestion logic
            KeyEvent {
                code: KeyCode::Right | KeyCode::End,
                ..
            } if self.buffer.is_cursor_at_end() && self.history_suggestion.is_some() => {
                if let Some((_, suf)) = &self.history_suggestion {
                    self.buffer.insert_str(suf);
                    self.buffer.move_to_end();
                }
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } if matches!(self.content_mode, ContentMode::TabCompletion(_)) => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.on_up_arrow();
                }
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } if matches!(self.content_mode, ContentMode::AgentOutputSelection(_)) => {
                if let ContentMode::AgentOutputSelection(selection) = &mut self.content_mode {
                    selection.move_up();
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } if matches!(self.content_mode, ContentMode::AgentOutputSelection(_)) => {
                if let ContentMode::AgentOutputSelection(selection) = &mut self.content_mode {
                    selection.move_down();
                }
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) => {
                self.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::Forward);
            }
            // Handle Up with history navigation when at first line
            KeyEvent {
                code: KeyCode::Up, ..
            } if self.buffer.cursor_row() == 0 => {
                self.buffer_before_history_navigation
                    .get_or_insert_with(|| self.buffer.buffer().to_string());
                if let Some(entry) = self
                    .history_manager
                    .search_in_history(self.buffer.buffer(), HistorySearchDirection::Backward)
                {
                    self.buffer.replace_buffer(&entry.command);
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } if matches!(self.content_mode, ContentMode::TabCompletion(_)) => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.on_down_arrow();
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('s'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) => {
                self.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::Backward);
            }
            // Page Up/Down - scroll by a full page in fuzzy history search
            KeyEvent {
                code: KeyCode::PageUp,
                ..
            } if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) => {
                self.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::PageForward);
            }
            KeyEvent {
                code: KeyCode::PageDown,
                ..
            } if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) => {
                self.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::PageBackward);
            }
            // Handle Down with history navigation when at last line
            KeyEvent {
                code: KeyCode::Down,
                ..
            } if self.buffer.is_cursor_on_final_line() => {
                match self
                    .history_manager
                    .search_in_history(self.buffer.buffer(), HistorySearchDirection::Forward)
                {
                    Some(entry) => {
                        self.buffer.replace_buffer(&entry.command);
                    }
                    None => {
                        if let Some(original_buffer) = self.buffer_before_history_navigation.take()
                        {
                            self.buffer.replace_buffer(&original_buffer);
                        }
                    }
                }
            }
            // Alt+Enter - activate Agent mode (requires --agent-mode to be configured)
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::ALT,
                ..
            } if matches!(self.content_mode, ContentMode::Normal) => {
                if !self.settings.ai_command.is_empty() {
                    self.start_agent_mode();
                } else {
                    self.show_agent_mode_not_configured_error();
                }
            }
            // Enter key - accept suggestions or submit command
            KeyEvent {
                code: KeyCode::Enter,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'), // Without this, when I hold enter, sometimes 'j' is read as input
                modifiers: KeyModifiers::CONTROL,
                ..
            } => match &mut self.content_mode {
                ContentMode::FuzzyHistorySearch => {
                    self.accept_fuzzy_history_search();
                    // TODO: allow someone to accept and run the command immediately
                    // Instead of Enter+Enter
                    // self.try_submit_current_buffer();
                }
                ContentMode::TabCompletion(active_suggestions) => {
                    active_suggestions.accept_currently_selected(&mut self.buffer);
                    self.content_mode = ContentMode::Normal;
                }
                ContentMode::Normal => {
                    self.try_submit_current_buffer();
                }
                ContentMode::AgentMode { .. } => {}
                ContentMode::AgentError { .. } => {
                    self.content_mode = ContentMode::Normal;
                    self.buffer.replace_buffer("flyline agent-mode --help");
                    self.on_possible_buffer_change(None);
                    self.try_submit_current_buffer();
                }
                ContentMode::AgentOutputSelection(selection) => {
                    if let Some(cmd) = selection.selected_command() {
                        let cmd = cmd.to_string();
                        self.buffer.replace_buffer(&cmd);
                        self.on_possible_buffer_change(None);
                    }
                    self.content_mode = ContentMode::Normal;
                }
            },
            // Shift+Tab or BackTab - cycle suggestions backward
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::SHIFT,
                ..
            }
            | KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.on_tab(true);
                }
            }
            // Tab - cycle suggestions or trigger completion
            KeyEvent {
                code: KeyCode::Tab, ..
            } => match &mut self.content_mode {
                ContentMode::FuzzyHistorySearch => {
                    self.accept_fuzzy_history_search();
                }
                ContentMode::TabCompletion(active_suggestions) => {
                    active_suggestions.on_tab(false);
                }
                ContentMode::Normal => {
                    self.start_tab_complete();
                }
                ContentMode::AgentMode { .. } => {}
                ContentMode::AgentOutputSelection(_) => {}
                ContentMode::AgentError { .. } => {}
            },

            // Escape - clear suggestions or toggle mouse (Simple and Smart modes)
            KeyEvent {
                code: KeyCode::Esc, ..
            } => match self.content_mode {
                ContentMode::TabCompletion(_)
                | ContentMode::FuzzyHistorySearch
                | ContentMode::AgentMode { .. }
                | ContentMode::AgentOutputSelection(_)
                | ContentMode::AgentError { .. } => {
                    self.content_mode = ContentMode::Normal;
                }
                ContentMode::Normal => {
                    if matches!(
                        self.settings.mouse_mode,
                        MouseMode::Simple | MouseMode::Smart
                    ) {
                        self.toggle_mouse_state("Escape pressed");
                    }
                }
            },
            // Ctrl+D - exit
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                if self.buffer.buffer().is_empty() && unsafe { bash_symbols::ignoreeof != 0 } {
                    self.mode = AppRunningState::Exiting(ExitState::EOF);
                } else {
                    self.buffer.delete_forwards();
                }
            }
            // Ctrl+C - cancel
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::META,
                ..
            } => {
                self.mode = AppRunningState::Exiting(ExitState::WithoutCommand);
            }
            // Ctrl+/ (shows as Ctrl+7) - comment out and execute
            KeyEvent {
                code: KeyCode::Char('7') | KeyCode::Char('/'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::META | KeyModifiers::SUPER,
                ..
            } => {
                self.buffer.move_to_start();
                self.buffer.insert_str("#");
                self.try_submit_current_buffer();
            }
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::META,
                ..
            } => {
                match self.content_mode {
                    ContentMode::FuzzyHistorySearch => {
                        self.content_mode = ContentMode::Normal;
                        // self.fuzzy_history_search_results.clear();
                    }
                    ContentMode::Normal | ContentMode::TabCompletion(_) => {
                        self.content_mode = ContentMode::FuzzyHistorySearch;
                        self.history_manager
                            .warm_fuzzy_search_cache(self.buffer.buffer());
                    }
                    ContentMode::AgentMode { .. }
                    | ContentMode::AgentOutputSelection(_)
                    | ContentMode::AgentError { .. } => {}
                }
            }
            KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                // Clear screen
                return KeyPressReturnType::NeedScreenClear;
            }
            KeyEvent {
                code: KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                // In Simple mode: Alt press toggles mouse capture.
                if self.settings.mouse_mode == MouseMode::Simple {
                    self.toggle_mouse_state("simple mode: Alt pressed");
                }
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } if self.settings.auto_close_chars => {
                if self.would_overwrite_auto_inserted_closing(c) {
                    log::info!(
                        "Not inserting char '{}' to avoid overwriting auto-inserted closing token",
                        c
                    );
                    self.buffer.move_right();
                } else {
                    let initial_cursor_pos = self.buffer.cursor_byte_pos();
                    self.buffer.on_keypress(key);
                    if let Some((auto_char, auto_pos)) =
                        self.insert_closing_char(c, initial_cursor_pos)
                    {
                        keypress_action = Some(LastKeyPressAction::InsertedAutoClosing {
                            char: auto_char,
                            byte_pos: auto_pos,
                        });
                    }
                }
            }
            // Backspace: if the char to the right of the cursor is an auto-inserted closing token
            // paired with the char about to be deleted, remove it as well.
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.settings.auto_close_chars => {
                self.delete_auto_inserted_closing_if_present();
                self.buffer.on_keypress(key);
            }
            _ => {
                // Delegate basic text editing to TextBuffer
                self.buffer.on_keypress(key);
            }
        }

        self.on_possible_buffer_change(keypress_action);
        KeyPressReturnType::None
    }

    fn would_overwrite_auto_inserted_closing(&self, c: char) -> bool {
        let cursor_pos = self.buffer.cursor_byte_pos();
        if cursor_pos == 0 {
            return false;
        }
        if let Some(dparser_token) = self
            .dparser_tokens_cache
            .iter()
            .find(|t| t.token.byte_range().contains(&cursor_pos))
        {
            if let Some(dparser::ClosingAnnotation {
                is_auto_inserted: true,
                ..
            }) = &dparser_token.annotations.closing
            {
                return dparser_token.token.value.starts_with(c);
            }
        }
        false
    }

    /// After a character `c` has been inserted into the buffer, insert the corresponding
    /// closing character when `c` is an unmatched opening delimiter.
    ///
    /// The decision is made using `dparser_tokens_cache`, which represents the buffer state
    /// *before* `c` was typed (one character out of date).  The cache is passed to
    /// [`buffer_format::FormattedBuffer::closing_char_to_insert`] which uses the stale token
    /// annotations to determine whether `c` opens a new pair or closes an existing one.
    ///
    /// Returns the byte position of the auto-inserted closing character, or `None` if no
    /// closing character was inserted.
    fn insert_closing_char(&mut self, c: char, initial_cursor_pos: usize) -> Option<(char, usize)> {
        if let Some(closing) = dparser::DParser::closing_char_to_insert(
            &self.dparser_tokens_cache,
            c,
            initial_cursor_pos,
        ) {
            self.buffer.insert_char(closing);
            self.buffer.move_left();
            // After move_left, cursor is at the start of the auto-inserted closing char.
            log::info!(
                "Inserted auto-closing char '{}' at byte position {}",
                closing,
                self.buffer.cursor_byte_pos()
            );
            Some((closing, self.buffer.cursor_byte_pos()))
        } else {
            None
        }
    }

    /// Mark the dparser token at `byte_pos` as auto-inserted in the cache.
    fn mark_auto_inserted_closing(
        dparser_tokens: &mut [dparser::AnnotatedToken],
        c: char,
        byte_pos: usize,
    ) {
        for token in dparser_tokens {
            if token.token.byte_range().start == byte_pos
                && token.token.value.starts_with(c)
                && let Some(dparser::ClosingAnnotation {
                    is_auto_inserted, ..
                }) = &mut token.annotations.closing
            {
                *is_auto_inserted = true;
                log::info!(
                    "Marked token '{}' at byte {} as auto-inserted",
                    token.token.value,
                    byte_pos
                );
                return;
            }
        }
        log::warn!(
            "Failed to mark auto-inserted closing char '{}' at byte position {}: no matching token found in cache",
            c,
            byte_pos
        );
    }

    /// If the token immediately to the right of the cursor is an auto-inserted closing token
    /// that is paired with the token the cursor is right after, delete it.
    /// This is called before a simple Backspace so that deleting an auto-paired opener also
    /// removes the auto-inserted closer.
    fn delete_auto_inserted_closing_if_present(&mut self) {
        let cursor_pos = self.buffer.cursor_byte_pos();
        if cursor_pos == 0 {
            return;
        }

        // Find the token that ends at cursor_pos (the one about to be deleted by Backspace).
        let opening_annotation = self
            .dparser_tokens_cache
            .iter()
            .find(|t| t.token.byte_range().contains(&(cursor_pos - 1)))
            .map(|t| t.annotations.opening.clone());

        if let Some(Some(dparser::OpeningState::Matched(closing_idx))) = opening_annotation {
            // Check if the closing token starts immediately at cursor_pos and is auto-inserted.
            if let Some(closing_token) = self.dparser_tokens_cache.get(closing_idx)
                && closing_token.token.byte_range().start == cursor_pos
                && let Some(dparser::ClosingAnnotation {
                    is_auto_inserted: true,
                    ..
                }) = closing_token.annotations.closing
            {
                self.buffer.delete_forwards();
            }
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
    fn show_agent_mode_not_configured_error(&mut self) {
        self.content_mode = ContentMode::AgentError {
            message: "Agent mode is not configured. Run `flyline agent-mode --help` or see https://github.com/HalFrgrd/flyline#agent-mode".to_string(),
            raw_output: String::new(),
        };
    }

    /// Spawn the configured AI command in a background thread and transition to `AiMode`.
    /// Words that contain a space are quoted with single quotes in the display string.
    fn start_agent_mode(&mut self) {
        let cmd_args = self.settings.ai_command.clone();
        let buffer_str = self.buffer.buffer().to_string();
        let final_arg = match self.settings.ai_system_prompt.as_deref() {
            Some(prompt) => format!("{}\n{}", prompt, buffer_str),
            None => buffer_str,
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
        self.content_mode = ContentMode::AgentMode {
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

    fn on_possible_buffer_change(&mut self, last_keypress_action: Option<LastKeyPressAction>) {
        self.history_suggestion =
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
            last_keypress_action
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

            let graph_idx_to_tag = if part.token.token.kind == TokenKind::Newline {
                // For newlines, span_to_draw is replaced with " " (1 grapheme), but
                // styled_graphemes("\n") yields zero items, so we must build the tag
                // mapping manually to avoid falling back to Tag::Command(0).
                vec![Tag::Command(part.token.token.byte_range().start)]
            } else {
                part.normal_span()
                    .styled_graphemes(Style::default())
                    .scan(part.token.token.byte_range().start, |acc, graph| {
                        let tag = Tag::Command(*acc);
                        *acc += graph.symbol.len();
                        Some(tag)
                    })
                    .collect::<Vec<_>>()
            };

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

        if let Some((sug, suf)) = &self.history_suggestion
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
                    let mut rows: Vec<Vec<(Vec<Span>, usize)>> =
                        vec![vec![]; num_rows_for_suggestions as usize];

                    let mut selected_grid_row: Option<u16> = None;
                    let col_offset = active_suggestions.col_scroll_offset();
                    for (col, col_width) in active_suggestions.into_grid(
                        num_rows_for_suggestions as usize,
                        width as usize,
                        col_offset,
                        &self.settings.color_palette,
                    ) {
                        for (row_idx, (formatted, is_selected)) in col.iter().enumerate() {
                            let formatted_suggestion = formatted.render(col_width, *is_selected);
                            if *is_selected {
                                selected_grid_row = Some(row_idx as u16);
                            }
                            rows[row_idx].push((formatted_suggestion, formatted.suggestion_idx));
                        }
                    }

                    let num_visible_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);

                    for row in rows.into_iter().filter(|r| !r.is_empty()) {
                        let num_cols = row.len();
                        for (col_idx, (styled_spans, suggestion_idx)) in row.into_iter().enumerate()
                        {
                            for span in styled_spans {
                                content.write_span(&span, Tag::Suggestion(suggestion_idx));
                            }
                            if col_idx + 1 < num_cols {
                                content.write_span(
                                    &Span::raw(" ".repeat(COLUMN_PADDING)),
                                    Tag::TabSuggestion,
                                );
                            }
                        }
                        content.newline();
                    }
                    active_suggestions
                        .update_grid_size(num_rows_for_suggestions as usize, num_visible_cols);

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
            ContentMode::AgentMode {
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
            } if self.mode.is_running() => {
                content.newline();
                content.write_span(
                    &Span::styled(
                        format!("AI failed: {}", message),
                        Style::default().fg(Color::Red),
                    ),
                    Tag::Normal,
                );
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
