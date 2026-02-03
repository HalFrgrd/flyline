use crate::active_suggestions::{ActiveSuggestions, Suggestion};
use crate::bash_env_manager::BashEnvManager;
use crate::bash_funcs;
use crate::command_acceptance;
use crate::content_builder::{Contents, Tag};
use crate::cursor_animation::CursorAnimation;
use crate::history::{HistoryEntry, HistoryManager, HistorySearchDirection};
use crate::iter_first_last::FirstLast;
use crate::palette::Pallete;
use crate::prompt_manager::PromptManager;
use crate::snake_animation::SnakeAnimation;
use crate::tab_completion_context;
use crate::text_buffer::TextBuffer;
use crossterm::event::Event as CrosstermEvent;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, ModifierKeyCode, MouseEvent};
use futures::StreamExt;
use glob::glob;
use ratatui::prelude::*;
use ratatui::{Frame, TerminalOptions, Viewport, text::Line};
use std::boxed::Box;
use std::path::Path;
use std::time::{Duration, Instant};
use std::vec;
use timeago;

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

pub fn get_command() -> ExitState {
    // if let Err(e) = color_eyre::install() {
    //     log::error!("Failed to install color_eyre panic handler: {}", e);
    // }
    set_panic_hook();

    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::EnableBracketedPaste,
        crossterm::event::EnableFocusChange,
        crossterm::event::EnableMouseCapture,
        crossterm::event::PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
    .unwrap();

    let runtime = build_runtime();

    let end_state = runtime.block_on(App::new().run(backend));

    restore();

    log::debug!("Final state: {:?}", end_state);
    end_state
}

struct MouseState {
    enabled: bool,
}

impl MouseState {
    fn new() -> Self {
        MouseState { enabled: true }
    }

    fn enable(&mut self) {
        match crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture) {
            Ok(_) => {
                log::debug!("Enabled mouse capture");
                self.enabled = true;
            }
            Err(e) => {
                log::error!("Failed to enable mouse capture: {}", e);
            }
        }
    }
    fn disable(&mut self) {
        match crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture) {
            Ok(_) => {
                log::debug!("Disabled mouse capture");
                self.enabled = false;
            }
            Err(e) => {
                log::error!("Failed to disable mouse capture: {}", e);
            }
        }
    }

    fn toggle(&mut self) {
        if self.enabled {
            self.disable();
        } else {
            self.enable();
        }
    }
}

#[derive(Debug)]
enum ContentMode {
    Normal,
    FuzzyHistorySearch,
    TabCompletion(ActiveSuggestions),
}

struct App {
    mode: AppRunningState,
    buffer: TextBuffer,
    animation_tick: u64,
    cursor_animation: CursorAnimation,
    prompt_manager: PromptManager,
    home_path: String,
    /// Parsed bash history available at startup.
    history_manager: HistoryManager,
    bash_env: BashEnvManager,
    snake_animation: SnakeAnimation,
    history_suggestion: Option<(HistoryEntry, String)>,
    mouse_state: MouseState,
    content_mode: ContentMode,
    last_contents: Option<(Contents, i16)>,
}

impl App {
    fn new() -> Self {
        let user = bash_funcs::get_env_variable("USER").unwrap_or("user".into());

        let home_path =
            bash_funcs::get_env_variable("HOME").unwrap_or("/home/".to_string() + &user);

        let unfinished_from_prev_command =
            unsafe { crate::bash_symbols::current_command_line_count } > 0;

        App {
            mode: AppRunningState::Running,
            buffer: TextBuffer::new(""),
            animation_tick: 0,
            cursor_animation: CursorAnimation::new(),
            prompt_manager: PromptManager::new(unfinished_from_prev_command),
            home_path: home_path,
            history_manager: HistoryManager::new(),
            bash_env: BashEnvManager::new(), // TODO: This is potentially expensive, load in background?
            snake_animation: SnakeAnimation::new(),
            history_suggestion: None,
            mouse_state: MouseState::new(),
            content_mode: ContentMode::Normal,
            last_contents: None,
        }
    }

    pub async fn run(
        mut self,
        backend: ratatui::backend::CrosstermBackend<std::io::Stdout>,
    ) -> ExitState {
        // Clear any pending events before creating terminal
        // This helps prevent cursor position query timeouts
        log::debug!("Clearing any pending events before terminal creation");
        let clear_start = Instant::now();
        while crossterm::event::poll(Duration::from_millis(0)).unwrap_or(false) {
            if let Ok(event) = crossterm::event::read() {
                log::debug!("Discarded pending event: {:?}", event);
            }
            if clear_start.elapsed().as_millis() > 50 {
                log::warn!("Took too long clearing events, continuing anyway");
                break;
            }
        }

        let options = TerminalOptions {
            viewport: Viewport::Inline(0),
        };
        let mut terminal =
            ratatui::Terminal::with_options(backend, options).expect("Failed to create terminal");
        terminal.hide_cursor().unwrap();
        log::debug!(
            "Initial terminal cursor position: {:?}",
            terminal.get_cursor_position()
        );

        // Set up event stream and timers directly
        let mut reader = crossterm::event::EventStream::new();
        let mut time_since_last_input = Instant::now();

        const ANIMATION_FPS_MAX: u64 = 60;
        const ANIMATION_FPS_MIN: u64 = 5;
        const ANIM_SWITCH_INACTIVITY_START: u128 = 10000;
        const ANIM_SWITCH_INACTIVITY_LEN: u128 = 10000;

        let anim_period = Duration::from_millis(1000 / ANIMATION_FPS_MAX);
        let mut anim_tick = tokio::time::interval(anim_period);

        // Track last resize time to suppress animations during and after resize
        let mut last_resize_time: Option<Instant> = None;
        const RESIZE_COOLDOWN_MS: u128 = 200;

        let mut redraw = true;
        let mut needs_screen_cleared = false;
        let mut last_terminal_area = terminal.size().unwrap();
        let mut last_terminal_area_on_render = terminal.size().unwrap();

        loop {
            let been_long_enough_since_last_resize = if let Some(resize_time) = last_resize_time {
                resize_time.elapsed().as_millis() >= RESIZE_COOLDOWN_MS
            } else {
                true
            };

            if redraw && been_long_enough_since_last_resize {
                if last_terminal_area_on_render != last_terminal_area {
                    terminal.autoresize().unwrap_or_else(|e| {
                        log::error!("Failed to autoresize terminal: {}", e);
                    });
                    last_terminal_area_on_render = last_terminal_area;
                }

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

                    // Retry up to 10 times to verify cursor position
                    for attempt in 0..10 {
                        match terminal.get_cursor_position() {
                            Ok(actual_pos) => {
                                if actual_pos == pos {
                                    log::debug!(
                                        "Cursor position verified at ({}, {}) on attempt {}",
                                        actual_pos.x,
                                        actual_pos.y,
                                        attempt + 1
                                    );
                                    break;
                                } else {
                                    log::debug!(
                                        "Cursor position mismatch: expected ({}, {}), got ({}, {}) on attempt {}",
                                        pos.x,
                                        pos.y,
                                        actual_pos.x,
                                        actual_pos.y,
                                        attempt + 1
                                    );
                                    if attempt < 9 {
                                        tokio::time::sleep(Duration::from_millis(10)).await;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to get cursor position on attempt {}: {}",
                                    attempt + 1,
                                    e
                                );
                                if attempt < 9 {
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                            }
                        }
                    }
                }
            }

            if !self.mode.is_running() {
                break;
            }

            // Event handling with tokio::select
            let anim_tick_delay = anim_tick.tick();

            // log::debug!("Waiting for events...");

            redraw = tokio::select! {
                _ = anim_tick_delay => {
                    self.animation_tick = self.animation_tick.wrapping_add(1);

                    // Adjust animation FPS based on inactivity
                    let inactivity_duration = time_since_last_input.elapsed().as_millis();
                    let x: f32 = inactivity_duration.saturating_sub(ANIM_SWITCH_INACTIVITY_START) as f32 / ANIM_SWITCH_INACTIVITY_LEN as f32;
                    let x = x.max(0.0).min(1.0);
                    let fps = (ANIMATION_FPS_MAX as f32 * (1.0 - x)) + (ANIMATION_FPS_MIN as f32 * x);
                    assert!(fps >= 0.0);
                    let period = Duration::from_millis((1000.0 / fps) as u64);
                    anim_tick = tokio::time::interval_at((Instant::now() + period).into(), period);

                    true
                }
                Some(Ok(evt)) = reader.next() => {
                    time_since_last_input = Instant::now();

                    match evt {
                        CrosstermEvent::Key(key) => {
                            match key.kind {
                                crossterm::event::KeyEventKind::Press | crossterm::event::KeyEventKind::Repeat => {
                                    log::debug!("Key event: {:?}", key);
                                    needs_screen_cleared = self.on_keypress(key);
                                    true
                                }
                                crossterm::event::KeyEventKind::Release => {
                                    self.on_keyrelease(key);
                                    false
                                }
                            }
                        }
                        CrosstermEvent::Mouse(mouse) => {
                            self.on_mouse(mouse)
                        }
                        CrosstermEvent::Resize(new_cols, new_rows) => {
                            log::debug!("Terminal resized to {}x{}", new_cols, new_rows);
                            last_terminal_area = Size {
                                width: new_cols,
                                height: new_rows,
                            };

                            last_resize_time = Some(Instant::now());
                            true
                        }
                        CrosstermEvent::FocusLost => {
                            // log::debug!("Terminal focus lost");
                            false
                        },
                        CrosstermEvent::FocusGained => {
                            // log::debug!("Terminal focus gained");
                            false
                        },
                        CrosstermEvent::Paste(pasted) => {
                            self.buffer.insert_str(&pasted);
                            self.on_possible_buffer_change();
                            true
                        },
                    }
                }
            };
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

    fn on_mouse(&mut self, mouse: MouseEvent) -> bool {
        log::debug!("Mouse event: {:?}", mouse);

        match mouse.kind {
            crossterm::event::MouseEventKind::Down(_) => {
                log::debug!("Mouse down event at ({}, {})", mouse.column, mouse.row);
                if let Some((contents, offset)) = &self.last_contents {
                    if let Some(tagged_cell) =
                        contents.get_tagged_cell(mouse.column, mouse.row, *offset)
                    {
                        log::debug!(
                            "Mouse moved over cell at ({}, {}): {:?}",
                            mouse.column,
                            mouse.row,
                            tagged_cell
                        );
                    }
                }
            }
            e => {
                log::debug!("Mouse event: {:?}", e);
            }
        };
        false
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
    fn on_keypress(&mut self, key: KeyEvent) -> bool {
        match key {
            KeyEvent {
                code: KeyCode::Left,
                ..
            } if matches!(self.content_mode, ContentMode::TabCompletion(_)) => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.on_left_arrow();
                }
            }
            KeyEvent {
                code: KeyCode::Right,
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
            } if matches!(self.content_mode, ContentMode::FuzzyHistorySearch) => {
                self.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::Forward);
            }
            // Handle Up with history navigation when at first line
            KeyEvent {
                code: KeyCode::Up, ..
            } if self.buffer.cursor_row() == 0 => {
                if let Some(entry) = self
                    .history_manager
                    .search_in_history(self.buffer.buffer(), HistorySearchDirection::Backward)
                {
                    let new_command = entry.command.clone();
                    self.buffer.replace_buffer(new_command.as_str());
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
            // Handle Down with history navigation when at last line
            KeyEvent {
                code: KeyCode::Down,
                ..
            } if self.buffer.is_cursor_on_final_line() => {
                if let Some(entry) = self
                    .history_manager
                    .search_in_history(self.buffer.buffer(), HistorySearchDirection::Forward)
                {
                    let new_command = entry.command.clone();
                    self.buffer.replace_buffer(new_command.as_str());
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
            } => {
                match &mut self.content_mode {
                    ContentMode::FuzzyHistorySearch => {
                        if let Some(entry) = self.history_manager.accept_fuzzy_search_result() {
                            let new_command = entry.command.clone();
                            self.buffer.replace_buffer(new_command.as_str());
                        }
                        self.content_mode = ContentMode::Normal;
                    }
                    ContentMode::TabCompletion(active_suggestions) => {
                        active_suggestions.accept_currently_selected(&mut self.buffer);
                        self.content_mode = ContentMode::Normal;
                    }
                    ContentMode::Normal => {
                        // If it's a single line complete command, exit
                        // If it's a multi-line complete command, cursor needs to be at end to exit
                        if ((self.buffer.lines_with_cursor().iter().count() == 1)
                            || self.buffer.is_cursor_at_trimmed_end())
                            && command_acceptance::will_bash_accept_buffer(&self.buffer.buffer())
                        {
                            self.mode = AppRunningState::Exiting(ExitState::WithCommand(
                                self.buffer.buffer().to_string(),
                            ));
                        } else {
                            self.buffer.insert_newline();
                        }
                    }
                }
            }
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
            } => {
                // if the word under the cursor has changed, reset active suggestions
                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    if !self
                        .buffer
                        .substring_matches(&active_suggestions.word_under_cursor)
                        || !self
                            .buffer
                            .cursor_in_substring(&active_suggestions.word_under_cursor)
                    {
                        log::debug!("Word under cursor changed, clearing active suggestions");
                        self.content_mode = ContentMode::Normal;
                    }
                }

                if let ContentMode::TabCompletion(active_suggestions) = &mut self.content_mode {
                    active_suggestions.on_tab(false);
                } else {
                    self.start_tab_complete();
                }
            }

            // Escape - clear suggestions or toggle mouse
            KeyEvent {
                code: KeyCode::Esc, ..
            } => match self.content_mode {
                ContentMode::TabCompletion(_) | ContentMode::FuzzyHistorySearch => {
                    self.content_mode = ContentMode::Normal;
                }
                ContentMode::Normal => {
                    self.mouse_state.toggle();
                }
            },
            // Ctrl+C - cancel with comment
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::META,
                ..
            } => {
                self.buffer.move_to_end();
                self.buffer.insert_str(" #[Ctrl+C pressed] ");
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
                }
            }
            KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                // Clear screen
                return true;
            }
            KeyEvent {
                code: KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                // Idea is that when Alt/Meta is held down, mouse is toggled
                // But not all terminals send key release events for Alt/Meta
                // So we toggle on both press and release
                self.mouse_state.toggle();
            }
            // Delegate basic text editing to TextBuffer
            _ => {
                self.buffer.on_keypress(key);
            }
        }

        self.on_possible_buffer_change();
        return false;
    }

    fn on_keyrelease(&mut self, key: KeyEvent) {
        match key {
            KeyEvent {
                code: KeyCode::Modifier(ModifierKeyCode::LeftAlt),
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                self.mouse_state.toggle();
            }
            _ => {}
        }
    }

    fn on_possible_buffer_change(&mut self) {
        self.history_suggestion = self
            .history_manager
            .get_command_suggestion_suffix(self.buffer.buffer());

        let first_word = {
            let line = self.buffer.buffer();
            let space_pos = line.find(' ').unwrap_or(line.len());
            &line[0..space_pos]
        }
        .to_owned();
        // log::debug!("Caching command type for first word: {}", first_word);
        self.bash_env.cache_command_type(&first_word);
    }

    fn try_accept_tab_completion(&mut self, opt_suggestion: Option<ActiveSuggestions>) {
        match opt_suggestion.and_then(|s| s.try_accept(&mut self.buffer)) {
            None => {
                self.content_mode = ContentMode::Normal;
            }
            Some(suggestions) => {
                self.content_mode = ContentMode::TabCompletion(suggestions);
            }
        }
    }

    fn start_tab_complete(&mut self) {
        let buffer: &str = self.buffer.buffer();
        let completion_context =
            tab_completion_context::get_completion_context(buffer, self.buffer.cursor_byte_pos());

        log::debug!("Completion context: {:?}", completion_context);

        let word_under_cursor = completion_context.word_under_cursor;

        match completion_context.comp_type {
            tab_completion_context::CompType::FirstWord => {
                let completions = self.tab_complete_first_word(word_under_cursor);
                log::debug!("First word completions: {:?}", completions);
                self.try_accept_tab_completion(ActiveSuggestions::try_new(
                    completions,
                    word_under_cursor,
                    &self.buffer,
                ));
            }
            tab_completion_context::CompType::CommandComp { mut command_word } => {
                // This isnt just for commands like `git`, `cargo`
                // Because we call bash_symbols::programmable_completions
                // Bash also completes env vars (`echo $HO`) and other useful completions.
                // Bash doesnt handle alias expansion well:
                // https://www.reddit.com/r/bash/comments/eqwitd/programmable_completion_on_expanded_aliases_not/
                // Since aliases are the highest priority in command word resolution,
                // If it is an alias, lets expand it here for better completion results.
                let poss_alias = bash_funcs::find_alias(&command_word);
                log::debug!(
                    "Checking for alias for command word '{}': {:?}",
                    command_word,
                    poss_alias
                );

                let alias = if let Some(a) = poss_alias
                    && !a.is_empty()
                {
                    a
                } else {
                    command_word.clone()
                };

                let len_delta = alias.len() as isize - command_word.len() as isize;
                let word_under_cursor_end = {
                    let word_start_offset_in_context = word_under_cursor.as_ptr() as usize
                        - completion_context.context.as_ptr() as usize;
                    (word_start_offset_in_context + word_under_cursor.len())
                        .saturating_add_signed(len_delta)
                };

                // this it the cursor position relative to the start of the completion context
                let cursor_byte_pos = completion_context
                    .context_until_cursor
                    .len()
                    .saturating_add_signed(len_delta);

                let full_command =
                    alias.to_string() + &completion_context.context[command_word.len()..];
                command_word = alias.split_whitespace().next().unwrap().to_string();

                let poss_completions = bash_funcs::run_autocomplete_compspec(
                    &full_command,
                    &command_word,
                    &word_under_cursor,
                    cursor_byte_pos,
                    word_under_cursor_end,
                );
                match poss_completions {
                    Ok(completions) => {
                        log::debug!("Bash autocomplete results for command: {}", full_command);
                        self.try_accept_tab_completion(ActiveSuggestions::try_new(
                            Suggestion::from_string_vec(completions, "", " "),
                            word_under_cursor,
                            &self.buffer,
                        ));
                    }
                    Err(e) => {
                        log::debug!(
                            "Bash autocompletion failed for command: {} with error: {}. Falling back to glob expansion.",
                            full_command,
                            e
                        );
                        let completions = self.tab_complete_current_path(word_under_cursor);
                        self.try_accept_tab_completion(ActiveSuggestions::try_new(
                            completions,
                            word_under_cursor,
                            &self.buffer,
                        ));
                    }
                }
            }
            // tab_completion::CompType::CursorOnBlank(word_under_cursor) => {
            //     log::debug!("Cursor is on blank space, no tab completion performed");
            //     let completions = self.tab_complete_current_path("");
            //     self.active_tab_suggestions = ActiveSuggestions::try_new(
            //         completions
            //             .into_iter()
            //             .map(|mut sug| {
            //                 sug.prefix = " ".to_string();
            //                 sug
            //             })
            //             .collect(),
            //         word_under_cursor,
            //         &mut self.buffer,
            //     );
            // }
            tab_completion_context::CompType::EnvVariable => {
                log::debug!(
                    "Environment variable completion not yet implemented: {:?}",
                    word_under_cursor
                );
            }
            tab_completion_context::CompType::TildeExpansion => {
                log::debug!("Tilde expansion completion: {:?}", word_under_cursor);
                let completions = self.tab_complete_tilde_expansion(&word_under_cursor);
                self.try_accept_tab_completion(ActiveSuggestions::try_new(
                    completions,
                    word_under_cursor,
                    &self.buffer,
                ));
            }
            tab_completion_context::CompType::GlobExpansion => {
                log::debug!("Glob expansion for: {:?}", word_under_cursor);
                let completions = self.tab_complete_glob_expansion(&word_under_cursor);

                // Unlike other completions, if there are multiple glob completions,
                // we join them with spaces and insert them all at once.
                let completions_as_string = completions.iter().map(|sug| sug.s.clone()).fold(
                    String::new(),
                    |mut acc, s| {
                        if !acc.is_empty() {
                            acc.push(' ');
                        }
                        acc.push_str(&s);
                        acc
                    },
                );
                if completions_as_string.is_empty() {
                    log::debug!(
                        "No glob expansion completions found for pattern: {}",
                        word_under_cursor
                    );
                } else {
                    self.try_accept_tab_completion(ActiveSuggestions::try_new(
                        Suggestion::from_string_vec(vec![completions_as_string], "", " "),
                        word_under_cursor,
                        &self.buffer,
                    ));
                }
            }
        }
    }

    fn tab_complete_first_word(&self, command: &str) -> Vec<Suggestion> {
        if command.is_empty() {
            return vec![];
        }

        if command.starts_with('.') || command.starts_with('/') {
            // Path to executable
            return self.tab_complete_glob_expansion(&(command.to_string() + "*"));
        }

        let mut res = self.bash_env.get_first_word_completions(&command);

        // TODO: could prioritize based on frequency of use
        res.sort();
        res.sort_by_key(|s| s.len());

        let mut seen = std::collections::HashSet::new();
        res.retain(|s| seen.insert(s.clone()));
        Suggestion::from_string_vec(res, "", " ")
    }

    fn tab_complete_current_path(&self, pattern: &str) -> Vec<Suggestion> {
        self.tab_complete_glob_expansion(&(pattern.to_string() + "*"))
    }

    fn expand_path_pattern(&self, pattern: &str) -> (String, Vec<(String, String)>) {
        // TODO expand other variables?
        let mut prefixes_swaps = vec![];
        let mut pattern = pattern.to_string();
        if pattern.starts_with("~/") {
            prefixes_swaps.push((self.home_path.to_string() + "/", "~/".to_string()));
            pattern = pattern.replace(&prefixes_swaps[0].1, &prefixes_swaps[0].0);
        }

        // Resolve the pattern relative to cwd if it's not absolute
        if !Path::new(&pattern).is_absolute() {
            // Get the current working directory for relative paths
            if let Ok(cwd) = std::env::current_dir() {
                if let Some(cwd_str) = cwd.to_str() {
                    prefixes_swaps.push((format!("{}/", cwd_str), "".to_string()));
                    pattern = format!("{}/{}", cwd_str, pattern);
                }
            }
        }

        (pattern, prefixes_swaps)
    }

    fn tab_complete_glob_expansion(&self, pattern: &str) -> Vec<Suggestion> {
        log::debug!("Performing glob expansion for pattern: {}", pattern);
        let (resolved_pattern, prefixes_swaps) = self.expand_path_pattern(pattern);
        log::debug!(
            "resolved_pattern: {} {:?}",
            resolved_pattern,
            prefixes_swaps
        );

        // Use glob to find matching paths
        let mut results = Vec::new();

        const MAX_GLOB_RESULTS: usize = 1_000;

        if let Ok(paths) = glob(&resolved_pattern) {
            for (idx, path_result) in paths.enumerate() {
                if idx >= MAX_GLOB_RESULTS {
                    log::debug!(
                        "Reached maximum glob results limit of {}. Stopping further processing.",
                        MAX_GLOB_RESULTS
                    );
                    break;
                }
                if let Ok(path) = path_result {
                    // Convert the path to a string relative to cwd (or absolute if pattern was absolute)
                    let unexpanded = {
                        let mut p = path.to_string_lossy().to_string();

                        for (prefix_to_remove, prefix_to_replace) in &prefixes_swaps {
                            if p.starts_with(prefix_to_remove) {
                                p = p.replacen(prefix_to_remove, prefix_to_replace, 1);
                            } else {
                                log::warn!(
                                    "Expected path '{}' to start with prefix '{}', but it did not.",
                                    p,
                                    prefix_to_remove
                                );
                                break;
                            }
                        }
                        p
                    };

                    // Add trailing slash for directories
                    if path.is_dir() {
                        // no trailing space for directories
                        results.push(Suggestion::new(
                            format!("{}/", unexpanded),
                            "".to_string(),
                            "".to_string(),
                        ));
                    } else {
                        // trailing space for files
                        results.push(Suggestion::new(unexpanded, "".to_string(), " ".to_string()));
                    }
                }
            }
        }

        results.sort();
        results
    }

    fn tab_complete_tilde_expansion(&self, pattern: &str) -> Vec<Suggestion> {
        let user_pattern = if pattern.starts_with('~') {
            &pattern[1..]
        } else {
            return vec![];
        };

        self.tab_complete_glob_expansion(&("/home/".to_string() + user_pattern + "*"))
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

    fn create_content(self: &mut Self, width: u16) -> Contents {
        // Basically build the entire frame in a Content first
        // Then figure out how to fit that into the actual frame area
        let mut content = Contents::new(width);

        for (_, is_last, line) in self.prompt_manager.get_ps1_lines().iter().flag_first_last() {
            content.write_line(&line, !is_last, Tag::Ps1Prompt);
        }

        let mut command_description: Option<String> = None;

        for (is_first, _, (line_idx, (line, cursor_col))) in self
            .buffer
            .lines_with_cursor()
            .iter()
            .enumerate()
            .flag_first_last()
        {
            let line_offset: u16;
            if is_first {
                let space_pos = line.find(' ').unwrap_or(line.len());
                let (first_word, rest) = line.split_at(space_pos);

                let (command_type, short_desc) = self.bash_env.get_command_info(first_word);
                if !short_desc.is_empty() {
                    command_description = Some(short_desc.to_owned());
                }

                let first_word = if first_word.starts_with("python") && self.mode.is_running() {
                    self.snake_animation.update_anim();
                    let snake_chars: Vec<char> = self.snake_animation.to_string().chars().collect();

                    first_word
                        .chars()
                        .enumerate()
                        .map(|(i, original_char)| {
                            snake_chars
                                .get(i)
                                .filter(|&&snake_char| snake_char != '⠀')
                                .unwrap_or(&original_char)
                                .to_owned()
                        })
                        .collect()
                } else {
                    first_word.to_string()
                };

                let first_word_style: Style = match command_type {
                    bash_funcs::CommandType::Unknown => Pallete::unrecognised_word(),
                    _ => Pallete::recognised_word(),
                };
                line_offset = content.cursor_position().0;
                content.write_span(
                    &Span::styled(first_word, first_word_style),
                    Tag::CommandFirstWord,
                );
                content.write_span(
                    &Span::styled(rest.to_string(), Pallete::normal_text()),
                    Tag::CommandOther,
                );
            } else {
                content.newline();
                let ps2 = Span::styled(format!("{}∙", line_idx + 1), Pallete::secondary_text());
                content.write_span(&ps2, Tag::Ps2Prompt);
                line_offset = content.cursor_position().0;
                content.write_line(&Line::from(line.to_owned()), false, Tag::CommandOther);
            }
            // Draw cursor on this line
            if self.mode.is_running()
                && let Some(cursor_col_in_line) = cursor_col
            {
                let cursor_logical_col = cursor_col_in_line + line_offset;
                let cursor_logical_row = content.cursor_logical_row();

                let (vis_row, vis_col) =
                    content.cursor_logical_to_visual(cursor_logical_row, cursor_logical_col);
                self.cursor_animation.update_position(vis_row, vis_col);
                let (animated_vis_row, animated_vis_col) = self.cursor_animation.get_position();

                let cursor_style = {
                    let cursor_intensity = self.cursor_animation.get_intensity();
                    Pallete::cursor_style(cursor_intensity)
                };

                content.set_edit_cursor_style(animated_vis_row, animated_vis_col, cursor_style);
            }
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

                    content.write_span(
                        &Span::from(line.to_owned()).style(Pallete::secondary_text()),
                        Tag::HistorySuggestion,
                    );

                    if is_last {
                        let mut extra_info_text = " #".to_string();
                        if let Some(ts) = sug.timestamp {
                            let time_ago_str = Self::ts_to_timeago_string_5chars(ts);
                            extra_info_text
                                .push_str(&format!(" {} ago", time_ago_str.trim_start()));
                        }
                        extra_info_text.push_str(&format!(" idx={}", sug.index));

                        content.write_span(
                            &Span::from(extra_info_text).style(Pallete::secondary_text()),
                            Tag::HistorySuggestion,
                        );
                    }
                });
        }

        match &mut self.content_mode {
            ContentMode::TabCompletion(active_suggestions) if self.mode.is_running() => {
                content.newline();
                let max_num_rows = 10;
                let mut rows = vec![vec![]; max_num_rows];

                for (col, col_width) in active_suggestions.into_grid(max_num_rows, width as usize) {
                    for (row_idx, (suggestion, is_selected)) in col.iter().enumerate() {
                        let style = if *is_selected {
                            Pallete::selection_style()
                        } else {
                            Pallete::normal_text()
                        };

                        let word = if suggestion.len() > col_width {
                            let mut truncated = suggestion[..col_width - 1].to_string();
                            truncated.push('…');
                            truncated
                        } else {
                            suggestion.to_string() + &" ".repeat(col_width - suggestion.len())
                        };

                        rows[row_idx].push((word, style));
                    }
                }

                let num_rows_used = rows.iter().filter(|r| !r.is_empty()).count();
                let num_logical_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);

                for row in rows.into_iter().filter(|r| !r.is_empty()) {
                    let mut line = vec![];
                    for (word, style) in row {
                        line.push(Span::styled(word, style));
                    }
                    content.write_line(&Line::from(line), true, Tag::TabSuggestion);
                }
                active_suggestions.update_grid_size(num_rows_used, num_logical_cols);
            }
            ContentMode::FuzzyHistorySearch if self.mode.is_running() => {
                content.newline();

                let (fuzzy_results, fuzzy_search_index, num_results, num_searched) = self
                    .history_manager
                    .get_fuzzy_search_results(self.buffer.buffer());
                for (row_idx, entry_with_indices) in fuzzy_results.iter().enumerate() {
                    let entry = &entry_with_indices.0;
                    let mut spans = vec![];

                    spans.push(Span::styled(
                        format!("{} ", entry.index + 1),
                        Pallete::secondary_text(),
                    ));

                    let timeago_str = entry
                        .timestamp
                        .map(|ts| Self::ts_to_timeago_string_5chars(ts));
                    if let Some(timeago) = timeago_str {
                        spans.push(Span::styled(timeago, Pallete::secondary_text()));
                    }

                    if fuzzy_search_index == row_idx {
                        spans.push(Span::styled("▐", Pallete::matched_character()));
                    } else {
                        spans.push(Span::styled(" ", Pallete::secondary_text()));
                    }

                    let match_indices_set: std::collections::HashSet<usize> =
                        entry_with_indices.1.iter().cloned().collect();
                    for (idx, ch) in entry.command.chars().enumerate() {
                        let mut style = if match_indices_set.contains(&idx) {
                            Pallete::matched_character()
                        } else {
                            Pallete::normal_text()
                        };
                        if fuzzy_search_index == row_idx {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                        spans.push(Span::styled(ch.to_string(), style));
                    }

                    let line = Line::from(spans);
                    content.write_line(&line, true, Tag::FuzzySearch);
                }
                content.write_span(
                    &Span::styled(
                        format!("# Fuzzy search: {}/{}", num_results, num_searched),
                        Pallete::secondary_text(),
                    ),
                    Tag::FuzzySearch,
                );
            }
            _ => {}
        }
        content
    }

    fn ui(&mut self, frame: &mut Frame, content: Contents) {
        let frame_area = frame.area();
        frame.buffer_mut().reset();

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

        self.last_contents = Some((content, (frame_area.y as i16) - start_content_row as i16));
    }
}
