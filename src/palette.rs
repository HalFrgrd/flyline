use itertools::Itertools;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Parse a rich-style string (e.g. `"bold red"`) into a `ratatui::style::Style`.
/// Returns an error message if the string cannot be parsed.
pub fn parse_str_to_style(s: &str) -> Result<ratatui::style::Style, String> {
    use parse_style::{Attribute, Style as ParseStyle};
    use ratatui::style::{Modifier, Style};

    let parsed: ParseStyle = s.parse().map_err(|e| format!("{e}"))?;
    let mut style = Style::default();

    if let Some(fg) = parsed.get_foreground() {
        style = style.fg(parse_color_to_ratatui(fg));
    }
    if let Some(bg) = parsed.get_background() {
        style = style.bg(parse_color_to_ratatui(bg));
    }

    let attr_map: &[(Attribute, Modifier)] = &[
        (Attribute::Bold, Modifier::BOLD),
        (Attribute::Dim, Modifier::DIM),
        (Attribute::Italic, Modifier::ITALIC),
        (Attribute::Underline, Modifier::UNDERLINED),
        (Attribute::Blink, Modifier::SLOW_BLINK),
        (Attribute::Blink2, Modifier::RAPID_BLINK),
        (Attribute::Reverse, Modifier::REVERSED),
        (Attribute::Conceal, Modifier::HIDDEN),
        (Attribute::Strike, Modifier::CROSSED_OUT),
    ];
    for &(attr, modifier) in attr_map {
        if parsed.is_enabled(attr) {
            style = style.add_modifier(modifier);
        }
    }
    Ok(style)
}

fn parse_color_to_ratatui(c: parse_style::Color) -> ratatui::style::Color {
    use parse_style::Color;
    match c {
        Color::Default => ratatui::style::Color::Reset,
        Color::Color256(c256) => ratatui::style::Color::Indexed(c256.0),
        Color::Rgb(rgb) => ratatui::style::Color::Rgb(rgb.red(), rgb.green(), rgb.blue()),
    }
}

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
    pub recognised_command_override: Option<Style>,

    unrecognised_command: Style,
    pub unrecognised_command_override: Option<Style>,

    single_quoted_text: Style,
    pub single_quoted_text_override: Option<Style>,

    double_quoted_text: Style,
    pub double_quoted_text_override: Option<Style>,

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

    comment: Style,
    pub comment_override: Option<Style>,

    env_var: Style,
    pub env_var_override: Option<Style>,

    markdown_heading1: Style,
    pub markdown_heading1_override: Option<Style>,

    markdown_heading2: Style,
    pub markdown_heading2_override: Option<Style>,

    markdown_heading3: Style,
    pub markdown_heading3_override: Option<Style>,

    markdown_code: Style,
    pub markdown_code_override: Option<Style>,
}

impl Palette {
    // ── Getters (override wins over theme default) ────────────────────

    pub fn recognised_command(&self) -> Style {
        self.recognised_command_override
            .unwrap_or(self.recognised_command)
    }

    pub fn unrecognised_command(&self) -> Style {
        self.unrecognised_command_override
            .unwrap_or(self.unrecognised_command)
    }

    pub fn single_quoted_text(&self) -> Style {
        self.single_quoted_text_override
            .unwrap_or(self.single_quoted_text)
    }

    pub fn double_quoted_text(&self) -> Style {
        self.double_quoted_text_override
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

    pub fn comment(&self) -> Style {
        self.comment_override.unwrap_or(self.comment)
    }

    pub fn env_var(&self) -> Style {
        self.env_var_override.unwrap_or(self.env_var)
    }

    pub fn markdown_heading1(&self) -> Style {
        self.markdown_heading1_override
            .unwrap_or(self.markdown_heading1)
    }

    pub fn markdown_heading2(&self) -> Style {
        self.markdown_heading2_override
            .unwrap_or(self.markdown_heading2)
    }

    pub fn markdown_heading3(&self) -> Style {
        self.markdown_heading3_override
            .unwrap_or(self.markdown_heading3)
    }

    pub fn markdown_code(&self) -> Style {
        self.markdown_code_override.unwrap_or(self.markdown_code)
    }

    // ── Presets ──────────────────────────────────────────────────────

    /// Dark-terminal defaults (the original flyline palette).
    pub fn dark() -> Self {
        Palette {
            default_mode: DefaultMode::Dark,
            recognised_command: Style::default().fg(Color::Green),
            recognised_command_override: None,
            unrecognised_command: Style::default().fg(Color::Red),
            unrecognised_command_override: None,
            single_quoted_text: Style::default().fg(Color::Yellow),
            single_quoted_text_override: None,
            double_quoted_text: Style::default().fg(Color::Magenta),
            double_quoted_text_override: None,
            secondary_text: Style::default().add_modifier(Modifier::DIM),
            secondary_text_override: None,
            inline_suggestion: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::ITALIC),
            inline_suggestion_override: None,
            tutorial_hint: Style::default().add_modifier(Modifier::BOLD),
            tutorial_hint_override: None,
            matching_char: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            matching_char_override: None,
            opening_and_closing_pair: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            opening_and_closing_pair_override: None,
            normal_text: Style::default(),
            normal_text_override: None,
            comment: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::ITALIC),
            comment_override: None,
            env_var: Style::default().fg(Color::Cyan),
            env_var_override: None,
            markdown_heading1: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            markdown_heading1_override: None,
            markdown_heading2: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            markdown_heading2_override: None,
            markdown_heading3: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            markdown_heading3_override: None,
            markdown_code: Style::default().add_modifier(Modifier::DIM),
            markdown_code_override: None,
        }
    }

    /// Light-terminal defaults.
    pub fn light() -> Self {
        Palette {
            default_mode: DefaultMode::Light,
            recognised_command: Style::default().fg(Color::DarkGray),
            recognised_command_override: None,
            unrecognised_command: Style::default().fg(Color::Red),
            unrecognised_command_override: None,
            single_quoted_text: Style::default().fg(Color::Yellow),
            single_quoted_text_override: None,
            double_quoted_text: Style::default().fg(Color::Magenta),
            double_quoted_text_override: None,
            secondary_text: Style::default().fg(Color::DarkGray),
            secondary_text_override: None,
            inline_suggestion: Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
            inline_suggestion_override: None,
            tutorial_hint: Style::default().add_modifier(Modifier::BOLD),
            tutorial_hint_override: None,
            matching_char: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            matching_char_override: None,
            opening_and_closing_pair: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            opening_and_closing_pair_override: None,
            normal_text: Style::default(),
            normal_text_override: None,
            comment: Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
            comment_override: None,
            env_var: Style::default().fg(Color::Blue),
            env_var_override: None,
            markdown_heading1: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            markdown_heading1_override: None,
            markdown_heading2: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            markdown_heading2_override: None,
            markdown_heading3: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            markdown_heading3_override: None,
            markdown_code: Style::default().add_modifier(Modifier::DIM),
            markdown_code_override: None,
        }
    }

    /// Apply a new theme preset to the default style values, leaving any
    /// user-specified overrides intact.
    pub fn apply_theme(&mut self, mode: DefaultMode) {
        let template = match mode {
            DefaultMode::Dark => Self::dark(),
            DefaultMode::Light => Self::light(),
        };
        self.default_mode = template.default_mode;
        self.recognised_command = template.recognised_command;
        self.unrecognised_command = template.unrecognised_command;
        self.single_quoted_text = template.single_quoted_text;
        self.double_quoted_text = template.double_quoted_text;
        self.secondary_text = template.secondary_text;
        self.inline_suggestion = template.inline_suggestion;
        self.tutorial_hint = template.tutorial_hint;
        self.matching_char = template.matching_char;
        self.opening_and_closing_pair = template.opening_and_closing_pair;
        self.normal_text = template.normal_text;
        self.comment = template.comment;
        self.env_var = template.env_var;
        self.markdown_heading1 = template.markdown_heading1;
        self.markdown_heading2 = template.markdown_heading2;
        self.markdown_heading3 = template.markdown_heading3;
        self.markdown_code = template.markdown_code;
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
        Self::dark()
    }
}
