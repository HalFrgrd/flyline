use ratatui::style::{Color, Modifier, Style};
use strum::EnumIter;

use crate::cursor::CursorStyleConfig;
use crate::settings::ColourTheme;

/// Visual interaction state for an interactive button-like cell
/// (clipboard slots, the PS1 copy-buffer button, tutorial buttons, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    /// No mouse interaction with the cell.
    Normal,
    /// The mouse cursor is hovering over the cell.
    Hovered,
    /// The mouse cursor is hovering over the cell and the left mouse button
    /// is currently held down.
    Depressed,
}

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

/// Parse a cursor style string into a [`CursorStyleConfig`].
///
/// Special values:
/// - `"reverse"` (case-insensitive): returns [`CursorStyleConfig::Reverse`].
/// - `"default"` (case-insensitive): returns [`CursorStyleConfig::Default`].
///
/// Otherwise the string is parsed as a rich-style expression with one difference
/// from [`parse_str_to_style`]: a **single colour with no `on` keyword** is
/// treated as the **background** colour of the cursor cell (e.g. `"red"` →
/// `bg(red)`).  When an explicit `on` is present (e.g. `"pink on white"`) the
/// foreground and background are used as-is.
pub fn parse_cursor_style_str(s: &str) -> Result<CursorStyleConfig, String> {
    use parse_style::{Attribute, Style as ParseStyle};
    use ratatui::style::Modifier;

    if s.eq_ignore_ascii_case("reverse") {
        return Ok(CursorStyleConfig::Reverse);
    }
    if s.eq_ignore_ascii_case("default") {
        return Ok(CursorStyleConfig::Default);
    }

    let parsed: ParseStyle = s.parse().map_err(|e| format!("{e}"))?;
    let mut style = Style::default();

    match (parsed.get_foreground(), parsed.get_background()) {
        (None, None) => {}
        // Single colour → treat as background
        (Some(fg), None) => {
            style = style.bg(parse_color_to_ratatui(fg));
        }
        (fg, Some(bg)) => {
            if let Some(f) = fg {
                style = style.fg(parse_color_to_ratatui(f));
            }
            style = style.bg(parse_color_to_ratatui(bg));
        }
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
    Ok(CursorStyleConfig::Custom(style))
}

fn parse_color_to_ratatui(c: parse_style::Color) -> ratatui::style::Color {
    use parse_style::Color;
    match c {
        Color::Default => ratatui::style::Color::Reset,
        Color::Color256(c256) => ratatui::style::Color::Indexed(c256.0),
        Color::Rgb(rgb) => ratatui::style::Color::Rgb(rgb.red(), rgb.green(), rgb.blue()),
    }
}

/// All individually-configurable palette slots.
///
/// The kebab-case name of each variant (e.g. `"recognised-command"`) is used
/// in the `flyline set-colour --style NAME=STYLE` command-line interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, strum::Display, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum PaletteStyleKind {
    RecognisedCommand,
    UnrecognisedCommand,
    SingleQuotedText,
    DoubleQuotedText,
    SecondaryText,
    InlineSuggestion,
    TutorialHint,
    MatchingChar,
    OpeningAndClosingPair,
    NormalText,
    Comment,
    EnvVar,
    MarkdownHeading1,
    MarkdownHeading2,
    MarkdownHeading3,
    MarkdownCode,
    KeySequenceStyle,
    SelectedText,
    BashReserved,
}

/// The colour palette.  One [`Style`] per slot.
///
/// Use [`Palette::apply_theme`] to reset all slots from a built-in preset,
/// then call [`Palette::set`] (or set the public fields directly) to customise
/// individual slots.
#[derive(Debug, Clone)]
pub struct Palette {
    recognised_command: Style,
    unrecognised_command: Style,
    single_quoted_text: Style,
    double_quoted_text: Style,
    secondary_text: Style,
    inline_suggestion: Style,
    tutorial_hint: Style,
    matching_char: Style,
    opening_and_closing_pair: Style,
    normal_text: Style,
    comment: Style,
    env_var: Style,
    markdown_heading1: Style,
    markdown_heading2: Style,
    markdown_heading3: Style,
    markdown_code: Style,
    key_sequence_style: Style,
    selected_text: Style,
    bash_reserved: Style,
}

impl Palette {
    // ── Getters ───────────────────────────────────────────────────────

    pub fn recognised_command(&self) -> Style {
        self.recognised_command
    }

    pub fn unrecognised_command(&self) -> Style {
        self.unrecognised_command
    }

    pub fn single_quoted_text(&self) -> Style {
        self.single_quoted_text
    }

    pub fn double_quoted_text(&self) -> Style {
        self.double_quoted_text
    }

    pub fn secondary_text(&self) -> Style {
        self.secondary_text
    }

    pub fn inline_suggestion(&self) -> Style {
        self.inline_suggestion
    }

    pub fn tutorial_hint(&self) -> Style {
        self.tutorial_hint
    }

    pub fn matching_char(&self) -> Style {
        self.matching_char
    }

    pub fn opening_and_closing_pair(&self) -> Style {
        self.opening_and_closing_pair
    }

    pub fn normal_text(&self) -> Style {
        self.normal_text
    }

    pub fn comment(&self) -> Style {
        self.comment
    }

    pub fn env_var(&self) -> Style {
        self.env_var
    }

    pub fn markdown_heading1(&self) -> Style {
        self.markdown_heading1
    }

    pub fn markdown_heading2(&self) -> Style {
        self.markdown_heading2
    }

    pub fn markdown_heading3(&self) -> Style {
        self.markdown_heading3
    }

    pub fn markdown_code(&self) -> Style {
        self.markdown_code
    }

    pub fn key_sequence_style(&self) -> Style {
        self.key_sequence_style
    }

    pub fn selected_text(&self) -> Style {
        self.selected_text
    }

    pub fn bash_reserved(&self) -> Style {
        self.bash_reserved
    }

    // ── Setter ────────────────────────────────────────────────────────

    /// Set an individual palette slot by kind.
    pub fn set(&mut self, kind: PaletteStyleKind, style: Style) {
        match kind {
            PaletteStyleKind::RecognisedCommand => self.recognised_command = style,
            PaletteStyleKind::UnrecognisedCommand => self.unrecognised_command = style,
            PaletteStyleKind::SingleQuotedText => self.single_quoted_text = style,
            PaletteStyleKind::DoubleQuotedText => self.double_quoted_text = style,
            PaletteStyleKind::SecondaryText => self.secondary_text = style,
            PaletteStyleKind::InlineSuggestion => self.inline_suggestion = style,
            PaletteStyleKind::TutorialHint => self.tutorial_hint = style,
            PaletteStyleKind::MatchingChar => self.matching_char = style,
            PaletteStyleKind::OpeningAndClosingPair => self.opening_and_closing_pair = style,
            PaletteStyleKind::NormalText => self.normal_text = style,
            PaletteStyleKind::Comment => self.comment = style,
            PaletteStyleKind::EnvVar => self.env_var = style,
            PaletteStyleKind::MarkdownHeading1 => self.markdown_heading1 = style,
            PaletteStyleKind::MarkdownHeading2 => self.markdown_heading2 = style,
            PaletteStyleKind::MarkdownHeading3 => self.markdown_heading3 = style,
            PaletteStyleKind::MarkdownCode => self.markdown_code = style,
            PaletteStyleKind::KeySequenceStyle => self.key_sequence_style = style,
            PaletteStyleKind::SelectedText => self.selected_text = style,
            PaletteStyleKind::BashReserved => self.bash_reserved = style,
        }
    }

    // ── Presets ──────────────────────────────────────────────────────

    /// Dark-terminal defaults (the original flyline palette).
    pub fn dark() -> Self {
        Palette {
            recognised_command: Style::default().fg(Color::Green),
            unrecognised_command: Style::default().fg(Color::Red),
            single_quoted_text: Style::default().fg(Color::Yellow),
            double_quoted_text: Style::default().fg(Color::Magenta),
            secondary_text: Style::default().add_modifier(Modifier::DIM),
            inline_suggestion: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::ITALIC),
            tutorial_hint: Style::default().add_modifier(Modifier::BOLD),
            matching_char: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            opening_and_closing_pair: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
            normal_text: Style::default(),
            comment: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::ITALIC),
            env_var: Style::default().fg(Color::Cyan),
            markdown_heading1: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            markdown_heading2: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            markdown_heading3: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            markdown_code: Style::default().add_modifier(Modifier::DIM),
            key_sequence_style: Style::default().add_modifier(Modifier::DIM),
            selected_text: Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(255, 102, 102)),
            bash_reserved: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Light-terminal defaults.
    pub fn light() -> Self {
        Palette {
            recognised_command: Style::default().fg(Color::Green).bold(),
            unrecognised_command: Style::default().fg(Color::Red).bold(),
            single_quoted_text: Style::default().fg(Color::Magenta),
            double_quoted_text: Style::default().fg(Color::Magenta),
            secondary_text: Style::default().dim().bold(),
            inline_suggestion: Style::default()
                .fg(Color::Blue)
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
            comment: Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
            env_var: Style::default().fg(Color::Blue),
            markdown_heading1: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            markdown_heading2: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            markdown_heading3: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            markdown_code: Style::default().add_modifier(Modifier::DIM),
            key_sequence_style: Style::default().fg(Color::DarkGray),
            selected_text: Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(255, 102, 102)),
            bash_reserved: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Reset all palette slots to the given theme preset.
    pub fn apply_theme(&mut self, mode: ColourTheme) {
        *self = match mode {
            ColourTheme::Dark => Self::dark(),
            ColourTheme::Light => Self::light(),
        };
    }

    // ── Derived / constant styles ───────────────────────────────────

    pub fn convert_to_highlighted(style: Style) -> Style {
        style.add_modifier(Modifier::REVERSED)
    }

    /// Apply the styling that corresponds to a non-normal [`ButtonState`] on
    /// top of `style`. Callers should branch on [`ButtonState::Normal`]
    /// themselves and only invoke this for `Hovered` or `Depressed`.
    pub fn apply_button_style(style: Style, state: ButtonState) -> Style {
        match state {
            ButtonState::Normal => style,
            ButtonState::Hovered => style.add_modifier(Modifier::REVERSED),
            ButtonState::Depressed => style
                .fg(Color::Black)
                .bg(Color::Rgb(100, 100, 100))
                .add_modifier(Modifier::BOLD),
        }
    }

    pub fn convert_to_selected(&self, style: Style) -> Style {
        style.patch(self.selected_text())
    }

    pub fn cursor_style(intensity: u8) -> Style {
        Style::new().bg(Color::Rgb(intensity, intensity, intensity))
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}
