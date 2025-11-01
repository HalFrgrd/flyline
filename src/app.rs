
use std::vec;

use crossterm::{ event::{ KeyCode, KeyEvent, KeyModifiers,}};
use ratatui::{
    layout::Rect, text::Line, widgets::{Paragraph, Wrap}, DefaultTerminal, Frame, TerminalOptions, Viewport
};
use ratatui::prelude::*;
use tui_textarea::{CursorMove, Key, TextArea};
use crate::events;
use ansi_to_tui::IntoText;
use std::fs;

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

pub async fn get_command() -> String {
    let options = TerminalOptions {
        // TODO: consider restricting viewport
        viewport: Viewport::Fullscreen,
    };
    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let terminal = ratatui::Terminal::with_options(backend, options).unwrap();

    let starting_cursor_position = crossterm::cursor::position().unwrap();
    
    // get PS1 from environment
    let ps1: String = std::env::var("PS1").unwrap_or( "default> ".to_string());
    // Strip literal "\[" and "\]" markers from PS1 (they wrap non-printing sequences)
    let ps1 = ps1.replace("\\[", "").replace("\\]", "");
    let ps1: Text = ps1.into_text().unwrap_or("bad ps1>".into());
    // println!("PS1: {}", ps1);
    // let ps1: Text = Text::from(ps1);
    // let ps1 = Text::from("default> ");


    log::debug!("Starting cursor position: {:?}", starting_cursor_position);

    // Parse the user's bash history into a vector of command strings.
    let history = parse_bash_history();

    let mut app = App::new(ps1, starting_cursor_position.1, history);
    app.run(terminal).await;

    crossterm::terminal::disable_raw_mode().unwrap();
        crossterm::execute!(
        std::io::stdout(),
        crossterm::cursor::MoveTo(
            starting_cursor_position.0,
            starting_cursor_position.1
        ),
        crossterm::cursor::Show

    ).unwrap();

    let command = app.buffer.lines().join("\n");

    command
}


struct App<'a> {
    is_running: bool,
    buffer: TextArea<'a>,
    cursor_intensity: f32,
    ticks: u64,
    ps1: Text<'a>,
    /// Parsed bash history available at startup.
    history: Vec<String>,
    history_index: usize,
    is_multiline_mode: bool,
    num_rows_above_prompt: u16,
    num_rows_of_prompt: u16,
}

impl<'a> App<'a> {
    fn new(ps1: Text<'a>, num_rows_above_prompt: u16, history: Vec<String>) -> Self {
        let num_rows_of_prompt = ps1.lines.len() as u16;
        assert!(num_rows_of_prompt > 0, "PS1 must have at least one line");

        // let mut buffer = TextArea::new(vec![PS1.to_string()]);
        // buffer.move_cursor(CursorMove::End);
        let buffer = TextArea::default();
        let history_index = history.len();
        App {
            is_running: true, 
            buffer,
            cursor_intensity: 0.0,
            ticks: 0,
            ps1: ps1.to_owned(),
            history,
            history_index,
            is_multiline_mode: false,
            num_rows_above_prompt,
            num_rows_of_prompt,
        }
    }

    pub async fn run(&mut self, mut terminal: DefaultTerminal) {
        // Update application state here
        let mut events = events::EventHandler::new();
        loop {
            terminal.draw(|f| self.ui(f)).unwrap();
            if !self.is_running {
                break;
            } 

            if let Some(event) = events.receiver.recv().await{
                match event {
                    events::Event::Key(event) => {
                        self.onkeypress(event);
                    }
                    events::Event::Mouse(mouse_event) => {
                        todo!("Handle mouse event: {:?}", mouse_event);
                    }
                    events::Event::AnimationTick => {
                        // Toggle cursor visibility for blinking effect
                        self.ticks += 1;
                        let mult = 0.004 * events::ANIMATION_TICK_RATE_MS as f32;
                        self.cursor_intensity = (self.ticks as f32 * mult).sin() * 0.4 + 0.6;
                    }
                    events::Event::Resize => {}
                }
            }
        }
        crossterm::execute!(
            std::io::stdout(),
            crossterm::cursor::MoveTo(
                0,
                self.num_rows_above_prompt + self.num_rows_of_prompt + 1
            ),
        ).unwrap();
    }

    fn increase_num_rows_below_prompt(&mut self, lines_to_scroll: u16) {
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::ScrollUp(lines_to_scroll),
        ).unwrap();
        self.num_rows_above_prompt -= lines_to_scroll;
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
        match key {
            KeyEvent{code: KeyCode::Backspace, modifiers: KeyModifiers::NONE, ..} => {
                self.buffer.delete_char();
            }
            KeyEvent{code: KeyCode::Backspace, modifiers: KeyModifiers::CONTROL, ..} => {
                self.buffer.delete_word();
            }
            KeyEvent{code: KeyCode::Char('h'), modifiers: KeyModifiers::CONTROL, ..} => {
                self.buffer.delete_word();
            }
            KeyEvent{code: KeyCode::Delete, modifiers: KeyModifiers::CONTROL, ..} => {
                self.buffer.delete_next_word();
            }
            KeyEvent{code: KeyCode::Delete, ..} => {
                // self.buffer.move_cursor(CursorMove::Forward);
                self.buffer.delete_next_char();
            }
            KeyEvent{code: KeyCode::Left, ..} => {
                self.buffer.move_cursor(CursorMove::Back);
            }
            KeyEvent{code: KeyCode::Right, ..} => {
                self.buffer.move_cursor(CursorMove::Forward);
            }
            KeyEvent{code: KeyCode::Home, ..} => {
                self.buffer.move_cursor(CursorMove::Head);
            }
            KeyEvent{code: KeyCode::End, ..} => {
                self.buffer.move_cursor(CursorMove::End);
            }
            KeyEvent{code: KeyCode::Up, ..} => {
                let (cursor_row, _) = self.buffer.cursor();
                let new_hist_index = self.history_index.saturating_sub(1);
                if cursor_row == 0 && new_hist_index < self.history.len() {
                    // Replace current buffer with last history entry
                    let new_command = self.history[new_hist_index].clone();
                    self.buffer = TextArea::from(vec![new_command.as_str()]);
                    self.buffer.move_cursor(CursorMove::End);
                    self.history_index = new_hist_index;
                } else {
                    self.buffer.move_cursor(CursorMove::Up);
                }
            }
            KeyEvent{code: KeyCode::Down, ..} => {
                let (cursor_row, _) = self.buffer.cursor();
                let new_hist_index = self.history_index.saturating_add(1);
                if cursor_row + 1 >= self.buffer.lines().len() && new_hist_index < self.history.len() {
                    let new_command = self.history[new_hist_index].clone();
                    self.buffer = TextArea::from(vec![new_command.as_str()]);
                    self.buffer.move_cursor(CursorMove::End);
                    self.history_index = new_hist_index;
                } else {
                    self.buffer.move_cursor(CursorMove::Down);
                }
            }
            KeyEvent{code: KeyCode::Enter, ..} => {
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
            KeyEvent{code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, ..} => {
                self.buffer = TextArea::from(vec!["#Ctrl+C pressed"]);
                self.is_running = false;
            }
            KeyEvent{code: KeyCode::Char(c), ..} => {
                self.buffer.insert_char(c);
            }
            _ => {}
        }
    }

    fn get_ps1_lines(ps1: Text) -> Vec<Line> {
        let lines = ps1.lines;
        lines.into_iter().map(|line| {
            let spans: Vec<Span> = line.spans.into_iter().map(|span| {
                if span.content.contains("JOBU_TIME_XXXXX") {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap();
                    let secs = now.as_secs();
                    let millis = now.subsec_millis();
                    let hours = (secs / 3600) % 24;
                    let minutes = (secs / 60) % 60;
                    let seconds = secs % 60;
                    let time_str = format!("{:02}:{:02}:{:03}.{:03}", hours, minutes, seconds, millis);
                    Span::styled(span.content.replace("JOBU_TIME_XXXXX", &time_str), span.style)
                } else {
                    span
                }
            }).collect();
            Line::from(spans)
        }).collect()
    }

    fn ui(&mut self, f: &mut Frame) {
        let full_terminal_area = f.area();
        let [_, area] = Layout::vertical([Constraint::Length(self.num_rows_above_prompt), Constraint::Fill(1)]).areas(full_terminal_area);

        let mut output_lines: Vec<Line> = Self::get_ps1_lines(self.ps1.clone());

        let (row, mut col) = self.buffer.cursor();

        for (i, line) in self.buffer.lines().iter().enumerate() {
            let new_line = if i == 0 {
                // Combine the last PS1 line with the first buffer line
                let last_ps1_line = output_lines.pop().unwrap_or_else(|| Line::from(""));
                
                if row == 0 {
                    // TODO: unicode width and all that
                    col += last_ps1_line.width();
                }
                let mut combined_spans = last_ps1_line.spans;
                combined_spans.push(Span::raw(line.clone()));
                Line::from(combined_spans)
            } else {
                Line::from(line.clone())
            };

            let final_line = if i == row && self.is_running {
                let intensity = (self.cursor_intensity * 255.0) as u8;
                let color = ratatui::style::Color::Rgb(intensity, intensity, intensity);
                let cursor_style = ratatui::style::Style::new().bg(color);

                // Split the line at cursor position and apply cursor style
                let mut styled_spans = Vec::new();
                let mut current_col = 0;
                
                for span in new_line.spans {
                    let span_text = &span.content;
                    let span_len = span_text.chars().count();
                    
                    if current_col + span_len <= col {
                        // Cursor is after this span
                        styled_spans.push(span);
                        current_col += span_len;
                    } else if current_col >= col + 1 {
                        // Cursor is before this span
                        styled_spans.push(span);
                    } else {
                        // Cursor is within this span
                        let chars: Vec<char> = span_text.chars().collect();
                        let cursor_pos_in_span = col - current_col;
                        
                        // Text before cursor
                        if cursor_pos_in_span > 0 {
                            let before: String = chars[..cursor_pos_in_span].iter().collect();
                            styled_spans.push(Span::styled(before, span.style));
                        }
                        
                        // Character at cursor position
                        if cursor_pos_in_span < chars.len() {
                            let cursor_char = chars[cursor_pos_in_span].to_string();
                            styled_spans.push(Span::styled(cursor_char, span.style.patch(cursor_style)));
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
                if col >= current_col {
                    styled_spans.push(Span::styled(" ", cursor_style));
                }
                
                Line::from(styled_spans)
            } else {
                new_line
            };

            output_lines.push(final_line);
        }

        // if self.is_running {
        //     let intensity = (self.cursor_intensity * 255.0) as u8;
        //     let color = ratatui::style::Color::Rgb(intensity, intensity, intensity);
        //     let cursor_style = ratatui::style::Style::new().bg(color);

        // }


        let output = Paragraph::new(output_lines).wrap(Wrap { trim: false });

        let num_lines = output.line_count(area.width) as u16;
        if num_lines + self.num_rows_above_prompt > full_terminal_area.height {
            let lines_to_scroll = num_lines + self.num_rows_above_prompt - full_terminal_area.height;
            self.increase_num_rows_below_prompt(lines_to_scroll);
        }

        f.render_widget(&output, area);

        // let area = Rect { x: sx + 40, y: sy, width, height };
        // f.render_widget(Line::from("test").fg(ratatui::style::Color::Red), area);
    }
}