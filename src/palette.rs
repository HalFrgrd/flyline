use ratatui::style::{Color, Modifier, Style};

pub struct Pallete;

impl Pallete {
    pub fn recognised_word() -> Style {
        Style::default().fg(Color::Green)
    }
    pub fn unrecognised_word() -> Style {
        Style::default().fg(Color::Red)
    }
    pub fn secondary_text() -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }
    pub fn selection_style() -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }
    pub fn selected_matching_char() -> Style {
        Self::matched_character().add_modifier(Modifier::REVERSED)
    }
    pub fn normal_text() -> Style {
        Style::default()
    }
    pub fn matched_character() -> Style {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    }
    pub fn cursor_style(intensity: u8) -> Style {
        Style::new().bg(Color::Rgb(intensity, intensity, intensity))
    }
}
