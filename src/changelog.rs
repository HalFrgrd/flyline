use termina::escape::csi::{Csi, Sgr, SgrAttributes, SgrModifiers};
use termina::style::ColorSpec;

pub(crate) const CHANGELOG: &str = r#"# Changelog

## v1.4.0
- **Inline Viewport Smooth Height**: Viewport height pre-allocates to available space down to the bottom of the screen without scrolling up, eliminating viewport resize flicker when opening popups.
- **Third-Party Integration**: Enhanced support and terminal state synchronization for third-party tools (Atuin, FZF).
- **Customizable PS2**: Added support for customizable PS2 multi-line prompt rendering.
- **Settings & Config**: Exposed Flycomp settings in Flyline and added options to disable easter eggs.
- **Bug Fixes & Stability**: Resolved PATH scan lock contention, zero-width terminal suggestion popup panics, and unterminated quote auto-newline insertion.

## v1.3.0
- **Leader Keys**: Added support for chorded keybinding sequences (e.g., `Ctrl+x` followed by `Ctrl+f`) via the new `setLeaderKey` and `unsetLeaderKey` actions and the `leaderKeyActive` context variable.
- **Leader Key Visual Feedback**: Introduced the `leader-mode` prompt widget to display visual indicators (like ` X `) in the prompt when the leader key state is active.
- **String Insertion Action**: `insertString(...)` action allows inserting arbitrary strings into the buffer.
- **Strict Modifier Matching**: Switched to strict modifier equality matching to prevent modifier-overlap conflicts when dispatching key actions.
- **Key List Autocomplete & Completion**: Added autocomplete support for listing keybindings for a specific key event (`flyline key list <key>`).

## v1.2.5
- **Global Allocator**: Integrated `mimalloc` to bypass Bash's non-thread-safe allocator and prevent heap corruption on multi-threaded allocations.
- **Nested Arithmetic Lexing**: Stateful lexing updates to correctly parse nested brackets/parentheses inside arithmetic `$(( ... ))` blocks.
- **Word Under Cursor breaks**: Updated word-under-cursor (WUC) detection to respect `:` and `=`, matching bash's standard `COMP_WORD_BREAK` behavior.
- **Kitty Cursor Support**: Added backend selection to keep the terminal emulator cursor visible on Kitty, preventing prompts when closing the window.

## v1.2.4
- **Safety Guards**: Fixed a Use-After-Free (UAF) issue, added safety guards, and enforced usage of the thread manager.
- **Mouse UX Improvements**: Corrected mouse event output formatting and resolved layout bugs, ensuring mouse event rows are always fully printed.
- **Robust WUC Handling**: Patched Word Under Cursor (WUC) edge cases and downgraded internal assertions to errors to prevent shell crashes.
- **AUR Package**: Documented and referenced the official Arch Linux User Repository (AUR) package.
- **Cleanups**: Removed the legacy `get_current_readline_prompt` hook dependency to streamline FFI interactions.

## v1.2.3
- **Thread Safety**: Added `BASH_LOCK` to prevent concurrency crashes when accessing Bash FFI from background threads.
- **Log Forwarding**: Pipes tab-completion child logs back to the parent to prevent double-logging and preserve trails.
- **Fuzzy Mode**: Added `flyline suggestions set-fuzzy-mode` (`all`, `none`, `folder-prefixes`) for folder prefix matching.

## v1.2.2
- **Changelog Command**: Added `flyline changelog` command to display user-facing changelogs directly in the pager.
- **Upgrade Assistant**: Added `flyline upgrade` command which pre-fills the prompt line with the curl installer command.
- **Installer improvements**: Streamlined `install.sh` to run non-interactively, resolving target folders automatically.

## v1.2.1
- **Declarative Mouse Actions**: Re-architected mouse event processing into a declarative, context-aware routing system.
- **Tab Completion Latency**: Reduced visual flashing during tab completion redraws and optimized filtering latency for large lists.
- **Offline Installer**: Updated `install.sh` to bypass GitHub API rate limits by resolving release redirect headers.
- **Wider Platform Support**: Added release builds for FreeBSD, ARMv7, 32-bit x86, RISC-V 64, and PowerPC 64 LE.
- **OSC 52 Paste**: Replaced custom OSC 52 querying with crossterm's native RequestClipboardContents.

## v1.2.0
- **Transient Prompts**: Added support for transient prompts, reducing terminal noise by condensing past prompts upon execution.
- **History Management**: Introduced separate history managers for cancelled commands and agent prompts.
- **Non-blocking Completion**: Improved tab-completion responsiveness by spawning completion generation in a dedicated process.
- **Scroll & Right-Click UX**: Enhanced right-click context menu and continuous proportional scrollbar dragging.

## v1.1.0
- **Fuzzy Sorting**: Introduced suggestion sorting algorithms (mtime, alphabetical) and CLI configuration options.
- **Improved Parsing**: Enhanced flycomp parsing for cargo, git --help, and flag values ending in `=`.
- **Fuzzy Matching**: Tightened fuzzy suggestion matching and fixed scrollbar positions.

## v1.0.0
- **Stable Line Editor**: First major release of the Rust-based GNU readline replacement builtin for Bash.
- **Mouse Selection**: Support for cursor placement and visual drag-selections using mouse.
- **Auto-Closing pairs**: Automatic insertion of closing quotes, brackets, and parentheses.
- **Interactive Tutorial**: Added an in-terminal tutorial to guide users through keyboard and mouse controls.
"#;

/// Reorders the markdown changelog string so that older versions appear first
/// and the most recent version is printed LAST at the bottom of the output.
pub fn get_reordered_changelog() -> String {
    let mut sections = Vec::new();
    let mut header = String::new();
    let mut current_section = String::new();

    for line in CHANGELOG.lines() {
        if line.starts_with("# ") {
            header.push_str(line);
            header.push('\n');
        } else if line.starts_with("## ") {
            if !current_section.trim().is_empty() {
                sections.push(current_section);
            }
            current_section = String::new();
            current_section.push_str(line);
            current_section.push('\n');
        } else if !current_section.is_empty() {
            current_section.push_str(line);
            current_section.push('\n');
        } else if !header.is_empty() {
            header.push_str(line);
            header.push('\n');
        }
    }
    if !current_section.trim().is_empty() {
        sections.push(current_section);
    }

    sections.reverse();

    let mut out = header;
    out.push('\n');
    for sec in sections {
        out.push_str(&sec);
        out.push('\n');
    }
    out
}

/// Renders the styled changelog using the existing markdown parser and palette styling.
pub fn render_styled_changelog() -> String {
    let palette = crate::palette::Palette::default();
    let reordered_md = get_reordered_changelog();
    let parsed_text = crate::agent_mode::markdown_to_text(&reordered_md, &palette);

    text_to_ansi(&parsed_text)
}

fn text_to_ansi(text: &ratatui::text::Text<'static>) -> String {
    let mut out = String::new();
    for (i, line) in text.lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for span in &line.spans {
            if termina::style::Stylized::is_ansi_color_disabled() {
                out.push_str(&span.content);
            } else {
                let (prefix, reset) = style_to_ansi(span.style);
                out.push_str(&prefix);
                out.push_str(&span.content);
                out.push_str(&reset);
            }
        }
    }
    out
}

fn style_to_ansi(style: ratatui::style::Style) -> (String, String) {
    let mut modifiers = SgrModifiers::empty();
    if style.add_modifier.contains(ratatui::style::Modifier::BOLD) {
        modifiers |= SgrModifiers::INTENSITY_BOLD;
    }
    if style
        .add_modifier
        .contains(ratatui::style::Modifier::ITALIC)
    {
        modifiers |= SgrModifiers::ITALIC;
    }

    let fg = style.fg.map(ratatui_color_to_termina);
    let bg = style.bg.map(ratatui_color_to_termina);

    let prefix = Csi::Sgr(Sgr::Attributes(SgrAttributes {
        modifiers,
        foreground: fg,
        background: bg,
        ..Default::default()
    }))
    .to_string();

    let reset = Csi::Sgr(Sgr::Reset).to_string();
    (prefix, reset)
}

fn ratatui_color_to_termina(color: ratatui::style::Color) -> ColorSpec {
    use ratatui::style::Color;
    match color {
        Color::Reset => ColorSpec::WHITE,
        Color::Black => ColorSpec::BLACK,
        Color::Red => ColorSpec::RED,
        Color::Green => ColorSpec::GREEN,
        Color::Yellow => ColorSpec::YELLOW,
        Color::Blue => ColorSpec::BLUE,
        Color::Magenta => ColorSpec::MAGENTA,
        Color::Cyan => ColorSpec::CYAN,
        Color::Gray => ColorSpec::WHITE,
        Color::DarkGray => ColorSpec::BLACK,
        Color::LightRed => ColorSpec::RED,
        Color::LightGreen => ColorSpec::GREEN,
        Color::LightYellow => ColorSpec::YELLOW,
        Color::LightBlue => ColorSpec::BLUE,
        Color::LightMagenta => ColorSpec::MAGENTA,
        Color::LightCyan => ColorSpec::CYAN,
        Color::White => ColorSpec::WHITE,
        Color::Rgb(red, green, blue) => ColorSpec::TrueColor(termina::style::RgbaColor {
            red,
            green,
            blue,
            alpha: 255,
        }),
        Color::Indexed(i) => ColorSpec::PaletteIndex(i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_styled_changelog_order() {
        let output = render_styled_changelog();

        let v1_0_pos = output.find("v1.0.0").expect("v1.0.0 present");
        let v1_3_pos = output.find("v1.3.0").expect("v1.3.0 present");
        let unreleased_pos = output.find("Unreleased").expect("Unreleased present");

        // Oldest version (v1.0.0) should appear before newer versions (v1.3.0 and Unreleased)
        assert!(v1_0_pos < v1_3_pos);
        assert!(v1_3_pos < unreleased_pos);
    }
}
