use crate::active_suggestions::{ActiveSuggestions, Suggestion};
use crate::bash_env_manager::BashEnvManager;
use crate::bash_funcs;
use crate::command_acceptance;
use crate::content_builder::Contents;
use crate::cursor_animation::CursorAnimation;
use crate::history::{HistoryEntry, HistoryManager, HistorySearchDirection};
use crate::iter_first_last::FirstLast;
use crate::prompt_manager::PromptManager;
use crate::snake_animation::SnakeAnimation;
use crate::tab_completion_context;
use crate::text_buffer::TextBuffer;
use crossterm::event::Event as CrosstermEvent;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use futures::StreamExt;
use ratatui::prelude::*;
use ratatui::{Frame, TerminalOptions, Viewport, text::Line};
use std::boxed::Box;
use std::time::{Duration, Instant};
use std::vec;

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

pub fn get_command() -> AppRunningState {
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
        // crossterm::event::EnableMouseCapture,
        crossterm::event::PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
    .unwrap();

    let runtime = build_runtime();

    let mut app = App::new();
    let end_state = runtime.block_on(app.run(backend));

    restore();

    log::debug!("Final state: {:?}", end_state);
    end_state
}

struct MouseState {
    enabled: bool,
}

impl MouseState {
    fn new() -> Self {
        MouseState { enabled: false }
    }

    fn enable(&mut self) {
        use std::io::Write;

        let mut f = std::io::stdout();

        let _ = f.write_all(
            concat!(
                // Normal tracking: Send mouse X & Y on button press and release
                "\x1b[?1000h",
                // Button-event tracking: Report button motion events (dragging)
                "\x1b[?1002h",
                // Any-event tracking: Report all motion events
                // "\x1b[?1003h",
                // RXVT mouse mode: Allows mouse coordinates of >223
                "\x1b[?1015h",
                // SGR mouse mode: Allows mouse coordinates of >223, preferred over RXVT mode
                "\x1b[?1006h",
            )
            .as_bytes(),
        );

        let _ = f.flush();
        self.enabled = true;
    }
    fn disable(&mut self) {
        crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture)
            .unwrap_or_else(|e| {
                log::error!("Failed to disable mouse capture: {}", e);
            });

        self.enabled = false;
    }

    fn toggle(&mut self) {
        if self.enabled {
            self.disable();
        } else {
            self.enable();
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum AppRunningState {
    Running,
    ExitingWithCommand(String),
    ExitingWithoutCommand,
}

impl AppRunningState {
    pub fn is_running(&self) -> bool {
        *self == AppRunningState::Running
    }
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
    command_word_cells: Vec<(u16, u16)>,
    should_show_command_info: bool,
    mouse_state: MouseState,
    active_tab_suggestions: Option<ActiveSuggestions>,
}

impl App {
    fn new() -> Self {
        // TODO: fetch these in background

        let ps1_prompt = bash_builtins::variables::find_as_string("PS1")
            .as_ref()
            .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            .unwrap_or("default> ".into());

        let user = bash_builtins::variables::find_as_string("USER")
            .as_ref()
            .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            .unwrap_or("user".into());

        let home_path = bash_builtins::variables::find_as_string("HOME")
            .as_ref()
            .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            .unwrap_or("/home/".to_string() + &user);

        let unfinished_from_prev_command =
            unsafe { crate::bash_symbols::current_command_line_count } > 0;

        // history.new_session();
        App {
            mode: AppRunningState::Running,
            buffer: TextBuffer::new(""),
            animation_tick: 0,
            cursor_animation: CursorAnimation::new(),
            prompt_manager: PromptManager::new(ps1_prompt, unfinished_from_prev_command),
            home_path: home_path,
            history_manager: HistoryManager::new(),
            bash_env: BashEnvManager::new(), // TODO: This is potentially expensive, load in background?
            snake_animation: SnakeAnimation::new(),
            history_suggestion: None,
            command_word_cells: Vec::new(),
            should_show_command_info: false,
            mouse_state: MouseState::new(),
            active_tab_suggestions: None,
        }
    }

    pub async fn run(
        &mut self,
        backend: ratatui::backend::CrosstermBackend<std::io::Stdout>,
    ) -> AppRunningState {
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

                let desired_height = content.height().min(last_terminal_area.height);
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
                            if key.kind == crossterm::event::KeyEventKind::Press {
                                self.on_keypress(key);
                                true
                            } else {
                                false
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
                            log::debug!("Terminal focus lost");
                            false
                        },
                        CrosstermEvent::FocusGained => {
                            log::debug!("Terminal focus gained");
                            false
                        },
                        CrosstermEvent::Paste(pasted) => {
                            self.buffer.insert_str(&pasted);
                            true
                        },
                    }
                }
            };
        }

        self.mode.clone()
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            // MouseEventKind::Moved => {
            //     if !self.mouse_state.update_on_move() {
            //         // log::debug!("Mouse move ignored due to rapid movement");
            //         return false;
            //     }
            //     self.should_show_command_info = false;
            //     for (cell_row, cell_col) in &self.command_word_cells {
            //         if *cell_row == mouse.row && *cell_col == mouse.column {
            //             log::debug!("Hovering on first word at ({}, {})", cell_row, cell_col);
            //             // Additional logic can be added here if needed
            //             self.should_show_command_info = true;
            //         }
            //     }
            // }
            e => {
                log::debug!("Mouse event: {:?}", e);
            }
        };
        false
    }

    fn on_keypress(&mut self, key: KeyEvent) {
        // log::debug!("Key pressed: {:?}", key);

        match key {
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
                if let Some(active_suggestions) = self.active_tab_suggestions.take() {
                    active_suggestions.accept_currently_selected(&mut self.buffer);
                } else {
                    // If it's a single line complete command, exit
                    // If it's a multi-line complete command, cursor needs to be at end to exit
                    if ((self.buffer.lines_with_cursor().iter().count() == 1)
                        || self.buffer.is_cursor_at_trimmed_end())
                        && command_acceptance::will_bash_accept_buffer(&self.buffer.buffer())
                    {
                        self.mode =
                            AppRunningState::ExitingWithCommand(self.buffer.buffer().to_string());
                    } else {
                        self.buffer.insert_newline();
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
                if let Some(active_suggestions) = &mut self.active_tab_suggestions {
                    active_suggestions.on_tab(true);
                }
            }
            // Tab - cycle suggestions or trigger completion
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                // if the word under the cursor has changed, reset active suggestions
                if let Some(active_suggestions) = &mut self.active_tab_suggestions {
                    if !self
                        .buffer
                        .substring_matches(&active_suggestions.word_under_cursor)
                        || !self
                            .buffer
                            .cursor_in_substring(&active_suggestions.word_under_cursor)
                    {
                        log::debug!("Word under cursor changed, clearing active suggestions");
                        self.active_tab_suggestions = None;
                    }
                }

                if let Some(active_suggestions) = &mut self.active_tab_suggestions {
                    active_suggestions.on_tab(false);
                } else {
                    let res = self.tab_complete();
                    log::debug!("Tab completion result: {:?}", res);
                }
            }
            // Escape - clear suggestions or toggle mouse
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                if self.active_tab_suggestions.is_some() {
                    self.active_tab_suggestions = None;
                } else {
                    self.mouse_state.toggle();
                }
            }
            // Ctrl+C - cancel with comment
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer.move_to_end();
                self.buffer.insert_str(" #[Ctrl+C pressed] ");
                self.mode = AppRunningState::ExitingWithoutCommand;
            }
            // Ctrl+/ (shows as Ctrl+7) - comment out and execute
            KeyEvent {
                code: KeyCode::Char('7'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer.move_to_start();
                self.buffer.insert_str("#");
                self.mode = AppRunningState::ExitingWithCommand(self.buffer.buffer().to_string());
            }
            // Delegate basic text editing to TextBuffer
            _ => {
                self.buffer.on_keypress(key);
            }
        }

        self.on_possible_buffer_change();
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
        self.bash_env.cache_command_type(&first_word);
    }

    fn tab_complete(&mut self) -> Option<()> {
        let buffer: &str = self.buffer.buffer();
        let completion_context =
            tab_completion_context::get_completion_context(buffer, self.buffer.cursor_byte_pos());

        log::debug!("Completion context: {:?}", completion_context);

        let word_under_cursor = completion_context.word_under_cursor;

        match completion_context.comp_type {
            tab_completion_context::CompType::FirstWord => {
                let completions = self.tab_complete_first_word(word_under_cursor);
                self.active_tab_suggestions = ActiveSuggestions::new(
                    Suggestion::from_string_vec(completions, "", " "),
                    word_under_cursor,
                    &self.buffer,
                )
                .try_accept(&mut self.buffer);
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
                        self.active_tab_suggestions = ActiveSuggestions::new(
                            Suggestion::from_string_vec(completions, "", " "),
                            word_under_cursor,
                            &self.buffer,
                        )
                        .try_accept(&mut self.buffer);
                    }
                    Err(e) => {
                        log::debug!(
                            "Bash autocompletion failed for command: {} with error: {}. Falling back to glob expansion.",
                            full_command,
                            e
                        );
                        let completions = self.tab_complete_current_path(word_under_cursor);
                        self.active_tab_suggestions =
                            ActiveSuggestions::new(completions, word_under_cursor, &self.buffer)
                                .try_accept(&mut self.buffer);
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
                self.active_tab_suggestions =
                    ActiveSuggestions::new(completions, word_under_cursor, &self.buffer)
                        .try_accept(&mut self.buffer);
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
                    self.active_tab_suggestions = ActiveSuggestions::new(
                        Suggestion::from_string_vec(vec![completions_as_string], "", " "),
                        word_under_cursor,
                        &self.buffer,
                    )
                    .try_accept(&mut self.buffer);
                }
            }
        }

        Some(())
    }

    fn tab_complete_first_word(&self, command: &str) -> Vec<String> {
        let mut res = Vec::new();

        if command.is_empty() {
            return res;
        }

        res = self.bash_env.get_first_word_completions(&command);

        // TODO: could prioritize based on frequency of use
        res.sort();
        res.sort_by_key(|s| s.len());

        let mut seen = std::collections::HashSet::new();
        res.retain(|s| seen.insert(s.clone()));
        res
    }

    fn tab_complete_current_path(&self, pattern: &str) -> Vec<Suggestion> {
        let glob_results = self.tab_complete_glob_expansion(&(pattern.to_string() + "*"));
        let prefix_to_remove = pattern
            .rsplit_once('/')
            .map(|(p, _)| format!("{}/", p))
            .unwrap_or_default();
        log::debug!(
            "Removing prefix '{}' from glob results for pattern '{}'",
            prefix_to_remove,
            pattern
        );
        glob_results
            .into_iter()
            .map(|mut sug| {
                if let Some(rest) = sug.s.strip_prefix(&prefix_to_remove) {
                    sug.prefix = prefix_to_remove.clone();
                    sug.s = rest.to_string();
                }
                sug
            })
            .collect()
    }

    fn expand_path_pattern(&self, pattern: &str) -> String {
        // TODO expand other variables?
        pattern.replace("~/", &(self.home_path.to_string() + "/"))
    }

    fn tab_complete_glob_expansion(&self, pattern: &str) -> Vec<Suggestion> {
        use glob::glob;
        use std::path::Path;

        log::debug!("Performing glob expansion for pattern: {}", pattern);

        let pattern = &self.expand_path_pattern(pattern);

        // Get the current working directory for relative paths
        let cwd = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(_) => return vec![],
        };

        // Resolve the pattern relative to cwd if it's not absolute
        let full_pattern = if Path::new(pattern).is_absolute() {
            pattern.to_string()
        } else {
            cwd.join(pattern).to_string_lossy().to_string()
        };

        // Use glob to find matching paths
        let mut results = Vec::new();

        const MAX_GLOB_RESULTS: usize = 1_000;

        if let Ok(paths) = glob(&full_pattern) {
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
                    let path_str = if Path::new(pattern).is_absolute() {
                        path.to_string_lossy().to_string()
                    } else {
                        // Strip the cwd prefix to get relative path
                        path.strip_prefix(&cwd)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .to_string()
                    };

                    // Add trailing slash for directories
                    if path.is_dir() {
                        // no trailing space for directories
                        results.push(Suggestion::new(
                            format!("{}/", path_str),
                            "".to_string(),
                            "".to_string(),
                        ));
                    } else {
                        // trailing space for files
                        results.push(Suggestion::new(path_str, "".to_string(), " ".to_string()));
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

    fn create_content(self: &mut Self, width: u16) -> Contents {
        // Basically build the entire frame in a Content first
        // Then figure out how to fit that into the actual frame area
        let mut content = Contents::new(width);

        for (_, is_last, line) in self.prompt_manager.get_ps1_lines().iter().flag_first_last() {
            content.write_line(&line, !is_last);
        }

        self.command_word_cells = vec![];

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
                    bash_funcs::CommandType::Unknown => Style::default().fg(Color::Red),
                    _ => Style::default().fg(Color::Green),
                };
                line_offset = content.cursor_position().0;
                content.write_span(&Span::styled(first_word, first_word_style));
                content.write_span(&Span::styled(rest.to_string(), Style::default()));
            } else {
                content.newline();
                let ps2 = Span::styled(
                    format!("{}∙", line_idx + 1),
                    Style::default()
                        .fg(Color::Indexed(242))
                        .add_modifier(Modifier::DIM),
                );
                content.write_span(&ps2);
                line_offset = content.cursor_position().0;
                content.write_line(&Line::from(line.to_owned()), false);
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
                    Style::new().bg(Color::Rgb(
                        cursor_intensity,
                        cursor_intensity,
                        cursor_intensity,
                    ))
                };

                content.set_edit_cursor_style(animated_vis_row, animated_vis_col, cursor_style);
            }
        }

        if let Some((sug, suf)) = &self.history_suggestion
            && self.mode.is_running()
        {
            let suggestion_style: Style = Style::default().fg(Color::DarkGray);

            suf.lines()
                .collect::<Vec<_>>()
                .iter()
                .flag_first_last()
                .for_each(|(is_first, is_last, line)| {
                    if !is_first {
                        content.newline();
                    }

                    content.write_span(&Span::from(line.to_owned()).style(suggestion_style));

                    if is_last {
                        let mut extra_info_text = format!(" # idx={}", sug.index);
                        if let Some(ts) = sug.timestamp {
                            use timeago;
                            let duration = std::time::Duration::from_secs(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                    .saturating_sub(ts),
                            );
                            let time_ago_str = timeago::Formatter::new().convert(duration);
                            extra_info_text.push_str(&format!(" t={}", time_ago_str));
                        }

                        content.write_span(&Span::from(extra_info_text).style(suggestion_style));
                    }
                });
        }

        if self.should_show_command_info
            && self.mode.is_running()
            && let Some(desc) = command_description
        {
            content.newline();
            content.write_span(&Span::styled(
                format!("# {}", desc),
                Style::default().fg(Color::Blue).italic(),
            ));
        }

        if self.mode.is_running()
            && let Some(tab_suggestions) = &self.active_tab_suggestions
        {
            content.newline();
            for (_, is_last, (suggestion, is_selected)) in tab_suggestions.iter().flag_first_last()
            {
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };

                content.write_span(&Span::styled(suggestion, style));
                if !is_last {
                    content.write_span(&Span::from(" "));
                }
            }
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
                    for (x, cell) in row.iter().enumerate() {
                        if x < frame_area.width as usize {
                            frame.buffer_mut().content
                                [row_idx as usize * frame_area.width as usize + x] = cell.clone();
                        }
                    }
                }
                None => break,
            };
        }
    }
}
