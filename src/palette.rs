use clap_complete::CompletionCandidate;
use ratatui::style::{Color, Modifier, Style};
use strum::{EnumIter, EnumMessage, IntoEnumIterator};

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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, EnumIter, EnumMessage, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "kebab-case")]
pub enum PaletteStyleKind {
    #[strum(message = "Syntax highlighting for recognised shell commands (e.g. ls, git)")]
    RecognisedCommand,
    #[strum(message = "Syntax highlighting for unrecognised commands")]
    UnrecognisedCommand,
    #[strum(message = "Syntax highlighting for single-quoted strings")]
    SingleQuotedText,
    #[strum(message = "Syntax highlighting for double-quoted strings")]
    DoubleQuotedText,
    #[strum(message = "Dimmed style for secondary and decorative text")]
    SecondaryText,
    #[strum(message = "Style for inline history suggestions shown after the cursor")]
    InlineSuggestion,
    #[strum(message = "Style for tutorial hint text")]
    TutorialHint,
    #[strum(message = "Highlight style for characters matched by fuzzy search")]
    MatchingChar,
    #[strum(message = "Style for matched opening/closing bracket or quote pairs")]
    OpeningAndClosingPair,
    #[strum(message = "Default style for unclassified command buffer text")]
    NormalText,
    #[strum(message = "Syntax highlighting for shell comments (text after #)")]
    Comment,
    #[strum(message = "Syntax highlighting for environment variable references (e.g. $HOME)")]
    EnvVar,
    #[strum(message = "Style for level-1 Markdown headings (# heading)")]
    MarkdownHeading1,
    #[strum(message = "Style for level-2 Markdown headings (## heading)")]
    MarkdownHeading2,
    #[strum(message = "Style for level-3 Markdown headings (### heading)")]
    MarkdownHeading3,
    #[strum(message = "Style for inline code spans in Markdown")]
    MarkdownCode,
    #[strum(message = "Style used to render key sequences in the UI")]
    KeySequenceStyle,
    #[strum(message = "Highlight style for the current text selection")]
    SelectedText,
    #[strum(message = "Syntax highlighting for bash reserved words (e.g. if, while, for)")]
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
            selected_text: Style::default().bg(Color::LightRed),
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
            selected_text: Style::default().bg(Color::LightRed),
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

/// Tab-completion for the `NAME=STYLE` arguments of `flyline set-style`.
///
/// Yields each [`PaletteStyleKind`] name (in kebab-case) with `=` appended and
/// `NO_SUFFIX` so that flyline's completion engine suppresses the trailing
/// space, allowing the user to type the style value immediately after the `=`.
///
/// When the current token already contains `=` the user is typing the style
/// value (a free-form rich-style string), so no candidates are returned.
pub fn possible_style_name_completions(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy().to_string();
    if current.contains('=') {
        return Vec::new();
    }
    let current_lower = current.to_lowercase();
    PaletteStyleKind::iter()
        .filter_map(|kind| {
            let name = kind.to_string();
            if name.to_lowercase().contains(&current_lower) {
                let candidate = CompletionCandidate::new(format!("{}=NO_SUFFIX", name))
                    .help(kind.get_message().map(clap::builder::StyledStr::from));
                Some(candidate)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_possible_style_name_completions_empty_yields_all() {
        let values: Vec<String> = possible_style_name_completions(std::ffi::OsStr::new(""))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect();
        assert!(values.contains(&"recognised-command=NO_SUFFIX".to_string()));
        assert!(values.contains(&"inline-suggestion=NO_SUFFIX".to_string()));
        assert!(values.contains(&"bash-reserved=NO_SUFFIX".to_string()));
        assert_eq!(values.len(), PaletteStyleKind::iter().count());
    }

    #[test]
    fn test_possible_style_name_completions_partial_filters() {
        let values: Vec<String> = possible_style_name_completions(std::ffi::OsStr::new("inline"))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect();
        assert!(values.contains(&"inline-suggestion=NO_SUFFIX".to_string()));
        assert!(!values.contains(&"recognised-command=NO_SUFFIX".to_string()));
    }

    #[test]
    fn test_possible_style_name_completions_after_equals_returns_empty() {
        let values =
            possible_style_name_completions(std::ffi::OsStr::new("inline-suggestion=bold"));
        assert!(values.is_empty());
    }

    #[test]
    fn test_possible_style_name_completions_have_help() {
        let candidates = possible_style_name_completions(std::ffi::OsStr::new("recognised"));
        assert!(!candidates.is_empty());
        for c in &candidates {
            assert!(c.get_help().is_some());
        }
    }
}
