
use crossterm::{cursor, event::{ KeyCode, KeyEvent, KeyModifiers,}};
use ratatui::{
    layout::Rect, text::{Line, Span, Text}, widgets::Paragraph, DefaultTerminal, Frame, TerminalOptions, Viewport
};
use ratatui::prelude::*;
use log::{info, error, debug};
use tui_textarea::{TextArea, CursorMove};
use crate::events;


pub async fn get_command() -> String {
    let options = TerminalOptions {
        // TODO: consider restricting viewport
        viewport: Viewport::Fullscreen,
    };
    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    crossterm::terminal::enable_raw_mode().unwrap();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::with_options(backend, options).unwrap();

    let starting_cursor_position = crossterm::cursor::position().unwrap();
    
    // get PS1 from environment
    let ps1 = std::env::var("PS1").unwrap_or( "default> ".to_string());

    let mut app = App::new(ps1, starting_cursor_position);
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
    starting_cursor_position: (u16, u16),
    cursor_intensity: f32,
    ticks: u64,
    ps1: String,
}

impl App<'_> {
    fn new(ps1: String, starting_cursor_position: (u16, u16)) -> Self {

        // let mut buffer = TextArea::new(vec![PS1.to_string()]);
        // buffer.move_cursor(CursorMove::End);
        let buffer = TextArea::default();
        App { 
            is_running: true, 
            buffer,
            starting_cursor_position,
            cursor_intensity: 1.0,
            ticks: 0,
            ps1,
        }
    }

    pub async fn run(&mut self, mut terminal: DefaultTerminal) {
        // Update application state here
        let mut events = events::EventHandler::new();
        loop {
            terminal.draw(|f| self.ui(f)).unwrap();

            if let Some(event) = events.receiver.recv().await{
                match event {
                    events::Event::Key(event) => {
                        self.onkeypress(event);
                    }
                    events::Event::Mouse(_) => {}
                    events::Event::AnimationTick => {
                        // Toggle cursor visibility for blinking effect
                        self.ticks += 1;
                        let mult = 0.004 * events::ANIMATION_TICK_RATE_MS as f32;
                        self.cursor_intensity = (self.ticks as f32 * mult).sin() * 0.4 + 0.6;
                    }
                    events::Event::Resize => {}
                }
            }
            if !self.is_running {
                self.cursor_intensity = 0.0;
                terminal.draw(|f| self.ui(f)).unwrap();
                break;
            }
        }

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
                self.is_running = false;
            }
            KeyEvent{code: KeyCode::Char(c), ..} => {
                self.buffer.insert_char(c);
            }
            _ => {}
        }
    }

    fn ui(&mut self, f: &mut Frame) {
        // info!("Rendering UI: {:?}", f);
        let size = f.area();
        // info!("starting_cursor_position: {:?}", self.starting_cursor_position);
        let sx = self.starting_cursor_position.0.min(size.width.saturating_sub(1));
        assert!(sx == 0);
        let sy = self.starting_cursor_position.1.min(size.height.saturating_sub(1));
        let width = size.width.saturating_sub(sx).max(1);
        let height = size.height.saturating_sub(sy).max(1);
        let area = Rect { x: sx, y: sy, width, height };

        let intensity = (self.cursor_intensity * 255.0) as u8;
        let color = ratatui::style::Color::Rgb(intensity, intensity, intensity);
        self.buffer.set_cursor_style(ratatui::style::Style::new().bg(color));

        let mut temp = self.buffer.clone();

        let cursor = temp.cursor();
        let (row, col): (u16, u16) = (cursor.0 as u16, cursor.1 as u16);
        // log::debug!("Cursor position: row {}, col {}", row, col);
        temp.move_cursor(CursorMove::Head);
        temp.insert_str(&self.ps1);
        let col = if row == 0 {
            col + self.ps1.len() as u16
        } else {
            col
        };
        temp.move_cursor(CursorMove::Jump(row, col));
        f.render_widget(&temp, area);

        let area = Rect { x: sx + 40, y: sy, width, height };
        f.render_widget(Line::from("test").fg(ratatui::style::Color::Red), area);
    }
}