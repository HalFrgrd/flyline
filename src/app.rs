use crate::bash_funcs;
use crate::cursor_animation::CursorAnimation;
use crate::events;
use crate::frame_builder::FrameBuilder;
use crate::history::{HistoryEntry, HistoryManager, HistorySearchDirection};
use crate::iter_first_last::FirstLast;
use crate::layout_manager::LayoutManager;
use crate::prompt_manager::PromptManager;
use crate::snake_animation::SnakeAnimation;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use ratatui::{DefaultTerminal, Frame, TerminalOptions, Viewport, text::Line};
use std::vec;
use tui_textarea::{CursorMove, TextArea};

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

struct App<'a> {
    is_running: bool,
    buffer: TextArea<'a>,
    animation_tick: u64,
    cursor_animation: CursorAnimation,
    prompt_manager: PromptManager,
    /// Parsed bash history available at startup.
    history_manager: &'a mut HistoryManager,
    is_multiline_mode: bool,
    call_type_cache: std::collections::HashMap<String, (bash_funcs::CommandType, String)>,
    layout_manager: LayoutManager,
    snake_animation: SnakeAnimation,
    suggestion: Option<(HistoryEntry, String)>,
    last_first_word_cells: Vec<(u16, u16)>,
    should_show_command_info: bool,
    mouse_state: MouseState,
}

impl<'a> App<'a> {
    fn new(ps1: String, history: &'a mut HistoryManager, terminal_area: Rect) -> Self {
        history.new_session();
        App {
            is_running: true,
            buffer: TextArea::default(),
            animation_tick: 0,
            cursor_animation: CursorAnimation::new(),
            prompt_manager: PromptManager::new(ps1),
            history_manager: history,
            is_multiline_mode: false,
            call_type_cache: std::collections::HashMap::new(),
            layout_manager: LayoutManager::new(terminal_area),
            snake_animation: SnakeAnimation::new(),
            suggestion: None,
            last_first_word_cells: Vec::new(),
            should_show_command_info: false,
            mouse_state: MouseState::new(),
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
            if !self.is_running {
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
                    events::Event::Resize => true,
                }
            }
        }

        self.buffer.lines().join("\n")
    }

    fn unbalanced_quotes(&self) -> bool {
        let mut single_quotes = 0;
        let mut double_quotes = 0;
        for line in self.buffer.lines() {
            for c in line.chars() {
                match c {
                    '\'' => single_quotes += 1,
                    '"' => double_quotes += 1,
                    _ => {}
                }
            }
        }
        single_quotes % 2 != 0 || double_quotes % 2 != 0
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> bool {
        log::debug!("Mouse event: {:?}", mouse);
        match mouse.kind {
            MouseEventKind::Moved => {
                if !self.mouse_state.update_on_move() {
                    log::debug!("Mouse move ignored due to rapid movement");
                    return false;
                }
                self.should_show_command_info = false;
                for (cell_row, cell_col) in &self.last_first_word_cells {
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
        // log::debug!("Key pressed: {:?}", key);
        match key {
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.buffer.delete_char();
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
                self.buffer.delete_word();
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
                self.buffer.delete_next_word();
            }
            KeyEvent {
                code: KeyCode::Delete,
                ..
            } => {
                // self.buffer.move_cursor(CursorMove::Forward);
                self.buffer.delete_next_char();
            }
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => {
                let move_type = if key.modifiers.contains(KeyModifiers::CONTROL) {
                    CursorMove::WordBack
                } else {
                    CursorMove::Back
                };
                self.buffer.move_cursor(move_type);
            }
            KeyEvent {
                code: KeyCode::Right | KeyCode::End,
                ..
            } => {
                let current_cursor_pos = self.buffer.cursor();
                self.buffer.move_cursor(CursorMove::Bottom);
                self.buffer.move_cursor(CursorMove::End);
                let end_cursor_pos = self.buffer.cursor();

                if current_cursor_pos == end_cursor_pos
                    && let Some((_, suf)) = &self.suggestion
                {
                    self.buffer.insert_str(suf);
                    self.buffer.move_cursor(CursorMove::Bottom);
                    self.buffer.move_cursor(CursorMove::End);
                } else {
                    let restore_cursor_pos: (u16, u16) = (
                        current_cursor_pos.0.try_into().unwrap_or(0),
                        current_cursor_pos.1.try_into().unwrap_or(0),
                    );
                    self.buffer
                        .move_cursor(CursorMove::Jump(restore_cursor_pos.0, restore_cursor_pos.1));
                    let move_type = match key {
                        KeyEvent {
                            code: KeyCode::Right,
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => CursorMove::WordForward,
                        KeyEvent {
                            code: KeyCode::End, ..
                        } => CursorMove::End,
                        _ => CursorMove::Forward,
                    };
                    self.buffer.move_cursor(move_type);
                }
            }
            KeyEvent {
                code: KeyCode::Home,
                ..
            } => {
                self.buffer.move_cursor(CursorMove::Head);
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                let (cursor_row, _) = self.buffer.cursor();
                if cursor_row == 0 {
                    // Replace current buffer with last history entry
                    if let Some(entry) = self.history_manager.search_in_history(
                        self.buffer.lines().join("\n").as_str(),
                        HistorySearchDirection::Backward,
                    ) {
                        let new_command = entry.command.clone();
                        self.buffer = TextArea::from(vec![new_command.as_str()]);
                        self.buffer.move_cursor(CursorMove::End);
                    }
                } else {
                    self.buffer.move_cursor(CursorMove::Up);
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                let (cursor_row, _) = self.buffer.cursor();
                if cursor_row + 1 >= self.buffer.lines().len() {
                    // Replace current buffer with next history entry
                    if let Some(entry) = self.history_manager.search_in_history(
                        self.buffer.lines().join("\n").as_str(),
                        HistorySearchDirection::Forward,
                    ) {
                        let new_command = entry.command.clone();
                        self.buffer = TextArea::from(vec![new_command.as_str()]);
                        self.buffer.move_cursor(CursorMove::End);
                    }
                } else {
                    self.buffer.move_cursor(CursorMove::Down);
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                if self.is_multiline_mode {
                    self.buffer.insert_newline();
                } else {
                    if self.unbalanced_quotes() {
                        self.is_multiline_mode = true;
                        self.buffer.insert_newline();
                        // self.increase_num_rows_below_prompt();
                    } else {
                        self.is_running = false;
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                // let resp = self.client.get_request(BashReq::Complete, self.buffer.lines().join("\n").as_str());
                // log::debug!("Completion response: {:?}", resp);
                // self.buffer.insert_str(&resp);
            }
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer = TextArea::from(vec!["#Ctrl+C pressed"]);
                self.is_running = false;
            }
            KeyEvent {
                code: KeyCode::Char(c),
                ..
            } => {
                self.buffer.insert_char(c);
            }
            _ => {}
        }

        self.suggestion = self
            .history_manager
            .get_command_suggestion_suffix(self.buffer.lines().join("\n").as_str());

        let first_word = self
            .buffer
            .lines()
            .get(0)
            .and_then(|line| {
                let space_pos = line.find(' ').unwrap_or(line.len());
                Some(&line[0..space_pos])
            })
            .unwrap_or("")
            .to_owned();
        self.cache_command_type(&first_word);
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
        let mut fb = FrameBuilder::new(f.area());

        for line in self.prompt_manager.get_ps1_lines() {
            fb.write_line(&line, false);
        }

        let (ps1_cursor_col, ps1_cursor_row) = fb.cursor_position();

        self.last_first_word_cells = vec![];

        let mut command_description: Option<String> = None;

        for (is_first, _, line) in self.buffer.lines().iter().flag_first_last() {
            if is_first {
                let space_pos = line.find(' ').unwrap_or(line.len());
                let (first_word, rest) = line.split_at(space_pos);

                let (command_type, short_desc) = self.get_command_type(first_word);
                if self.should_show_command_info && !short_desc.is_empty() {
                    command_description = Some(short_desc.to_owned());
                }

                let first_word = if first_word.starts_with("python") && self.is_running {
                    self.snake_animation.update_anim();
                    let snake_string = self.snake_animation.to_string();

                    let mut result = String::new();
                    let first_word_chars: Vec<char> = first_word.chars().collect();
                    let snake_chars: Vec<char> = snake_string.chars().collect();

                    for i in 0..6.min(first_word_chars.len()) {
                        if i < snake_chars.len() {
                            if snake_chars[i] == '⠀' {
                                result.push(first_word_chars[i]);
                            } else {
                                result.push(snake_chars[i]);
                            }
                        } else {
                            result.push(first_word_chars[i]);
                        }
                    }

                    // Add remaining characters from first_word if it's longer than 6
                    if first_word_chars.len() > 6 {
                        result.push_str(&first_word_chars[6..].iter().collect::<String>());
                    }
                    result
                } else {
                    first_word.to_string()
                };

                let is_first_word_recognized = command_type != bash_funcs::CommandType::Unknown;

                let first_word_style = Style::default().fg(if is_first_word_recognized {
                    Color::Green
                } else {
                    Color::Red
                });

                for (col_offset, _ch) in first_word.chars().enumerate() {
                    self.last_first_word_cells.push((0, col_offset as u16));
                }

                fb.write_span(&Span::styled(first_word, first_word_style));
                fb.write_span(&Span::styled(rest.to_string(), Style::default()));
            } else {
                fb.newline();
                fb.write_line(&Line::from(line.as_str()), false);
            }
        }

        if let Some((sug, suf)) = &self.suggestion {
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

        // Draw cursor
        if self.is_running {
            self.cursor_animation.update_position(self.buffer.cursor());
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

            fb.buffer_mut().set_style(
                Rect::new(
                    final_cursor_col.try_into().unwrap_or(0),
                    final_cursor_row.try_into().unwrap_or(0),
                    1,
                    1,
                ),
                cursor_style,
            );
        }

        // Draw the buffer
        let (_, max_buf_row) = fb.cursor_position();

        let full_terminal_area = f.area();

        self.layout_manager.update_area(full_terminal_area);
        let drawing_area = self
            .layout_manager
            .get_area(max_buf_row.try_into().unwrap_or(0) + 1);

        for y in 0..drawing_area.height {
            for x in 0..drawing_area.width {
                let buf_x = x as usize;
                let buf_y = y as usize;
                if buf_x < fb.buffer().area().width as usize
                    && buf_y < fb.buffer().area().height as usize
                {
                    let cell = fb.buffer().cell((buf_x as u16, buf_y as u16)).unwrap();
                    let term_x: usize = (drawing_area.x + x) as usize;
                    let term_y: usize = (drawing_area.y + y) as usize;
                    let term_idx: usize = term_y * (full_terminal_area.width as usize) + term_x;
                    f.buffer_mut().content[term_idx] = cell.clone();
                }
            }
        }
    }
}
