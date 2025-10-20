
use crossterm::event::{ KeyCode, KeyEvent,};
use ratatui::{
    layout::Rect,
 
    text::{ Text},
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
    app.buffer
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
        
        // Create display text with blinking cursor
        let mut display_text = self.buffer.clone();
        
        // Insert cursor at current position if visible
        if self.cursor_visible {
            // Use a block character for the cursor
            let cursor_char = '█';
            if self.cursor_position <= display_text.len() {
                display_text.insert(self.cursor_position, cursor_char);
            }
        } else {
            // When cursor is invisible, still show position with a space if at end
            if self.cursor_position == self.buffer.len() && !self.buffer.is_empty() {
                display_text.push(' ');
            }
        }

        f.render_widget(Text::from(display_text), area);
    }
}