use std::io;

use crossterm::event::{ KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
    TerminalOptions, Viewport,
};
use log::{info, error, debug};


use crate::events;

pub async fn get_command() -> String {
    let options = TerminalOptions {
        // TODO: consider restricting viewport
        viewport: Viewport::Fullscreen,
    };
    let mut stdout = std::io::stdout();
    std::io::Write::flush(&mut stdout).unwrap();
    let mut terminal = ratatui::try_init_with_options(options).unwrap();

    let starting_cursor_position = crossterm::cursor::position().unwrap();
    

    let mut app = App::new(starting_cursor_position);
    app.run(terminal).await;
    crossterm::terminal::disable_raw_mode().unwrap();
    app.buffer
}

struct App {
    is_running: bool,
    buffer: String,
    starting_cursor_position: (u16, u16),
}

impl App {
    fn new(starting_cursor_position: (u16, u16)) -> Self {
        App { is_running: true, buffer: String::new(), starting_cursor_position }
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
                    events::Event::AnimationTick => {}
                    events::Event::Resize => {}
                }
            }
        }

    }

    fn onkeypress(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.buffer.push(c);
            }
            KeyCode::Backspace => {
                self.buffer.pop();
            }
            KeyCode::Enter => {
                self.is_running = false;
            }
            _ => {}
        }
    }

    fn ui(&mut self, f: &mut Frame) {
        info!("Rendering UI: {:?}", f);
        let size = f.area();
        info!("starting_cursor_position: {:?}", self.starting_cursor_position);
        let sx = self.starting_cursor_position.0.min(size.width.saturating_sub(1));
        let sy = self.starting_cursor_position.1.min(size.height.saturating_sub(1));
        let width = size.width.saturating_sub(sx).max(1);
        let height = size.height.saturating_sub(sy).max(1);
        let area = Rect { x: sx, y: sy, width, height };
        info!("Calculated drawing area: {:?}", area);
        // let area = Rect { x: 10, y: 10, width: 10, height: 1 };
        // let area = Rect { x: 2, y: 2, width: 10, height: 3 };

        // let paragraph = Paragraph::new(Text::from(self.buffer.as_str()))
        //     .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(Text::from(self.buffer.as_str()), area);
    }
}