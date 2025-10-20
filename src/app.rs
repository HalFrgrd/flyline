
use crossterm::{cursor, event::{ KeyCode, KeyEvent,}};
use ratatui::{
    layout::Rect,
    text::{Span, Text, Line},
    DefaultTerminal, Frame,
    TerminalOptions, Viewport,
};
use ratatui::prelude::*;
use log::{info, error, debug};

use crate::events;

const PS1: &str = "my prompt: ";

pub async fn get_command() -> (String, String)   {
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
    
    let mut app = App::new(starting_cursor_position);
    app.run(terminal).await;

    crossterm::terminal::disable_raw_mode().unwrap();
        crossterm::execute!(
        std::io::stdout(),
        crossterm::cursor::MoveTo(
            starting_cursor_position.0,
            starting_cursor_position.1
        )
    ).unwrap();
   (PS1.to_string(), app.buffer)
}

struct App {
    is_running: bool,
    buffer: String,
    starting_cursor_position: (u16, u16),
    cursor_visible: bool,
    cursor_position: usize,
}

impl App {
    fn new(starting_cursor_position: (u16, u16)) -> Self {
        App { 
            is_running: true, 
            buffer: String::new(), 
            starting_cursor_position,
            cursor_visible: true,
            cursor_position: 0,
        }
    }

    pub async fn run(&mut self, mut terminal: DefaultTerminal) {
        // Update application state here
        let mut events = events::EventHandler::new();
        while self.is_running {
            terminal.draw(|f| self.ui(f)).unwrap();

            if let Some(event) = events.receiver.recv().await{
                match event {
                    events::Event::Key(event) => {
                        self.onkeypress(event);
                    }
                    events::Event::Mouse(_) => {}
                    events::Event::AnimationTick => {
                        // Toggle cursor visibility for blinking effect
                        self.cursor_visible = !self.cursor_visible;
                    }
                    events::Event::Resize => {}
                }
            }
        }

    }

    fn onkeypress(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.buffer.remove(self.cursor_position);
                }
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_position < self.buffer.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_position = 0;
            }
            KeyCode::End => {
                self.cursor_position = self.buffer.len();
            }
            KeyCode::Enter => {
                self.is_running = false;
            }
            _ => {}
        }
    }

    fn ui(&mut self, f: &mut Frame) {
        // info!("Rendering UI: {:?}", f);
        let size = f.area();
        info!("starting_cursor_position: {:?}", self.starting_cursor_position);
        let sx = self.starting_cursor_position.0.min(size.width.saturating_sub(1));
        let sy = self.starting_cursor_position.1.min(size.height.saturating_sub(1));
        let width = size.width.saturating_sub(sx).max(1);
        let height = size.height.saturating_sub(sy).max(1);
        let area = Rect { x: sx, y: sy, width, height };
        info!("Calculated drawing area: {:?}", area);
        info!("Current buffer: {}", self.buffer);
        

        let mut line = vec![Span::raw(PS1).style(ratatui::style::Color::Yellow)];
        let mut b = self.buffer.clone();
        let mut cursor_pos = self.cursor_position;
        if self.cursor_visible {
            cursor_pos = cursor_pos.min(b.len());
            if cursor_pos == b.len() {
                b.push_str(" ");
            }
            line.push(Span::raw(&b[..cursor_pos]));

            line.push(Span::raw(&b[cursor_pos..cursor_pos+1]).bg(Color::Yellow));
            if (cursor_pos + 1) < b.len() {
                line.push(Span::raw(&b[cursor_pos+1..]));
            }
        } else {
            line.push(Span::raw(b));
        }

        f.render_widget(
            Line::from_iter(line), area);
    }
}