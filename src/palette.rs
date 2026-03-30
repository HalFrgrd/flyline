use itertools::Itertools;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Which built-in colour preset is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefaultMode {
    #[default]
    Dark,
    Light,
}

/// The colour palette. Holds all configurable styles.
#[derive(Debug, Clone)]
pub struct Palette {
    /// Which built-in preset was used as the base.
    pub default_mode: DefaultMode,
    /// Style for recognised (valid) commands.
    pub recognised_word: Style,
    /// Style for unrecognised (invalid) commands.
    pub unrecognised_word: Style,
    /// Style for single-quoted strings.
    pub single_quoted_word: Style,
    /// Style for double-quoted strings.
    pub double_quoted_word: Style,
    /// Style for secondary / muted text.
    pub secondary_text: Style,
    /// Style for inline history suggestions shown to the right of the cursor.
    pub inline_suggestion: Style,
    /// Style for tutorial hint text.
    pub tutorial_hint: Style,
    /// Style for matched characters in fuzzy-search results.
    pub matching_char: Style,
    /// Style for opening/closing bracket pairs when the cursor is on one.
    pub opening_and_closing_pair: Style,
    /// Style for normal (unstyled) text.
    pub normal_text: Style,
}

impl Palette {
    // ── Presets ──────────────────────────────────────────────────────

    /// Dark-terminal defaults (the original flyline palette).
    pub fn dark() -> Self {
        Palette {
            default_mode: DefaultMode::Dark,
            recognised_word: Style::default().fg(Color::Green),
            unrecognised_word: Style::default().fg(Color::Red),
            single_quoted_word: Style::default().fg(Color::Yellow),
            double_quoted_word: Style::default().fg(Color::Red),
            secondary_text: Style::default().add_modifier(Modifier::DIM),
            inline_suggestion: Style::default()
                .add_modifier(Modifier::DIM)
                .add_modifier(Modifier::ITALIC),
            tutorial_hint: Style::default().add_modifier(Modifier::BOLD),
            matching_char: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            opening_and_closing_pair: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            normal_text: Style::default(),
        }
    }

    /// Light-terminal defaults.
    pub fn light() -> Self {
        Palette {
            default_mode: DefaultMode::Light,
            recognised_word: Style::default().fg(Color::DarkGray),
            unrecognised_word: Style::default().fg(Color::Red),
            single_quoted_word: Style::default().fg(Color::Yellow),
            double_quoted_word: Style::default().fg(Color::Magenta),
            secondary_text: Style::default().fg(Color::DarkGray),
            inline_suggestion: Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
            tutorial_hint: Style::default().add_modifier(Modifier::BOLD),
            matching_char: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            opening_and_closing_pair: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            normal_text: Style::default(),
        }
    }

    // ── Derived / constant styles ───────────────────────────────────

    pub fn convert_to_selected(style: Style) -> Style {
        style.add_modifier(Modifier::REVERSED)
    }

    pub fn cursor_style(intensity: u8) -> Style {
        Style::new().bg(Color::Rgb(intensity, intensity, intensity))
    }

    pub fn highlight_maching_indices(
        &self,
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
                        base_style.patch(self.matching_char),
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

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}
