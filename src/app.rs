use crate::active_suggestions;
use crate::bash_funcs;
use crate::command_acceptance;
use crate::cursor_animation::CursorAnimation;
use crate::events;
use crate::frame_builder::FrameBuilder;
use crate::history::{HistoryEntry, HistoryManager, HistorySearchDirection};
use crate::iter_first_last::FirstLast;
use crate::layout_manager::LayoutManager;
use crate::prompt_manager::PromptManager;
use crate::snake_animation::SnakeAnimation;
use crate::tab_completion;
use crate::text_buffer::TextBuffer;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport, text::Line};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::vec;

fn build_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

pub fn get_command(ps1_prompt: String, history: &mut HistoryManager) -> String {
    let options = TerminalOptions {
        // TODO: consider restricting viewport
        viewport: Viewport::Fullscreen,
    };
    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::with_options(backend, options).unwrap();
    terminal.hide_cursor().unwrap();

    let runtime = build_runtime();

    let mut app = App::new(ps1_prompt, history, terminal.get_frame().area());
    let command = runtime.block_on(app.run(terminal));

    crossterm::terminal::disable_raw_mode().unwrap();
    app.mouse_state.disable();
    app.layout_manager.finalize();

    log::debug!("Final command: {}", command);
    command
}

struct MouseState {
    is_enabled: bool,
    time_of_last_enable_attempt: std::time::Instant,
    time_of_last_move: std::time::Instant,
}

impl MouseState {
    fn new() -> Self {
        let mut mouse_state = MouseState {
            is_enabled: false,
            time_of_last_enable_attempt: std::time::Instant::now(),
            time_of_last_move: std::time::Instant::now(),
        };
        mouse_state.enable();
        mouse_state
    }

    fn update_on_move(&mut self) -> bool {
        if self.time_of_last_move.elapsed().as_millis() < 50 {
            return false;
        }
        self.time_of_last_move = std::time::Instant::now();
        true
    }

    fn enable(&mut self) {
        if !self.is_enabled {
            let mut stdout = std::io::stdout();
            crossterm::execute!(stdout, crossterm::event::EnableMouseCapture).unwrap();
            self.is_enabled = true;
            self.time_of_last_enable_attempt = std::time::Instant::now();
        }
    }

    fn disable(&mut self) {
        if self.is_enabled {
            let mut stdout = std::io::stdout();
            crossterm::execute!(stdout, crossterm::event::DisableMouseCapture).unwrap();
            self.is_enabled = false;
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum AppRunningState {
    Running,
    ExitingWithCommand,
    ExitingWithoutCommand,
}

impl AppRunningState {
    pub fn is_running(&self) -> bool {
        *self == AppRunningState::Running
    }
}

struct App<'a> {
    mode: AppRunningState,
    buffer: TextBuffer,
    animation_tick: u64,
    cursor_animation: CursorAnimation,
    prompt_manager: PromptManager,
    /// Parsed bash history available at startup.
    history_manager: &'a mut HistoryManager,
    call_type_cache: std::collections::HashMap<String, (bash_funcs::CommandType, String)>,
    layout_manager: LayoutManager,
    snake_animation: SnakeAnimation,
    history_suggestion: Option<(HistoryEntry, String)>,
    command_word_cells: Vec<(u16, u16)>,
    should_show_command_info: bool,
    mouse_state: MouseState,
    defined_aliases: Vec<String>,
    defined_reserved_words: Vec<String>,
    defined_shell_functions: Vec<String>,
    defined_builtins: Vec<String>,
    defined_executables: Vec<(PathBuf, String)>,
    active_tab_suggestions: Option<active_suggestions::ActiveSuggestions>,
}

impl<'a> App<'a> {
    fn new(ps1: String, history: &'a mut HistoryManager, terminal_area: Rect) -> Self {
        // TODO: fetch these in background
        const PATH_VAR: &str = "PATH";
        let path_var = bash_builtins::variables::find_as_string(PATH_VAR);
        let executables = if let Some(path_str) = path_var.as_ref().and_then(|v| v.to_str().ok()) {
            App::get_executables_from_path(path_str)
        } else {
            Vec::new()
        };

        history.new_session();
        App {
            mode: AppRunningState::Running,
            buffer: TextBuffer::new(""),
            animation_tick: 0,
            cursor_animation: CursorAnimation::new(),
            prompt_manager: PromptManager::new(ps1),
            history_manager: history,
            call_type_cache: std::collections::HashMap::new(),
            layout_manager: LayoutManager::new(terminal_area),
            snake_animation: SnakeAnimation::new(),
            history_suggestion: None,
            command_word_cells: Vec::new(),
            should_show_command_info: false,
            mouse_state: MouseState::new(),
            // TODO: fetch these in background thread
            defined_aliases: bash_funcs::get_all_aliases(),
            defined_reserved_words: bash_funcs::get_all_reserved_words(),
            defined_shell_functions: bash_funcs::get_all_shell_functions(),
            defined_builtins: bash_funcs::get_all_shell_builtins(),
            defined_executables: executables,
            active_tab_suggestions: None,
        }
    }

    pub async fn run(&mut self, mut terminal: DefaultTerminal) -> String {
        // Update application state here
        let mut events = events::EventHandler::new();
        let mut redraw = true;
        loop {
            if redraw {
                terminal.draw(|f| self.ui(f)).unwrap();
            }
            if self.mode != AppRunningState::Running {
                break;
            }

            if let Some(event) = events.receiver.recv().await {
                redraw = match event {
                    events::Event::Key(event) => {
                        // The user has stopped scrolling and wants to use the app
                        self.mouse_state.enable();

                        self.onkeypress(event);
                        true
                    }
                    events::Event::Mouse(mouse_event) => self.on_mouse(mouse_event),
                    events::Event::AnimationTick => {
                        // Toggle cursor visibility for blinking effect
                        self.animation_tick = self.animation_tick.wrapping_add(1);
                        true
                    }
                    events::Event::ReenableMouseAttempt => {
                        self.mouse_state.enable();
                        false
                    }
                    events::Event::Resize(new_cols, new_rows) => {
                        log::debug!("Terminal resized to {}x{}", new_cols, new_rows);
                        true
                    }
                }
            }
        }

        if self.mode == AppRunningState::ExitingWithCommand {
            self.buffer.buffer().to_owned()
        } else {
            String::new()
        }
    }

    fn get_executables_from_path(path: &str) -> Vec<(PathBuf, String)> {
        let mut executables = Vec::new();
        for dir in path.split(':') {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && path
                            .metadata()
                            .map(|m| m.permissions().mode() & 0o111 != 0)
                            .unwrap_or(false)
                    {
                        if let Some(file_name) = path
                            .file_name()
                            .and_then(|n| n.to_str().map(|s| s.to_string()))
                        {
                            executables.push((path, file_name));
                        }
                    }
                }
            }
        }
        executables
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> bool {
        // log::debug!("Mouse event: {:?}", mouse);
        match mouse.kind {
            MouseEventKind::Moved => {
                if !self.mouse_state.update_on_move() {
                    log::debug!("Mouse move ignored due to rapid movement");
                    return false;
                }
                self.should_show_command_info = false;
                for (cell_row, cell_col) in &self.command_word_cells {
                    if *cell_row == mouse.row && *cell_col == mouse.column {
                        log::debug!("Hovering on first word at ({}, {})", cell_row, cell_col);
                        // Additional logic can be added here if needed
                        self.should_show_command_info = true;
                    }
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                self.mouse_state.disable();
            }
            _ => {}
        };
        true
    }

    fn onkeypress(&mut self, key: KeyEvent) {
        log::debug!("Key pressed: {:?}", key);
        match key {
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.buffer.delete_backwards();
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                // control backspace show up as these ones for me
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('w'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer.delete_one_word_left();
            }
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::ALT,
                ..
            } => {
                self.buffer.delete_one_word_right();
            }
            KeyEvent {
                code: KeyCode::Delete,
                ..
            } => {
                self.buffer.delete_forwards();
            }
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.buffer.move_one_word_left();
                } else {
                    self.buffer.move_left();
                };
            }
            KeyEvent {
                code: KeyCode::Right | KeyCode::End,
                ..
            } => {
                if self.buffer.is_cursor_at_end()
                    && let Some((_, suf)) = &self.history_suggestion
                {
                    self.buffer.insert_str(suf);
                    self.buffer.move_to_end();
                } else {
                    match key {
                        KeyEvent {
                            code: KeyCode::Right,
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => self.buffer.move_one_word_right(),
                        KeyEvent {
                            code: KeyCode::End, ..
                        } => self.buffer.move_end_of_line(),
                        _ => self.buffer.move_right(),
                    };
                }
            }
            KeyEvent {
                code: KeyCode::Home,
                ..
            } => {
                self.buffer.move_start_of_line();
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                let cursor_row = self.buffer.cursor_row();
                if cursor_row == 0 {
                    // Replace current buffer with last history entry
                    if let Some(entry) = self
                        .history_manager
                        .search_in_history(self.buffer.buffer(), HistorySearchDirection::Backward)
                    {
                        let new_command = entry.command.clone();
                        self.buffer = TextBuffer::new(new_command.as_str());
                    }
                } else {
                    // log::debug!("cursor starting in     {:?}", self.buffer.cursor_2d_position());
                    self.buffer.move_line_up();
                    // log::debug!("Moved cursor up to row {:?}", self.buffer.cursor_row());
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                if self.buffer.is_cursor_on_final_line() {
                    // Replace current buffer with next history entry
                    if let Some(entry) = self
                        .history_manager
                        .search_in_history(self.buffer.buffer(), HistorySearchDirection::Forward)
                    {
                        let new_command = entry.command.clone();
                        self.buffer = TextBuffer::new(new_command.as_str());
                    }
                } else {
                    self.buffer.move_line_down();
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                if let Some(active_suggestions) = &mut self.active_tab_suggestions {
                    let (selected_command, word_under_cursor) = active_suggestions.on_enter();
                    self.active_tab_suggestions = None;
                    let res = self
                        .buffer
                        .replace_word_under_cursor(&selected_command, &word_under_cursor);
                    match res {
                        Ok(_) => {
                            self.buffer.insert_char(' ');
                        }
                        Err(e) => {
                            log::error!("Error during tab completion accepting: {}", e)
                        }
                    }
                } else {
                    if self.buffer.is_cursor_at_end()
                        && command_acceptance::will_bash_accept_buffer(&self.buffer.buffer())
                    {
                        self.mode = AppRunningState::ExitingWithCommand;
                    } else {
                        self.buffer.insert_newline();
                    }
                }
            }
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
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer.move_to_end();
                if !self.buffer.last_line_is_empty() {
                    self.buffer.insert_newline();
                }
                self.buffer.insert_str("#[Ctrl+C pressed] ");
                self.mode = AppRunningState::ExitingWithoutCommand;
            }
            KeyEvent {
                // Ctrl+/ comes up as this for me
                code: KeyCode::Char('7'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer.move_to_start();
                self.buffer.insert_str("#");
                self.mode = AppRunningState::ExitingWithCommand;
            }
            KeyEvent {
                code: KeyCode::Char(c),
                ..
            } => {
                self.buffer.insert_char(c);
            }
            _ => {}
        }

        self.history_suggestion = self
            .history_manager
            .get_command_suggestion_suffix(self.buffer.buffer());

        let first_word = {
            let line = self.buffer.buffer();
            let space_pos = line.find(' ').unwrap_or(line.len());
            &line[0..space_pos]
        }
        .to_owned();
        self.cache_command_type(&first_word);
    }

    fn tab_complete(&mut self) -> Option<()> {
        let buffer: &str = self.buffer.buffer();
        let completion_context =
            tab_completion::get_completion_context(buffer, self.buffer.cursor_char_pos());

        log::debug!("Completion context: {:?}", completion_context);

        match completion_context.comp_type {
            tab_completion::CompType::FirstWord(word_under_cursor) => {
                let completions = self.tab_complete_first_word(&word_under_cursor.s);
                match completions.as_slice() {
                    [completion] => {
                        let res = self
                            .buffer
                            .replace_word_under_cursor(&completion, &word_under_cursor);
                        match res {
                            Ok(_) => self.buffer.insert_char(' '),
                            Err(e) => {
                                log::error!("Error during tab completion: {}", e)
                            }
                        }
                    }
                    [] => {
                        log::debug!(
                            "No completions found for first word: {}",
                            word_under_cursor.s
                        );
                    }
                    _ => {
                        log::debug!("Multiple completions available: {:?}", completions);
                        // TODO: show active suggestions UI for the user to pick from
                        self.active_tab_suggestions =
                            Some(active_suggestions::ActiveSuggestions::new(
                                completions,
                                word_under_cursor,
                            ));
                    }
                }
            }
            tab_completion::CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
                word_under_cursor_byte_end,
            } => {
                let res = bash_funcs::run_autocomplete_compspec(
                    &full_command,
                    &command_word,
                    &word_under_cursor.s,
                    cursor_byte_pos,
                    word_under_cursor_byte_end,
                );
                log::debug!("Bash autocomplete results: {:?}", res);
                if let Some(completion) = res.first() {
                    let res = self
                        .buffer
                        .replace_word_under_cursor(&completion, &word_under_cursor);
                    match res {
                        Ok(_) => self.buffer.insert_char(' '),
                        Err(e) => {
                            log::error!("Error during tab completion: {}", e)
                        }
                    }
                }
            }
            tab_completion::CompType::CursorOnBlank => {
                log::debug!("Cursor is on blank space, no tab completion performed");
            }
        }

        Some(())
    }

    fn tab_complete_first_word(&self, command: &str) -> Vec<String> {
        let mut res = Vec::new();

        if command.is_empty() {
            return res;
        }

        for poss_completion in self
            .defined_aliases
            .iter()
            .chain(self.defined_reserved_words.iter())
            .chain(self.defined_shell_functions.iter())
            .chain(self.defined_builtins.iter())
            .chain(self.defined_executables.iter().map(|(_, name)| name))
        {
            if poss_completion.starts_with(&command) {
                res.push(poss_completion.to_string());
            }
        }

        // TODO: could prioritize based on frequency of use
        res.sort();
        res.sort_by_key(|s| s.len());

        let mut seen = std::collections::HashSet::new();
        res.retain(|s| seen.insert(s.clone()));
        res
    }

    fn get_command_type(&self, cmd: &str) -> (bash_funcs::CommandType, String) {
        self.call_type_cache
            .get(cmd)
            .unwrap_or(&(bash_funcs::CommandType::Unknown, String::new()))
            .clone()
    }

    fn cache_command_type(&mut self, cmd: &str) -> (bash_funcs::CommandType, String) {
        if let Some(cached) = self.call_type_cache.get(cmd) {
            return cached.clone();
        }
        let result = bash_funcs::call_type(cmd);
        self.call_type_cache.insert(cmd.to_string(), result.clone());
        log::debug!("call_type result for {}: {:?}", cmd, result);
        result
    }

    fn ui(&mut self, f: &mut Frame) {
        // Basically build the entire frame in a FrameBuilder first
        // Then figure out how to fit that into the actual frame area
        let mut fb = FrameBuilder::new(f.area().width);

        for line in self.prompt_manager.get_ps1_lines() {
            fb.write_line(&line, false);
        }

        let (ps1_cursor_col, ps1_cursor_row) = fb.cursor_position();

        self.command_word_cells = vec![];

        let mut command_description: Option<String> = None;

        for (is_first, _, line) in self.buffer.lines().iter().flag_first_last() {
            if is_first {
                let space_pos = line.find(' ').unwrap_or(line.len());
                let (first_word, rest) = line.split_at(space_pos);

                let (command_type, short_desc) = self.get_command_type(first_word);
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

                fb.write_span(&Span::styled(first_word, first_word_style));
                fb.write_span(&Span::styled(rest.to_string(), Style::default()));
            } else {
                fb.newline();
                fb.write_line(&Line::from(line.to_owned()), false);
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
                        fb.newline();
                    }

                    fb.write_span(&Span::from(line.to_owned()).style(suggestion_style));

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

                        fb.write_span(&Span::from(extra_info_text).style(suggestion_style));
                    }
                });
        }

        if self.should_show_command_info
            && self.mode.is_running()
            && let Some(desc) = command_description
        {
            fb.newline();
            fb.write_span(&Span::styled(
                format!("# {}", desc),
                Style::default().fg(Color::Blue).italic(),
            ));
        }

        if self.mode.is_running()
            && let Some(tab_suggestions) = &self.active_tab_suggestions
        {
            fb.newline();
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

                fb.write_span(&Span::styled(suggestion, style));
                if !is_last {
                    fb.write_span(&Span::from(" "));
                }
            }
        }

        // Draw cursor
        if self.mode.is_running() {
            self.cursor_animation
                .update_position(self.buffer.cursor_2d_position());
            let (cursor_row, cursor_col) = self.cursor_animation.get_position();
            let cursor_intensity = self.cursor_animation.get_intensity();

            let cursor_style = ratatui::style::Style::new().bg(ratatui::style::Color::Rgb(
                cursor_intensity,
                cursor_intensity,
                cursor_intensity,
            ));

            let final_cursor_row = ps1_cursor_row + cursor_row;
            let final_cursor_col = if cursor_row == 0 {
                ps1_cursor_col + cursor_col
            } else {
                cursor_col
            };

            let wrapped_cursor_row = final_cursor_row + (final_cursor_col / fb.width) as u16;
            let wrapped_cursor_col = final_cursor_col % fb.width;

            fb.set_style(
                Rect::new(wrapped_cursor_col, wrapped_cursor_row, 1, 1),
                cursor_style,
            );
        }

        // what should happen
        // framebuilder should be of the correct width but indefinitely tall
        // after we write everything to it, we figure out how many rows it used
        // then self.layout_manager tries and puts the framebuilder contents
        // into the frame area appropriately. It might need to scroll some rows off the top
        // of the screen to make framebuilder fit.
        // if framebuilder is longer than the display area, we try and center the cursor.
        // care needs to be taken to ensure that everything from the start of framebuilder
        // down the bottom of the screen is cleared.

        // Draw the buffer

        self.layout_manager.fit_frame_builder_to_frame(&mut fb, f);
    }
}
