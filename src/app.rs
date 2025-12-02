use std::vec;

use crate::bash_funcs;
use crate::cursor_animation::CursorAnimation;
use crate::events;
use crate::history::{HistoryEntry, HistoryManager, HistorySearchDirection};
use crate::layout_manager::LayoutManager;
use crate::prompt_manager::PromptManager;
use crate::snake_animation::SnakeAnimation;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use ratatui::{
    DefaultTerminal, Frame, TerminalOptions, Viewport,
    text::Line,
    widgets::{Paragraph, Wrap},
};
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
    crossterm::execute!(stdout, crossterm::event::EnableMouseCapture).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::with_options(backend, options).unwrap();
    terminal.hide_cursor().unwrap();

    let runtime = build_runtime();

    let mut app = App::new(ps1_prompt, history, terminal.get_frame().area());
    let command = runtime.block_on(app.run(terminal));

    crossterm::terminal::disable_raw_mode().unwrap();
    crossterm::execute!(stdout, crossterm::event::DisableMouseCapture).unwrap();
    app.layout_manager.finalize();

    log::debug!("Final command: {}", command);
    command
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
        }
    }

    pub async fn run(&mut self, mut terminal: DefaultTerminal) -> String {
        // Update application state here
        let mut events = events::EventHandler::new();
        loop {
            terminal.draw(|f| self.ui(f)).unwrap();
            if !self.is_running {
                break;
            }

            if let Some(event) = events.receiver.recv().await {
                match event {
                    events::Event::Key(event) => {
                        self.onkeypress(event);
                    }
                    events::Event::Mouse(mouse_event) => {
                        self.on_mouse(mouse_event);
                    }
                    events::Event::AnimationTick => {
                        // Toggle cursor visibility for blinking effect
                        self.animation_tick = self.animation_tick.wrapping_add(1);
                    }
                    events::Event::Resize => {}
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

    fn on_mouse(&mut self, mouse: MouseEvent) {
        log::debug!("Mouse event: {:?}", mouse);
        log::debug!(
            " self.last_first_word_cells: {:?}",
            self.last_first_word_cells
        );
        match mouse.kind {
            MouseEventKind::Moved => {
                self.should_show_command_info = false;
                for (cell_row, cell_col) in &self.last_first_word_cells {
                    if *cell_row == mouse.row && *cell_col == mouse.column {
                        log::debug!("Hovering on first word at ({}, {})", cell_row, cell_col);
                        // Additional logic can be added here if needed
                        self.should_show_command_info = true;
                        return;
                    }
                }
            }
            _ => {}
        }
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

        log::debug!("Suggestion updated: {:?}", self.suggestion);
    }

    fn get_command_type(&mut self, cmd: &str) -> (bash_funcs::CommandType, String) {
        if let Some(cached) = self.call_type_cache.get(cmd) {
            return cached.clone();
        }
        let result = bash_funcs::call_type(cmd);
        self.call_type_cache.insert(cmd.to_string(), result.clone());
        log::debug!("call_type result for {}: {:?}", cmd, result);
        result
    }

    /// Concatenates two vectors of Lines by merging their spans at the boundary.
    /// The last line of `a` and the first line of `b` are combined into a single line.
    fn splice_lines<'c>(a: Vec<Line<'c>>, b: Vec<Line<'c>>) -> Vec<Line<'c>> {
        let mut result = a;
        if !result.is_empty() && !b.is_empty() {
            let last_line = result.pop().unwrap();
            let first_line = &b[0];
            let mut combined_spans = last_line.spans;
            // log combined_spans style
            // log::debug!("Splicing lines. First line: {:?}, First line spans: {:?}", first_line, first_line.spans);
            combined_spans.extend(first_line.spans.clone());
            result.push(Line::from(combined_spans));
            result.extend(b.into_iter().skip(1));
        } else {
            result.extend(b);
        }
        result
    }

    fn ui(&mut self, f: &mut Frame) {
        let mut output_lines: Vec<Line> = self.prompt_manager.get_ps1_lines();

        self.cursor_animation.update_position(self.buffer.cursor());
        let (cursor_row, cursor_col) = self.cursor_animation.get_position();
        let cursor_intensity = self.cursor_animation.get_intensity();

        self.last_first_word_cells = vec![];

        // TODO: cache this
        let suggestion_suffix_lines: Vec<Line> =
            self.suggestion.as_ref().map_or(vec![], |(sug, suf)| {
                suf.lines()
                    .enumerate()
                    .map(|(i, line)| {
                        let mut line_parts = vec![];

                        line_parts.push(
                            Span::from(line.to_owned())
                                .style(Style::default().fg(Color::DarkGray))
                                .into(),
                        );

                        if i == suf.lines().count() - 1 {
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

                            line_parts.push(
                                Span::from(extra_info_text)
                                    .style(Style::default().fg(Color::DarkGray))
                                    .into(),
                            );
                        }

                        Line::from(line_parts)
                    })
                    .collect()
            });

        // Clone lines to break the borrow so we can call get_command_type
        let command_lines_str: Vec<String> = self.buffer.lines().to_vec();

        let mut command_description: Option<String> = None;

        let mut command_lines: Vec<Line> = command_lines_str
            .iter()
            .enumerate()
            .map(|(i, line)| {
                if i == 0 {
                    let space_pos = line.find(' ').unwrap_or(line.len());
                    let (first_word, rest) = line.split_at(space_pos);

                    let (command_type, short_desc) = self.get_command_type(first_word);
                    if self.should_show_command_info && !short_desc.is_empty() {
                        command_description = Some(short_desc);
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

                    let mut combined_spans = Vec::new();
                    combined_spans.push(Span::styled(first_word, first_word_style));
                    combined_spans.push(Span::styled(rest.to_string(), Style::default()));
                    Line::from(combined_spans)
                } else {
                    Line::from(line.as_str())
                }
            })
            .collect();

        if self.is_running {
            command_lines = Self::splice_lines(command_lines, suggestion_suffix_lines);
        }

        // Add cursor
        for (i, line) in command_lines.iter_mut().enumerate() {
            if i == cursor_row && self.is_running {
                let cursor_style = ratatui::style::Style::new().bg(ratatui::style::Color::Rgb(
                    cursor_intensity,
                    cursor_intensity,
                    cursor_intensity,
                ));

                // Split the line at cursor position and apply cursor style
                let mut styled_spans = Vec::new();
                let mut current_col = 0;

                for span in line.spans.clone() {
                    let span_text = &span.content;
                    let span_len = span_text.chars().count();

                    if current_col + span_len <= cursor_col {
                        // Cursor is after this span
                        styled_spans.push(span);
                        current_col += span_len;
                    } else if current_col >= cursor_col + 1 {
                        // Cursor is before this span
                        styled_spans.push(span);
                    } else {
                        // Cursor is within this span
                        let chars: Vec<char> = span_text.chars().collect();
                        let cursor_pos_in_span = cursor_col - current_col;

                        // Text before cursor
                        if cursor_pos_in_span > 0 {
                            let before: String = chars[..cursor_pos_in_span].iter().collect();
                            styled_spans.push(Span::styled(before, span.style));
                        }

                        // Character at cursor position
                        if cursor_pos_in_span < chars.len() {
                            let cursor_char = chars[cursor_pos_in_span].to_string();
                            styled_spans
                                .push(Span::styled(cursor_char, span.style.patch(cursor_style)));
                        } else {
                            // Cursor at end of line - add a space with cursor style
                            styled_spans.push(Span::styled(" ", cursor_style));
                        }

                        // Text after cursor
                        if cursor_pos_in_span + 1 < chars.len() {
                            let after: String = chars[cursor_pos_in_span + 1..].iter().collect();
                            styled_spans.push(Span::styled(after, span.style));
                        }

                        current_col += span_len;
                    }
                }

                // If cursor is at the very end of the line, add a space with cursor style
                if cursor_col >= current_col {
                    styled_spans.push(Span::styled(" ", cursor_style));
                }

                *line = Line::from(styled_spans);
            }
        }

        // Combine with prompt
        if let Some(last_ps1_lines) = output_lines.last() {
            let last_ps1_line_len = last_ps1_lines.width() as u16;
            let row_offset = output_lines.len().saturating_sub(1) as u16;
            // log::debug!("last_ps1_line_len: {}, row_offset: {}", last_ps1_line_len, row_offset);
            self.last_first_word_cells
                .iter_mut()
                .for_each(|(row, col)| {
                    *col += last_ps1_line_len;
                    *row += row_offset;
                });
        }
        output_lines = Self::splice_lines(output_lines, command_lines);
        // log::debug!("command_description: {:?}", command_description);
        if let Some(desc) = command_description {
            output_lines.push(Line::from(Span::styled(
                format!(" # {}", desc),
                Style::default().fg(Color::Red),
            )));
        }

        let output = Paragraph::new(output_lines).wrap(Wrap { trim: false });
        let full_terminal_area = f.area();
        let output_num_lines = output.line_count(full_terminal_area.width) as u16;

        self.layout_manager.update_area(full_terminal_area);
        let area = self.layout_manager.get_area(output_num_lines);

        f.render_widget(&output, area);

        // log::debug!("area.top(): {} area.left(): {}", area.top(), area.left());

        // TODO: this might be split across multiple lines. the current tracking logic makes some assumptions
        self.last_first_word_cells
            .iter_mut()
            .for_each(|(row, col)| {
                *row += area.top();
                *col += area.left();
            });
    }
}
