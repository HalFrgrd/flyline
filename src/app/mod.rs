mod buffer_format;
mod tab_completion;

use crate::active_suggestions::ActiveSuggestions;
use crate::agent_mode::{AiOutputSelection, parse_ai_output};
use crate::app::buffer_format::{FormattedBuffer, format_buffer};
use crate::bash_env_manager::BashEnvManager;
use crate::command_acceptance;
use crate::content_builder::{Contents, Tag, split_line_to_terminal_rows};
use crate::cursor_animation::CursorAnimation;
use crate::dparser::{AnnotatedToken, ToInclusiveRange};
use crate::history::{HistoryEntry, HistoryManager, HistorySearchDirection};
use crate::iter_first_last::FirstLast;
use crate::mouse_state::MouseState;
use crate::palette::Palette;
use crate::prompt_manager::PromptManager;
use crate::settings::{MouseMode, Settings};
use crate::snake_animation::SnakeAnimation;
use crate::tab_completion_context;
use crate::text_buffer::{SubString, TextBuffer};
use crate::{bash_funcs, dparser};
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

const TUTORIAL_FUZZY_SEARCH_HINT: &str = "💡 Type to search, press arrow keys / Page Up/Down to browse, Enter to run the command, Shift+Enter to accept the command for editing";
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

fn restore() {
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
        restore();
        hook(info);
    }));
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ExitState {
    WithCommand(String),
    WithoutCommand,
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

pub fn get_command(settings: &Settings) -> ExitState {
    // if let Err(e) = color_eyre::install() {
    //     log::error!("Failed to install color_eyre panic handler: {}", e);
    // }
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

    restore();

    log::debug!("Final state: {:?}", end_state);
    end_state
}

#[derive(Debug)]
enum ContentMode {
    Normal,
    FuzzyHistorySearch,
    TabCompletion(ActiveSuggestions),
    /// AI command is running in the background. Stores the channel receiver and the
    /// human-readable representation of the command being executed.
    AiMode {
        receiver: std::sync::mpsc::Receiver<Result<String, (String, String)>>,
        command_display: String,
        start_time: std::time::Instant,
    },
    /// AI output has been parsed; user is selecting a suggestion from the list.
    AiOutputSelection(AiOutputSelection),
    /// AI command or JSON parsing failed; stores the error message and any raw output.
    AiError {
        message: String,
        raw_output: String,
    },
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
    bash_env: BashEnvManager,
    snake_animation: SnakeAnimation,
    history_suggestion: Option<(HistoryEntry, String)>,
    mouse_state: MouseState,
    content_mode: ContentMode,
    last_contents: Option<(Contents, i16)>,
    last_mouse_over_cell: Option<Tag>,
    tooltip: Option<String>,
    settings: &'a Settings,
    /// Terminal row (absolute) where the inline viewport starts; used by smart mouse mode.
    last_viewport_top: u16,
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
            bash_env: BashEnvManager::new(), // TODO: This is potentially expensive, load in background?
            snake_animation: SnakeAnimation::new(),
            history_suggestion: None,
            mouse_state: MouseState::initialize(&settings.mouse_mode),
            content_mode: ContentMode::Normal,
            last_contents: None,
            last_mouse_over_cell: None,
            tooltip: None,
            settings,
            last_viewport_top: 0,
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

        let options = TerminalOptions {
            viewport: Viewport::Inline(0),
        };
        let mut terminal =
            ratatui::Terminal::with_options(backend, options).expect("Failed to create terminal");
        if !self.settings.use_term_emulator_cursor {
            terminal.hide_cursor().unwrap();
        }

        // Set up event stream and timers directly
        // let mut time_since_last_input = Instant::now();

        // const ANIMATION_FPS_MAX: u64 = 60;
        // const ANIMATION_FPS_MIN: u64 = 5;
        // const ANIM_SWITCH_INACTIVITY_START: u128 = 10000;
        // const ANIM_SWITCH_INACTIVITY_LEN: u128 = 10000;

        // let anim_period = Duration::from_millis(1000 / ANIMATION_FPS_MAX);

        let mut redraw = true;
        let mut needs_screen_cleared = false;
        let mut last_terminal_area = terminal.size().unwrap();

        loop {
            // Poll AI background task: check if a result has arrived without blocking.
            let ai_result = if let ContentMode::AiMode { ref receiver, .. } = self.content_mode {
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
                    Ok(raw_output) => {
                        let suggestions = parse_ai_output(&raw_output);
                        if suggestions.is_empty() {
                            log::warn!("AI command returned no suggestions");
                            self.content_mode = ContentMode::AiError {
                                message: "Failed to parse AI output as valid JSON:".to_string(),
                                raw_output,
                            };
                        } else {
                            self.content_mode =
                                ContentMode::AiOutputSelection(AiOutputSelection::new(suggestions));
                        }
                    }
                    Err((msg, raw_output)) => {
                        log::error!("AI command failed: {}", msg);
                        self.content_mode = ContentMode::AiError {
                            message: msg,
                            raw_output,
                        };
                    }
                }
                redraw = true;
            }

            if redraw {
                let frame_area = terminal.get_frame().area();

                let mut content = self.create_content(frame_area.width);

                if !self.mode.is_running() {
                    // so that we can put the terminal emulators cursor below the content
                    content.increase_buf_single_row();
                }

                let desired_height = if needs_screen_cleared {
                    last_terminal_area.height
                } else {
                    content.height().min(last_terminal_area.height)
                };
                needs_screen_cleared = false;
                terminal
                    .set_viewport_height(desired_height)
                    .unwrap_or_else(|e| {
                        log::error!("Failed to set viewport height: {}", e);
                    });

                // The problem is that draw might try and query the cursor_position if it needs resizing
                // and we are using Inline viewport.
                // Call is try_draw->autoresize->resize->compute_inline_size->backend.get_cursor_position
                if let Err(e) = terminal.draw(|f| self.ui(f, content)) {
                    log::error!("Failed to draw terminal UI: {}", e);
                }

                if !self.mode.is_running() {
                    // put the terminal emulators cursor just below the content
                    let final_cursor_row = terminal.get_frame().area().bottom().saturating_sub(1);
                    let pos = Position {
                        x: 0,
                        y: final_cursor_row,
                    };

                    if let Err(e) = terminal.set_cursor_position(pos) {
                        log::error!("Failed to set cursor position: {}", e);
                    } else {
                        log::debug!("Set cursor position to ({}, {})", 0, final_cursor_row);
                    }
                }
            }

            if !self.mode.is_running() {
                break;
            }

            redraw = if event::poll(Duration::from_millis(30)).unwrap() {
                match event::read().unwrap() {
                    CrosstermEvent::Key(key) => {
                        if let KeyPressReturnType::NeedScreenClear = self.on_keypress(key) {
                            needs_screen_cleared = true;
                        }
                        true
                    }
                    CrosstermEvent::Mouse(mouse) => self.on_mouse(mouse),
                    CrosstermEvent::Resize(new_cols, new_rows) => {
                        log::debug!("Terminal resized to {}x{}", new_cols, new_rows);
                        last_terminal_area = Size {
                            width: new_cols,
                            height: new_rows,
                        };

                        true
                    }
                    CrosstermEvent::FocusLost => {
                        // log::debug!("Terminal focus lost");
                        self.cursor_animation.term_has_focus = false;
                        false
                    }
                    CrosstermEvent::FocusGained => {
                        // log::debug!("Terminal focus gained");
                        self.cursor_animation.term_has_focus = true;
                        if self.settings.mouse_mode == MouseMode::Smart
                            && !self.mouse_state.is_explicitly_disabled_by_user()
                        {
                            self.mouse_state.enable("smart mode: focus gained");
                        }
                        false
                    }
                    CrosstermEvent::Paste(pasted) => {
                        log::debug!("Pasted content: {}", pasted);
                        log::debug!("Pasted content as bytes: {:?}", pasted.as_bytes());
                        self.buffer.insert_str(&pasted);
                        self.on_possible_buffer_change(None);
                        true
                    }
                }
            } else {
                true
            }
        }

        match self.mode {
            AppRunningState::Exiting(exit_state) => exit_state,
            _ => {
                log::error!(
                    "Exited run loop without valid exit state, defaulting to ExitingWithoutCommand"
                );
                ExitState::WithoutCommand
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

        // Smart mode: check if the mouse is above the viewport or a scroll event occurred.
        if self.settings.mouse_mode == MouseMode::Smart {
            if mouse.row < self.last_viewport_top {
                self.mouse_state
                    .disable("smart mode: mouse is above the viewport");
                self.last_mouse_over_cell = None;
                return false;
            }
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
        }

        let mut cursor_directly_on_cell = true;

        match self.last_contents.as_ref().and_then(|(contents, offset)| {
            contents.get_tagged_cell(mouse.column, mouse.row, *offset)
        }) {
            Some((tag @ Tag::Suggestion(idx), true)) => {
                self.last_mouse_over_cell = Some(tag);
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.set_selected_by_idx(idx);
                }
            }
            Some((tag @ Tag::HistoryResult(idx), true)) => {
                self.last_mouse_over_cell = Some(tag);
                if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) {
                    self.history_manager.fuzzy_search_set_by_visual_idx(idx);
                }
            }
            Some((tag @ Tag::AiResult(idx), true)) => {
                self.last_mouse_over_cell = Some(tag);
                if let ContentMode::AiOutputSelection(selection) = &mut self.content_mode {
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
                    self.history_manager.fuzzy_search_set_by_visual_idx(idx);
                    self.accept_fuzzy_history_search();
                    update_buffer = true;
                }
            }
            Some(Tag::AiResult(idx)) => {
                if matches!(mouse.kind, MouseEventKind::Up(_))
                    && let ContentMode::AiOutputSelection(selection) = &mut self.content_mode
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
            } if matches!(self.content_mode, ContentMode::AiOutputSelection(_)) => {
                if let ContentMode::AiOutputSelection(selection) = &mut self.content_mode {
                    selection.move_up();
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } if matches!(self.content_mode, ContentMode::AiOutputSelection(_)) => {
                if let ContentMode::AiOutputSelection(selection) = &mut self.content_mode {
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
            // Shift+Enter in fuzzy search - accept without running (move to buffer only)
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::SHIFT,
                ..
            } if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) => {
                self.accept_fuzzy_history_search();
            }
            // Shift+Enter - activate AI mode like Ctrl+I (requires --ai-command to be configured)
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::SHIFT,
                ..
            } if !self.settings.ai_command.is_empty()
                && !matches!(self.content_mode, ContentMode::AiMode { .. }) =>
            {
                self.start_ai_mode();
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
                    self.try_submit_current_buffer();
                }
                ContentMode::TabCompletion(active_suggestions) => {
                    active_suggestions.accept_currently_selected(&mut self.buffer);
                    self.content_mode = ContentMode::Normal;
                }
                ContentMode::Normal => {
                    self.try_submit_current_buffer();
                }
                ContentMode::AiMode { .. } => {}
                ContentMode::AiError { .. } => {
                    self.content_mode = ContentMode::Normal;
                }
                ContentMode::AiOutputSelection(selection) => {
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
                ContentMode::AiMode { .. } => {}
                ContentMode::AiOutputSelection(_) => {}
                ContentMode::AiError { .. } => {}
            },

            // Escape - clear suggestions or toggle mouse (Simple and Smart modes)
            KeyEvent {
                code: KeyCode::Esc, ..
            } => match self.content_mode {
                ContentMode::TabCompletion(_)
                | ContentMode::FuzzyHistorySearch
                | ContentMode::AiMode { .. }
                | ContentMode::AiOutputSelection(_)
                | ContentMode::AiError { .. } => {
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
                code: KeyCode::Char('7'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::META,
                ..
            } => {
                self.buffer.move_to_start();
                self.buffer.insert_str("#");
                self.mode = AppRunningState::Exiting(ExitState::WithCommand(
                    self.buffer.buffer().to_string(),
                ));
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
                        let _ = self
                            .history_manager
                            .get_fuzzy_search_results(self.buffer.buffer());
                    }
                    ContentMode::AiMode { .. }
                    | ContentMode::AiOutputSelection(_)
                    | ContentMode::AiError { .. } => {}
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
            // Ctrl+I - activate AI mode (requires --ai-command to be configured)
            KeyEvent {
                code: KeyCode::Char('i'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                // This shortcut is just so it can work in the vhs demo.
                code: KeyCode::Char('i'),
                modifiers: KeyModifiers::ALT,
                ..
            } if !self.settings.ai_command.is_empty() => {
                self.start_ai_mode();
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
            // Delegate basic text editing to TextBuffer
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } if !self.settings.disable_auto_closing_char => {
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
            } if !self.settings.disable_auto_closing_char => {
                self.delete_auto_inserted_closing_if_present();
                self.buffer.on_keypress(key);
            }
            _ => {
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
            log::info!(
                "Token at cursor position: '{}', with annotation {:?}",
                dparser_token.token.value,
                dparser_token.annotation
            );
            if let dparser::TokenAnnotation::IsClosing {
                is_auto_inserted: true,
                ..
            } = dparser_token.annotation
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
                && let dparser::TokenAnnotation::IsClosing {
                    is_auto_inserted, ..
                } = &mut token.annotation
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
        log::info!(
            "Checking for auto-inserted closing token to delete at byte position {}",
            cursor_pos
        );

        // Find the token that ends at cursor_pos (the one about to be deleted by Backspace).
        let opening_annotation = self
            .dparser_tokens_cache
            .iter()
            .find(|t| t.token.byte_range().contains(&(cursor_pos - 1)))
            .inspect(|t| {
                log::info!(
                    "Token ending at cursor position: '{}', with annotation {:?}",
                    t.token.value,
                    t.annotation
                );
            })
            .map(|t| t.annotation);

        log::info!(
            "Token annotation for token ending at cursor position: {:?}",
            opening_annotation
        );

        if let Some(dparser::TokenAnnotation::IsOpening(Some(closing_idx))) = opening_annotation {
            // Check if the closing token starts immediately at cursor_pos and is auto-inserted.
            log::info!(
                "Found opening token with closing_idx {}. Checking for auto-inserted closing token at byte position {}",
                closing_idx,
                cursor_pos
            );
            if let Some(closing_token) = self.dparser_tokens_cache.get(closing_idx) {
                log::info!(
                    "Token at closing_idx {} is '{}', with annotation {:?}",
                    closing_idx,
                    closing_token.token.value,
                    closing_token.annotation
                );
                if closing_token.token.byte_range().start == cursor_pos {
                    log::info!(
                        "Found token starting at cursor position: '{}'",
                        closing_token.token.value
                    );
                    if let dparser::TokenAnnotation::IsClosing {
                        is_auto_inserted: true,
                        ..
                    } = closing_token.annotation
                    {
                        log::info!(
                            "Deleting auto-inserted closing token '{}' at byte {}",
                            closing_token.token.value,
                            cursor_pos
                        );
                        self.buffer.delete_forwards();
                    }
                }
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

    /// Spawn the configured AI command in a background thread and transition to `AiMode`.
    /// Words that contain a space are quoted with single quotes in the display string.
    fn start_ai_mode(&mut self) {
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
        self.content_mode = ContentMode::AiMode {
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
            if self.settings.disable_inline_history || self.buffer.buffer().is_empty() {
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
            Some(Box::new(Self::wordinfo_fn)),
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
                log::debug!(
                    "Setting tooltip for token at byte position {}: {}",
                    part.token.token.byte_range().start,
                    tooltip
                );
                self.tooltip = Some(tooltip.clone());
            }
        }

        // log::debug!("Formatted buffer cache updated:\n{:#?}", self.formatted_buffer_cache);
    }

    fn wordinfo_fn(token: &dparser::AnnotatedToken) -> Option<buffer_format::WordInfo> {
        match token.annotation {
            dparser::TokenAnnotation::IsCommandWord => {
                let (command_type, description) = bash_funcs::get_command_info(&token.token.value);
                Some(buffer_format::WordInfo {
                    tooltip: Some(description.to_string()),
                    is_recognised_command: command_type != bash_funcs::CommandType::Unknown,
                })
            }
            dparser::TokenAnnotation::IsEnvVar => {
                let env_var_name = &token.token.value;
                let tooltip = match bash_funcs::get_env_variable(env_var_name) {
                    Some(value) => format!("${}={}", env_var_name, value),
                    None => format!("${}", env_var_name),
                };
                Some(buffer_format::WordInfo {
                    tooltip: Some(tooltip),
                    is_recognised_command: false,
                })
            }
            dparser::TokenAnnotation::None if token.token.value.starts_with('~') => {
                let expanded = bash_funcs::expand_filename(&token.token.value);
                if expanded != token.token.value {
                    return Some(buffer_format::WordInfo {
                        tooltip: Some(format!("{}={}", token.token.value, expanded)),
                        is_recognised_command: false,
                    });
                }
                None
            }
            _ => None,
        }
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

    fn create_content(&mut self, width: u16) -> Contents {
        // Basically build the entire frame in a Content first
        // Then figure out how to fit that into the actual frame area
        let mut content = Contents::new(width);
        let empty_line = Line::from(vec![]);

        let (lprompt, rprompt, fill_span) = self
            .prompt_manager
            .get_ps1_lines(self.settings.disable_animations);
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
                // log::debug!("Writing PS1 line:  right='{}'", r_line);
                content.write_line_lrjustified(l_line, &fill_span, r_line, Tag::Ps1Prompt, false);
            }
            if !is_last {
                content.newline();
            }
        }

        let mut line_idx = 0;
        let mut cursor_pos_maybe = None;
        self.formatted_buffer_cache
            .parts
            .iter_mut()
            .for_each(|part| {
                if self.mode.is_running()
                    && !self.settings.disable_animations
                    && part.token.annotation == dparser::TokenAnnotation::IsCommandWord
                    && part.normal_span().content.starts_with("python")
                {
                    self.snake_animation.update_anim();
                    let snake_str = self
                        .snake_animation
                        .apply_to_string(&part.normal_span().content);
                    if let Err(e) =
                        part.set_alternative_span(Span::styled(snake_str, part.normal_span().style))
                    {
                        log::warn!("Failed to set alternative span for snake animation: {}", e);
                    }
                } else {
                    part.clear_alternative_span();
                }
            });

        for part in self.formatted_buffer_cache.parts.iter() {
            let span_to_draw = if part.token.token.kind == TokenKind::Newline {
                // For newlines, draw a space instead so that we can have a place to put the cursor
                &Span::from(" ")
            } else {
                part.span_to_use()
            };

            let poss_cursor_anim_pos = content.write_span_dont_overwrite(
                span_to_draw,
                Tag::Command(part.token.token.byte_range().start),
                part.cursor_grapheme_idx,
            );
            if cursor_pos_maybe.is_none() {
                cursor_pos_maybe = poss_cursor_anim_pos;
            }

            if part.token.token.kind == TokenKind::Newline {
                line_idx += 1;
                content.newline();
                let ps2 = Span::styled(format!("{}∙", line_idx + 1), Palette::secondary_text());
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
                Tag::Blank,
            );
        }

        if self.mode.is_running()
            && let Some(cursor_pos) = cursor_pos_maybe
        {
            self.cursor_animation.update_position(cursor_pos);
            let cursor_anim_pos = if self.settings.disable_animations {
                cursor_pos
            } else {
                self.cursor_animation.get_position()
            };
            let cursor_style = {
                if self.settings.use_term_emulator_cursor {
                    None
                } else {
                    let cursor_intensity = if self.settings.disable_animations {
                        255
                    } else {
                        self.cursor_animation.get_intensity()
                    };
                    Some(Palette::cursor_style(cursor_intensity))
                }
            };

            content.set_term_cursor_pos(
                cursor_anim_pos,
                cursor_style,
                self.settings.use_term_emulator_cursor,
            );
        }

        if self.mode.is_running()
            && self.settings.tutorial_mode
            && self.buffer.buffer().is_empty()
            && matches!(self.content_mode, ContentMode::Normal)
        {
            content.write_span_dont_overwrite(
                &Span::styled(
                    " 💡 Start typing or search history with Ctrl+R",
                    Palette::tutorial_hint(),
                ),
                Tag::HistorySuggestion,
                None,
            );
            content.newline();
            content.write_span_dont_overwrite(
                &Span::styled(TUTORIAL_HISTORY_PREFIX_HINT, Palette::tutorial_hint()),
                Tag::HistorySuggestion,
                None,
            );
            content.newline();
            content.write_span_dont_overwrite(
                &Span::styled(TUTORIAL_DISABLE_HINT, Palette::tutorial_hint()),
                Tag::HistorySuggestion,
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
                        &Span::from(line.to_owned()).style(Palette::secondary_text()),
                        Tag::HistorySuggestion,
                        None,
                    );

                    if is_last {
                        let mut extra_info_text = format!(" #idx={}", sug.index);
                        if let Some(ts) = sug.timestamp {
                            let time_ago_str = Self::ts_to_timeago_string_5chars(ts);
                            extra_info_text.push_str(&format!(" {}", time_ago_str.trim_start()));
                        }

                        content.write_span_dont_overwrite(
                            &Span::from(extra_info_text).style(Palette::secondary_text()),
                            Tag::HistorySuggestion,
                            None,
                        );

                        if self.settings.tutorial_mode {
                            content.write_span_dont_overwrite(
                                &Span::styled(
                                    " 💡 Press → or End to accept",
                                    Palette::tutorial_hint(),
                                ),
                                Tag::HistorySuggestion,
                                None,
                            );
                        }
                    }
                });
        }

        match &mut self.content_mode {
            ContentMode::TabCompletion(active_suggestions) if self.mode.is_running() => {
                content.newline();
                let max_num_rows = 10; // TODO
                let mut rows: Vec<Vec<(Vec<Span>, usize)>> = vec![vec![]; max_num_rows];

                for (col, col_width) in active_suggestions.into_grid(max_num_rows, width as usize) {
                    for (row_idx, (formatted, is_selected)) in col.iter().enumerate() {
                        let formatted_suggestion = formatted.render(col_width, *is_selected);
                        rows[row_idx].push((formatted_suggestion, formatted.suggestion_idx));
                    }
                }

                let num_rows_used = rows.iter().filter(|r| !r.is_empty()).count();
                let num_logical_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);

                for row in rows.into_iter().filter(|r| !r.is_empty()) {
                    for (styled_spans, suggestion_idx) in row {
                        for span in styled_spans {
                            content.write_span(&span, Tag::Suggestion(suggestion_idx));
                        }
                    }
                    content.newline();
                }
                if num_rows_used == 0 {
                    content.write_span(
                        &Span::styled("No suggestions", Palette::secondary_text()),
                        Tag::TabSuggestion,
                    );
                }
                active_suggestions.update_grid_size(num_rows_used, num_logical_cols);
            }
            ContentMode::FuzzyHistorySearch if self.mode.is_running() => {
                content.newline();

                let (fuzzy_results, fuzzy_search_index, num_results, num_searched) = self
                    .history_manager
                    .get_fuzzy_search_results(self.buffer.buffer());

                let num_digits_for_index = num_searched.to_string().len();
                let num_digits_for_score = 3;
                let timeago_width = 5; // ts_to_timeago_string_5chars always returns 5 chars
                let indicator_width = 1; // "▐" or " "
                // Width of the header prefix: "{index} {score} {timeago}{indicator}"
                let header_prefix_width = (num_digits_for_index + 1)
                    + (num_digits_for_score + 1)
                    + timeago_width
                    + indicator_width;
                for (row_idx, formatted_entry) in fuzzy_results.iter_mut().enumerate() {
                    let entry = &formatted_entry.entry;
                    let mut spans = vec![];

                    spans.push(Span::styled(
                        format!("{:>num_digits_for_index$} ", entry.index + 1),
                        Palette::secondary_text(),
                    ));

                    spans.push(Span::styled(
                        format!("{:>num_digits_for_score$} ", formatted_entry.score),
                        Palette::secondary_text(),
                    ));

                    let timeago_str = entry
                        .timestamp
                        .map(Self::ts_to_timeago_string_5chars)
                        .unwrap_or("     ".to_string());

                    spans.push(Span::styled(timeago_str, Palette::secondary_text()));

                    let is_selected = fuzzy_search_index == row_idx;
                    if is_selected {
                        spans.push(Span::styled("▐", Palette::matched_character()));
                    } else {
                        spans.push(Span::styled(" ", Palette::secondary_text()));
                    }

                    let line = Line::from(spans);
                    content.write_line(&line, false, Tag::HistoryResult(row_idx));

                    let formatted_text = {
                        if formatted_entry.command_spans.is_none() {
                            // Lazily generate the formatted command with highlights
                            formatted_entry.gen_formatted_command();
                        }
                        formatted_entry.command_spans.as_ref().unwrap()
                    };

                    // Width available for command content on each terminal row
                    // (the header/indent prefix always occupies header_prefix_width columns)
                    let available_cols = content.width.saturating_sub(header_prefix_width as u16);

                    // Pre-process all logical lines into terminal display rows.
                    // Each element is: (is_start_of_logical_line, logical_line_idx, row_spans)
                    let total_logical_lines = formatted_text.len();
                    let mut all_display_rows: Vec<(bool, usize, Line<'static>)> = vec![];
                    for (logical_idx, logical_line) in formatted_text.iter().enumerate() {
                        let terminal_rows =
                            split_line_to_terminal_rows(logical_line, available_cols);
                        for (sub_idx, terminal_row) in terminal_rows.into_iter().enumerate() {
                            all_display_rows.push((sub_idx == 0, logical_idx, terminal_row));
                        }
                    }

                    let total_display_rows = all_display_rows.len();
                    let max_display_rows = if is_selected { 4 } else { 1 };
                    let has_more = total_display_rows > max_display_rows;
                    let rows_to_show = total_display_rows.min(max_display_rows);

                    for (display_idx, (is_start_of_logical, logical_idx, display_line)) in
                        all_display_rows
                            .into_iter()
                            .take(max_display_rows)
                            .enumerate()
                    {
                        if display_idx > 0 {
                            content.fill_line(Tag::HistoryResult(row_idx));
                            content.newline();
                            // Write indent prefix aligned to the header width.
                            // For the first terminal row of a new logical line, show "X/N"
                            // right-justified; for wrapped continuation rows, use blank padding.
                            // The last column of the prefix is the indicator column (▐ or space).
                            let indent_prefix = if is_start_of_logical {
                                let line_num_str =
                                    format!("{}/{}", logical_idx + 1, total_logical_lines);
                                format!(
                                    "{:>width$}",
                                    line_num_str,
                                    // header_prefix_width - 1: last column is the indicator
                                    width = header_prefix_width - 1
                                )
                            } else {
                                " ".repeat(header_prefix_width - 1)
                            };
                            content.write_span(
                                &Span::styled(indent_prefix, Palette::secondary_text()),
                                Tag::HistoryResult(row_idx),
                            );
                            // Write the indicator for every line of the selected entry
                            if is_selected {
                                content.write_span(
                                    &Span::styled("▐", Palette::matched_character()),
                                    Tag::HistoryResult(row_idx),
                                );
                            } else {
                                content.write_span(
                                    &Span::styled(" ", Palette::secondary_text()),
                                    Tag::HistoryResult(row_idx),
                                );
                            }
                        }

                        for span in &display_line.spans {
                            if is_selected {
                                let selected_span = Span::styled(
                                    span.content.clone(),
                                    Palette::convert_to_selected(span.style),
                                );
                                content.write_span(&selected_span, Tag::HistoryResult(row_idx));
                            } else {
                                content.write_span(span, Tag::HistoryResult(row_idx));
                            }
                        }

                        // Append ellipsis on the last displayed row when more content exists.
                        // If the row is full (cursor at the end), jump back one column to
                        // overwrite the last character; otherwise write the ellipsis right
                        // after the last character so it isn't pushed to the line's far end.
                        if display_idx + 1 == rows_to_show && has_more {
                            let ellipsis_style = if is_selected {
                                Palette::convert_to_selected(Palette::secondary_text())
                            } else {
                                Palette::secondary_text()
                            };
                            // "…" (U+2026) has a terminal display width of 1.
                            if content.cursor_position().col >= content.width.saturating_sub(1) {
                                content.set_cursor_col(content.width.saturating_sub(1));
                            }
                            content.write_span(
                                &Span::styled("…", ellipsis_style),
                                Tag::HistoryResult(row_idx),
                            );
                        }
                    }
                    content.fill_line(Tag::HistoryResult(row_idx));
                    content.newline();
                }
                content.write_span(
                    &Span::styled(
                        format!("# Fuzzy search: {}/{}", num_results, num_searched),
                        Palette::secondary_text(),
                    ),
                    Tag::FuzzySearch,
                );
                if self.settings.tutorial_mode {
                    content.newline();
                    content.write_span(
                        &Span::styled(TUTORIAL_FUZZY_SEARCH_HINT, Palette::tutorial_hint()),
                        Tag::FuzzySearch,
                    );
                }
            }
            ContentMode::Normal if self.mode.is_running() => {}
            ContentMode::AiMode {
                command_display,
                start_time,
                ..
            } if self.mode.is_running() => {
                content.newline();
                let elapsed_secs = start_time.elapsed().as_secs();
                content.write_span(
                    &Span::styled(
                        format!("Running: {} [{}s]", command_display, elapsed_secs),
                        Palette::secondary_text(),
                    ),
                    Tag::Blank,
                );
            }
            ContentMode::AiOutputSelection(selection) if self.mode.is_running() => {
                content.newline();
                for (row_idx, suggestion) in selection.suggestions.iter().enumerate() {
                    let is_selected = selection.selected_idx == row_idx;
                    let indicator = if is_selected { "▐" } else { " " };
                    let indicator_style = if is_selected {
                        Palette::matched_character()
                    } else {
                        Palette::secondary_text()
                    };
                    content.write_span(
                        &Span::styled(indicator, indicator_style),
                        Tag::AiResult(row_idx),
                    );
                    // Description line
                    let desc_style = if is_selected {
                        Palette::convert_to_selected(Palette::secondary_text())
                    } else {
                        Palette::secondary_text()
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
                        Some(Box::new(Self::wordinfo_fn)),
                    );
                    for part in &formatted_cmd.parts {
                        if matches!(part.token.token.kind, TokenKind::Newline) {
                            continue;
                        }
                        let span = part.span_to_use();
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
            }
            ContentMode::AiError {
                message,
                raw_output,
            } if self.mode.is_running() => {
                content.newline();
                content.write_span(
                    &Span::styled(
                        format!("AI failed: {}", message),
                        Style::default().fg(Color::Red),
                    ),
                    Tag::Blank,
                );
                if !raw_output.is_empty() {
                    for line in raw_output.lines().take(5) {
                        content.newline();
                        content.write_span(
                            &Span::styled(line.to_string(), Palette::secondary_text()),
                            Tag::Blank,
                        );
                    }
                }
            }
            _ => {}
        }
        if self.mode.is_running()
            && let Some(tooltip) = &self.tooltip
        {
            content.newline();
            let tooltip_line = Line::from(Span::styled(tooltip.clone(), Palette::secondary_text()));
            // Limit the tooltip to at most 3 terminal display rows so it
            // doesn't push other UI elements too far down the screen.
            const MAX_TOOLTIP_ROWS: usize = 3;
            let rows = split_line_to_terminal_rows(&tooltip_line, content.width);
            let truncated = rows.len() > MAX_TOOLTIP_ROWS;
            for (i, row) in rows.into_iter().take(MAX_TOOLTIP_ROWS).enumerate() {
                if i > 0 {
                    content.newline();
                }
                for span in &row.spans {
                    content.write_span(span, Tag::Tooltip);
                }
            }
            if truncated {
                let last_col = content.width.saturating_sub(1);
                if content.cursor_position().col >= last_col {
                    content.set_cursor_col(last_col);
                }
                content.write_span(&Span::styled("…", Palette::secondary_text()), Tag::Tooltip);
            }
        }

        content
    }

    fn ui(&mut self, frame: &mut Frame, content: Contents) {
        let frame_area = frame.area();
        frame.buffer_mut().reset();

        // Record where the viewport starts for smart mouse-mode position checks.
        self.last_viewport_top = frame_area.y;

        let (start_content_row, _end_content_row) =
            content.get_row_range_to_show(frame_area.height);

        for row_idx in 0..frame_area.height {
            match content.buf.get((start_content_row + row_idx) as usize) {
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

        if content.use_term_emulator_cursor
            && let Some(cursor_pos) = content.term_cursor_pos
        {
            let screen_row = cursor_pos.row.saturating_sub(start_content_row);
            if screen_row < frame_area.height && cursor_pos.col < frame_area.width {
                frame.set_cursor_position(Position {
                    x: cursor_pos.col,
                    y: screen_row + frame_area.y,
                });
            }
        }

        self.last_contents = Some((content, (frame_area.y as i16) - start_content_row as i16));
    }
}
