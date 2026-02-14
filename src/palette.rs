use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Span};
use itertools::Itertools;

pub struct Palette;

impl Palette {
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

    pub fn highlight_maching_indices(s: &str, matching_indices: &[usize]) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let mut normal_spans = Vec::new();
        let mut selected_spans = Vec::new();

        for (is_matching, chunk) in &s
            .char_indices()
            .chunk_by(|(idx, _)| matching_indices.contains(idx))
        {
            let chunk_str = chunk.map(|(_, c)| c).collect::<String>();
            if is_matching {
                normal_spans.push(Span::styled(
                    chunk_str.clone(),
                    Palette::matched_character(),
                ));
                selected_spans
                    .push(Span::styled(chunk_str, Palette::selected_matching_char()));
            } else {
                normal_spans.push(Span::styled(chunk_str.clone(), Palette::normal_text()));
                selected_spans.push(Span::styled(chunk_str, Palette::selection_style()));
            }
        }

        (normal_spans, selected_spans)
    }
}
