use std::vec;

use crate::bash_funcs;
use crate::cursor_animation::CursorAnimation;
use crate::events;
use ansi_to_tui::IntoText;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use ratatui::prelude::*;
use ratatui::{
    DefaultTerminal, Frame, TerminalOptions, Viewport,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use std::fs;
use tui_textarea::{CursorMove, TextArea};

/// Read the user's bash history file into a Vec<String>.
/// Tries $HISTFILE first, otherwise falls back to $HOME/.bash_history.
fn parse_bash_history() -> Vec<String> {
    let hist_path = std::env::var("HISTFILE").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/.bash_history", home)
    });

    match fs::read_to_string(&hist_path) {
        Ok(s) => s.lines().map(|l| l.to_string()).collect(),
        Err(e) => {
            log::warn!("Could not read history file '{}': {}", hist_path, e);
            Vec::new()
        }
    }
}

fn build_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

pub fn get_command(ps1_prompt: String) -> String {
    let options = TerminalOptions {
        // TODO: consider restricting viewport
        viewport: Viewport::Fullscreen,
    };
    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::with_options(backend, options).unwrap();
    terminal.hide_cursor().unwrap();

    // log::debug!("terminal = {:?}", terminal);

    // Strip literal "\[" and "\]" markers from PS1 (they wrap non-printing sequences)
    let ps1_prompt = ps1_prompt.replace("\\[", "").replace("\\]", "");
    let ps1_prompt: Text = ps1_prompt.into_text().unwrap_or("bad ps1>".into());

    // Parse the user's bash history into a vector of command strings.
    let history = parse_bash_history();

    let runtime = build_runtime();

    let mut app = App::new(ps1_prompt, history, terminal.get_frame().area());
    let command = runtime.block_on(app.run(terminal));

    crossterm::terminal::disable_raw_mode().unwrap();
    app.layout_manager.finalize();

    let final_cursor_pos = crossterm::cursor::position().unwrap();
    log::debug!("Final cursor position: {:?}", final_cursor_pos);
    // terminal.show_cursor().unwrap();

    log::debug!("Final command: {}", command);
    command
}

#[derive(Debug)]
struct LayoutManager {
    terminal_height: u16,
    terminal_width: u16,
    range_start: u16,
    range_end: u16,
}

impl LayoutManager {

    fn new(terminal_area: Rect) -> Self {

        let starting_cursor_position = crossterm::cursor::position().unwrap();

        let layout_manager = LayoutManager {
            terminal_height: terminal_area.height,
            terminal_width: terminal_area.width,
            range_start: starting_cursor_position.1,
            range_end: terminal_area.height,
        };
        layout_manager
    }



    fn get_area(&mut self, output_num_lines: u16) -> Rect {

        let desired_area = Rect::new(0, self.range_start, self.terminal_width, output_num_lines);

        if desired_area.bottom() > self.terminal_height {
            let lines_to_scroll = desired_area.bottom().saturating_sub(self.terminal_height);
            log::debug!(
                "Desired area {:?} exceeds terminal height {}, scrolling by {}",
                desired_area,
                self.terminal_height,
                lines_to_scroll,
            );
            self.scroll_by(lines_to_scroll);
        }

        // TODO: check we are in bounds
        let area = Rect::new(0, self.range_start, self.terminal_width, output_num_lines);
        self.range_end = area.bottom();

        area
    }

    fn scroll_by(&mut self, lines_to_scroll: u16) {
        if lines_to_scroll == 0 {
            return;
        }
        log::debug!(
            "Scrolling by {}",
            lines_to_scroll,
        );
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::ScrollUp(lines_to_scroll),
        )
        .unwrap();

        self.range_start = self.range_start.saturating_sub(lines_to_scroll);
        self.range_end = self.range_end.saturating_sub(lines_to_scroll);
    }

    fn finalize(&mut self) {

        log::debug!("Finalizing layout pre  scroll  {:?}", self);
        self.scroll_by((self.range_end+1).saturating_sub(self.terminal_height));
        log::debug!("Finalizing layout post scroll  {:?}", self);


       crossterm::execute!(
           std::io::stdout(),
           crossterm::cursor::MoveTo(
               0,
               self.range_end,
           ),
       ).unwrap();

    

    }
}

struct App<'a> {
    is_running: bool,
    buffer: TextArea<'a>,
    animation_tick: u64,
    cursor_animation: CursorAnimation,
    ps1: Text<'a>,
    /// Parsed bash history available at startup.
    history: Vec<String>,
    history_index: usize,
    is_multiline_mode: bool,
    call_type_cache: std::collections::HashMap<String, (bash_funcs::CommandType, String)>,
    layout_manager: LayoutManager,
}

impl<'a> App<'a> {
    fn new(ps1: Text<'a>, history: Vec<String>, terminal_area: Rect) -> Self {
        let num_rows_of_prompt = ps1.lines.len() as u16;
        assert!(num_rows_of_prompt > 0, "PS1 must have at least one line");


        // let mut buffer = TextArea::new(vec![PS1.to_string()]);
        // buffer.move_cursor(CursorMove::End);
        let buffer = TextArea::default();
        let history_index = history.len();
        App {
            is_running: true,
            buffer,
            animation_tick: 0,
            cursor_animation: CursorAnimation::new(),
            ps1: ps1.to_owned(),
            history,
            history_index,
            is_multiline_mode: false,
            call_type_cache: std::collections::HashMap::new(),
            layout_manager: LayoutManager::new(terminal_area),
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
                        todo!("Handle mouse event: {:?}", mouse_event);
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
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer.move_cursor(CursorMove::WordBack);
            }
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => {
                self.buffer.move_cursor(CursorMove::Back);
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.buffer.move_cursor(CursorMove::WordForward);
            }
            KeyEvent {
                code: KeyCode::Right,
                ..
            } => {
                self.buffer.move_cursor(CursorMove::Forward);
            }
            KeyEvent {
                code: KeyCode::Home,
                ..
            } => {
                self.buffer.move_cursor(CursorMove::Head);
            }
            KeyEvent {
                code: KeyCode::End, ..
            } => {
                self.buffer.move_cursor(CursorMove::End);
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                let (cursor_row, _) = self.buffer.cursor();
                let new_hist_index = self.history_index.saturating_sub(1);
                if cursor_row == 0 && new_hist_index < self.history.len() {
                    // Replace current buffer with last history entry
                    log::debug!(
                        "Up key: Replacing buffer with history index {}",
                        new_hist_index
                    );
                    let new_command = self.history[new_hist_index].clone();
                    self.buffer = TextArea::from(vec![new_command.as_str()]);
                    self.buffer.move_cursor(CursorMove::End);
                    self.history_index = new_hist_index;
                } else {
                    self.buffer.move_cursor(CursorMove::Up);
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                let (cursor_row, _) = self.buffer.cursor();
                let new_hist_index = self.history_index.saturating_add(1);
                if cursor_row + 1 >= self.buffer.lines().len()
                    && new_hist_index < self.history.len()
                {
                    log::debug!(
                        "Down key: Replacing buffer with history index {}",
                        new_hist_index
                    );
                    let new_command = self.history[new_hist_index].clone();
                    self.buffer = TextArea::from(vec![new_command.as_str()]);
                    self.buffer.move_cursor(CursorMove::End);
                    self.history_index = new_hist_index;
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
    }

    fn get_ps1_lines(ps1: Text) -> Vec<Line> {
        let lines = ps1.lines;
        lines
            .into_iter()
            .map(|line| {
                let spans: Vec<Span> = line
                    .spans
                    .into_iter()
                    .map(|span| {
                        if span.content.contains("JOBU_TIME_XXX") {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap();
                            let time_str = format!(
                                "{:02}:{:02}:{:02}.{:03}",
                                (now.as_secs() / 3600) % 24,  // hours
                                (now.as_secs() / 60) % 60,     // minutes
                                now.as_secs() % 60,            // seconds
                                now.subsec_millis()            // milliseconds
                            );
                            Span::styled(
                                span.content.replace("JOBU_TIME_XXX", &time_str),
                                span.style,
                            )
                        } else {
                            span
                        }
                    })
                    .collect();
                Line::from(spans)
            })
            .collect()
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

    fn ui(&mut self, f: &mut Frame) {

        let mut output_lines: Vec<Line> = Self::get_ps1_lines(self.ps1.clone());

        self.cursor_animation.update_position(
            (
                self.buffer.cursor().0.try_into().unwrap(),
                self.buffer.cursor().1.try_into().unwrap(),
            ),
            self.animation_tick,
        );

        let (cursor_row, cursor_col) = self.cursor_animation.get_position(self.animation_tick);
        let cursor_row = cursor_row as usize;
        let mut cursor_col = cursor_col as usize;
        let cursor_intensity = self.cursor_animation.get_intensity(self.animation_tick);

        // Clone lines to break the borrow so we can call get_command_type
        let lines: Vec<String> = self.buffer.lines().to_vec();

        for (i, line) in lines.iter().enumerate() {
            let new_line = if i == 0 {
                // Combine the last PS1 line with the first buffer line
                let last_ps1_line = output_lines.pop().unwrap_or_else(|| Line::from(""));

                if cursor_row == 0 {
                    // TODO: unicode width and all that
                    cursor_col += last_ps1_line.width();
                }
                let mut combined_spans = last_ps1_line.spans;

                let space_pos = line.find(' ').unwrap_or(line.len());
                let (first_word, rest) = line.split_at(space_pos);

                let (command_type, _short_desc) = self.get_command_type(first_word);

                let is_first_word_recognized = command_type != bash_funcs::CommandType::Unknown;

                let first_word_style = if is_first_word_recognized {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                };

                combined_spans.push(Span::styled(first_word.to_string(), first_word_style));
                combined_spans.push(Span::styled(rest.to_string(), Style::default()));

                Line::from(combined_spans)
            } else {
                Line::from(line.as_str())
            };

            let final_line = if i == cursor_row && self.is_running {
                let color = ratatui::style::Color::Rgb(
                    cursor_intensity,
                    cursor_intensity,
                    cursor_intensity,
                );
                let cursor_style = ratatui::style::Style::new().bg(color);

                // Split the line at cursor position and apply cursor style
                let mut styled_spans = Vec::new();
                let mut current_col = 0;

                for span in new_line.spans {
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

                Line::from(styled_spans)
            } else {
                new_line
            };

            output_lines.push(final_line);
        }

        let output = Paragraph::new(output_lines).wrap(Wrap { trim: false });
        let full_terminal_area = f.area();
        let output_num_lines = output.line_count(full_terminal_area.width) as u16;

        let area = self.layout_manager.get_area(output_num_lines);
    
        f.render_widget(&output, area);

    }
}
