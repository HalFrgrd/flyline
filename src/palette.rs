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

/// The colour palette. Holds theme-default styles and per-field user overrides.
///
/// Each style is stored as a default (`field: Style`, from the active theme preset)
/// and an optional user override (`field_override: Option<Style>`).  The getter
/// method (`palette.field()`) returns the override when set, falling back to the
/// theme default.
///
/// Use [`Palette::apply_theme`] to change the theme defaults (overrides are left
/// untouched).  Set `palette.field_override = Some(style)` directly to record a
/// user override.
#[derive(Debug, Clone)]
pub struct Palette {
    /// Which built-in preset is active.
    pub default_mode: DefaultMode,

    recognised_command: Style,
    pub recognised_word_override: Option<Style>,

    unrecognised_command: Style,
    pub unrecognised_word_override: Option<Style>,

    single_quoted_text: Style,
    pub single_quoted_word_override: Option<Style>,

    double_quoted_text: Style,
    pub double_quoted_word_override: Option<Style>,

    secondary_text: Style,
    pub secondary_text_override: Option<Style>,

    inline_suggestion: Style,
    pub inline_suggestion_override: Option<Style>,

    tutorial_hint: Style,
    pub tutorial_hint_override: Option<Style>,

    matching_char: Style,
    pub matching_char_override: Option<Style>,

    opening_and_closing_pair: Style,
    pub opening_and_closing_pair_override: Option<Style>,

    normal_text: Style,
    pub normal_text_override: Option<Style>,
}

impl Palette {
    // ── Getters (override wins over theme default) ────────────────────

    pub fn recognised_command(&self) -> Style {
        self.recognised_word_override
            .unwrap_or(self.recognised_command)
    }

    pub fn unrecognised_command(&self) -> Style {
        self.unrecognised_word_override
            .unwrap_or(self.unrecognised_command)
    }

    pub fn single_quoted_text(&self) -> Style {
        self.single_quoted_word_override
            .unwrap_or(self.single_quoted_text)
    }

    pub fn double_quoted_text(&self) -> Style {
        self.double_quoted_word_override
            .unwrap_or(self.double_quoted_text)
    }

    pub fn secondary_text(&self) -> Style {
        self.secondary_text_override.unwrap_or(self.secondary_text)
    }

    pub fn inline_suggestion(&self) -> Style {
        self.inline_suggestion_override
            .unwrap_or(self.inline_suggestion)
    }

    pub fn tutorial_hint(&self) -> Style {
        self.tutorial_hint_override.unwrap_or(self.tutorial_hint)
    }

    pub fn matching_char(&self) -> Style {
        self.matching_char_override.unwrap_or(self.matching_char)
    }

    pub fn opening_and_closing_pair(&self) -> Style {
        self.opening_and_closing_pair_override
            .unwrap_or(self.opening_and_closing_pair)
    }

    pub fn normal_text(&self) -> Style {
        self.normal_text_override.unwrap_or(self.normal_text)
    }

    /// Apply a new theme preset to the default style values, leaving any
    /// user-specified overrides intact.
    pub fn apply_theme(&mut self, mode: DefaultMode) {
        match mode {
            DefaultMode::Dark => {
                self.default_mode = DefaultMode::Dark;
                self.recognised_command = Style::default().fg(Color::Green);
                self.unrecognised_command = Style::default().fg(Color::Red);
                self.single_quoted_text = Style::default().fg(Color::Yellow);
                self.double_quoted_text = Style::default().fg(Color::Red);
                self.secondary_text = Style::default().add_modifier(Modifier::DIM);
                self.inline_suggestion = Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::DIM)
                    .add_modifier(Modifier::ITALIC);
                self.tutorial_hint = Style::default().add_modifier(Modifier::BOLD);
                self.matching_char = Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD);
                self.opening_and_closing_pair = Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED);
                self.normal_text = Style::default();
            }
            DefaultMode::Light => {
                self.default_mode = DefaultMode::Light;
                self.recognised_command = Style::default().fg(Color::DarkGray);
                self.unrecognised_command = Style::default().fg(Color::Red);
                self.single_quoted_text = Style::default().fg(Color::Yellow);
                self.double_quoted_text = Style::default().fg(Color::Magenta);
                self.secondary_text = Style::default().fg(Color::DarkGray);
                self.inline_suggestion = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC);
                self.tutorial_hint = Style::default().add_modifier(Modifier::BOLD);
                self.matching_char = Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD);
                self.opening_and_closing_pair = Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED);
                self.normal_text = Style::default();
            }
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
                        base_style.patch(self.matching_char()),
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
        let mut p = Self {
            default_mode: DefaultMode::Dark,
            recognised_command: Style::default(),
            recognised_word_override: None,
            unrecognised_command: Style::default(),
            unrecognised_word_override: None,
            single_quoted_text: Style::default(),
            single_quoted_word_override: None,
            double_quoted_text: Style::default(),
            double_quoted_word_override: None,
            secondary_text: Style::default(),
            secondary_text_override: None,
            inline_suggestion: Style::default(),
            inline_suggestion_override: None,
            tutorial_hint: Style::default(),
            tutorial_hint_override: None,
            matching_char: Style::default(),
            matching_char_override: None,
            opening_and_closing_pair: Style::default(),
            opening_and_closing_pair_override: None,
            normal_text: Style::default(),
            normal_text_override: None,
        };
        p.apply_theme(DefaultMode::Dark);
        p
    }
}
