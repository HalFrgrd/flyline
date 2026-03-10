use itertools::Itertools;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

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
    pub fn convert_to_selected(style: Style) -> Style {
        style.add_modifier(Modifier::REVERSED)
    }
    pub fn normal_text() -> Style {
        Style::default()
    }
    pub fn matched_character() -> Style {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    }
    pub fn opening_and_closing_pair() -> Style {
        Self::matched_character().add_modifier(Modifier::UNDERLINED)
    }

    pub fn cursor_style(intensity: u8) -> Style {
        Style::new().bg(Color::Rgb(intensity, intensity, intensity))
    }

    pub fn highlight_maching_indices(
        s: &str,
        matching_indices: &[usize],
        base_style: Style,
    ) -> Vec<Line<'static>> {
        let mut normal_lines = Vec::new();

        let mut char_offset = 0usize;
        for text_line in s.split('\n') {
            let line_char_count = text_line.chars().count();
            let line_end_offset = char_offset + line_char_count;

            let relative_indices: Vec<usize> = matching_indices
                .iter()
                .filter(|&&idx| idx >= char_offset && idx < line_end_offset)
                .map(|&idx| idx - char_offset)
                .collect();

            let mut normal_spans = Vec::new();

            for (is_matching, chunk) in &text_line
                .char_indices()
                .chunk_by(|(idx, _)| relative_indices.contains(idx))
            {
                let chunk_str = chunk.map(|(_, c)| c).collect::<String>();
                if is_matching {
                    normal_spans.push(Span::styled(
                        chunk_str,
                        base_style.patch(Palette::matched_character()),
                    ));
                } else {
                    normal_spans.push(Span::styled(chunk_str, base_style));
                }
            }

            normal_lines.push(Line::from(normal_spans));

            char_offset = line_end_offset + 1; // +1 for the '\n' character
        }

        normal_lines
    }
}
