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

use crate::events;

pub async fn get_command() -> String {
    let options = TerminalOptions {
        viewport: Viewport::Inline(5),
    };
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
        let size = f.area();
        let sx = self.starting_cursor_position.0.min(size.width.saturating_sub(1));
        let sy = self.starting_cursor_position.1.min(size.height.saturating_sub(1));
        let width = size.width.saturating_sub(sx).max(1);
        let height = size.height.saturating_sub(sy).max(1);
        let area = Rect { x: sx, y: sy, width, height };

        let paragraph = Paragraph::new(Text::from(self.buffer.as_str()))
            .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(paragraph, area);
        // let block = Block::default()
        //     .borders(ratatui::widgets::Borders::ALL)
        //     .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::Red))
        //     .border_type(ratatui::widgets::BorderType::Rounded)
        //     .title("Jobu Terminal App")
        //     .title_alignment(ratatui::layout::Alignment::Center);

        // let paragraph = Paragraph::new(Text::from(self.buffer.as_str()))
        //     .block(block)
        //     .alignment(ratatui::layout::Alignment::Left);

        // f.render_widget(paragraph, size);
    }
}