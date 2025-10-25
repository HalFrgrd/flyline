
use std::vec;

use crossterm::{ event::{ KeyCode, KeyEvent, KeyModifiers,}};
use ratatui::{
    layout::Rect, text::Line, DefaultTerminal, Frame, TerminalOptions, Viewport
};
use ratatui::prelude::*;
use tui_textarea::{TextArea, CursorMove};
use crate::events;
use ansi_to_tui::IntoText;

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
    // let ps1 = std::env::var("PS1").unwrap_or( "default> ".to_string());
    // let ps1: Text = ps1.into_text().unwrap_or("bad ps1>".into());
    let ps1 = Text::from("default> ");


    log::debug!("Starting cursor position: {:?}", starting_cursor_position);

    let mut app = App::new(ps1, starting_cursor_position.1);
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
    is_multiline_mode: bool,
    num_rows_above_prompt: u16,
    num_rows_of_prompt: u16,
}

impl<'a> App<'a> {
    fn new(ps1: Text<'a>, num_rows_above_prompt: u16) -> Self {
        let num_rows_of_prompt = ps1.lines.len() as u16;
        assert!(num_rows_of_prompt > 0, "PS1 must have at least one line");

        // let mut buffer = TextArea::new(vec![PS1.to_string()]);
        // buffer.move_cursor(CursorMove::End);
        let buffer = TextArea::default();
        App {
            is_running: true, 
            buffer,
            cursor_intensity: 0.0,
            ticks: 0,
            ps1: ps1.to_owned(),
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

    fn ui(&mut self, f: &mut Frame) {
        let full_terminal_area = f.area();
        let [_, area] = Layout::vertical([Constraint::Length(self.num_rows_above_prompt), Constraint::Fill(1)]).areas(full_terminal_area);
        log::info!("area for rendering: {:?} and full_terminal_area: {:?}", area, full_terminal_area);



        // let mut temp = self.buffer.clone();

        let ps1_lines: Vec<Line> = self.ps1.lines.clone();
        let buffer_lines: Vec<Line> = self.buffer.lines().iter().map(|line| Line::from(line.as_str())).collect();

        let (mut row, mut col) = self.buffer.cursor();
        if row == 0 {
            col += ps1_lines.last().map(|line| line.width()).unwrap_or(0);
        }
        row += self.num_rows_of_prompt as usize;
        let mut temp = TextArea::from(ps1_lines.into_iter().chain(buffer_lines.into_iter()).collect::<Vec<Line>>());
        temp.move_cursor(CursorMove::Jump(row as u16, col as u16));

        if self.is_running {
            let intensity = (self.cursor_intensity * 255.0) as u8;
            let color = ratatui::style::Color::Rgb(intensity, intensity, intensity);
            temp.set_cursor_style(ratatui::style::Style::new().bg(color));
        } else {
            temp.set_cursor_style(temp.cursor_line_style());
        }


        let num_lines = temp.lines().len() as u16;
        if num_lines + self.num_rows_above_prompt > full_terminal_area.height {
            let lines_to_scroll = num_lines + self.num_rows_above_prompt - full_terminal_area.height;
            self.increase_num_rows_below_prompt(lines_to_scroll);
        }


        f.render_widget(&temp, area);

        // let area = Rect { x: sx + 40, y: sy, width, height };
        // f.render_widget(Line::from("test").fg(ratatui::style::Color::Red), area);
    }
}