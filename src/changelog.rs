pub(crate) const CHANGELOG: &str = r#"# Changelog

## v1.0.0 (2026-05-19)
- **Stable line editor**: First major release of the Rust-based GNU readline replacement builtin for Bash.
- **Mouse selection**: Support for cursor placement and visual drag-selections using mouse.
- **Auto-closing pairs**: Automatic insertion of closing quotes, brackets, and parentheses.
- **Interactive tutorial**: Added an in-terminal tutorial to guide users through keyboard and mouse controls.

## v1.1.0 (2026-06-12)
- **Fuzzy sorting**: Introduced suggestion sorting algorithms (mtime, alphabetical) and CLI configuration options.
- **Improved parsing**: Enhanced flycomp parsing for cargo, git --help, and flag values ending in `=`.
- **Fuzzy matching**: Tightened fuzzy suggestion matching and fixed scrollbar positions.

## v1.2.0 (2026-06-25)
- **Transient prompts**: Added support for transient prompts, reducing terminal noise by condensing past prompts upon execution.
- **History management**: Introduced separate history managers for cancelled commands and agent prompts.
- **Non-blocking completion**: Improved tab-completion responsiveness by spawning completion generation in a dedicated process.
- **Scroll & right-click UX**: Enhanced right-click context menu and continuous proportional scrollbar dragging.

## v1.2.1 (2026-06-28)
- **Declarative mouse actions**: Re-architected mouse event processing into a declarative, context-aware routing system.
- **Tab completion latency**: Reduced visual flashing during tab completion redraws and optimized filtering latency for large lists.
- **Offline installer**: Updated `install.sh` to bypass GitHub API rate limits by resolving release redirect headers.
- **Wider platform support**: Added release builds for FreeBSD, ARMv7, 32-bit x86, RISC-V 64, and PowerPC 64 LE.
- **OSC 52 paste**: Replaced custom OSC 52 querying with crossterm's native RequestClipboardContents.

## v1.2.2 (2026-07-01)
- **Changelog command**: Added `flyline changelog` command to display user-facing changelogs
- **Upgrade assistant**: Added `flyline upgrade` command which pre-fills the prompt line with the curl installer command.
- **Installer improvements**: Streamlined `install.sh` to run non-interactively, resolving target folders automatically.

## v1.2.3 (2026-07-02)
- **Thread safety**: Added `BASH_LOCK` to prevent concurrency crashes when accessing Bash FFI from background threads.
- **Log forwarding**: Pipes tab-completion child logs back to the parent to prevent double-logging and preserve trails.
- **Fuzzy mode**: Added `flyline suggestions set-fuzzy-mode` (`all`, `none`, `folder-prefixes`) for folder prefix matching.

## v1.2.4 (2026-07-03)
- **Safety guards**: Fixed a Use-After-Free (UAF) issue, added safety guards, and enforced usage of the thread manager.
- **Mouse UX improvements**: Corrected mouse event output formatting and resolved layout bugs, ensuring mouse event rows are always fully printed.
- **Robust WUC handling**: Patched Word Under Cursor (WUC) edge cases and downgraded internal assertions to errors to prevent shell crashes.
- **AUR package**: Documented and referenced the official Arch Linux User Repository (AUR) package.
- **Cleanups**: Removed the legacy `get_current_readline_prompt` hook dependency to streamline FFI interactions.

## v1.2.5 (2026-07-04)
- **Global allocator**: Integrated `mimalloc` to bypass Bash's non-thread-safe allocator and prevent heap corruption on multi-threaded allocations.
- **Nested arithmetic lexing**: Stateful lexing updates to correctly parse nested brackets/parentheses inside arithmetic `$(( ... ))` blocks.
- **Word under cursor breaks**: Updated word-under-cursor (WUC) detection to respect `:` and `=`, matching bash's standard `COMP_WORD_BREAK` behavior.
- **Kitty cursor support**: Added backend selection to keep the terminal emulator cursor visible on Kitty, preventing prompts when closing the window.

## v1.3.0 (2026-07-05)
- **Leader keys**: Added support for chorded keybinding sequences (e.g., `Ctrl+x` followed by `Ctrl+f`) via the new `setLeaderKey` and `unsetLeaderKey` actions and the `leaderKeyActive` context variable.
- **Leader key visual feedback**: Introduced the `leader-mode` prompt widget to display visual indicators (like ` X `) in the prompt when the leader key state is active.
- **String insertion action**: `insertString(...)` action allows inserting arbitrary strings into the buffer.
- **Strict modifier matching**: Switched to strict modifier equality matching to prevent modifier-overlap conflicts when dispatching key actions.
- **Key list autocomplete & completion**: Added autocomplete support for listing keybindings for a specific key event (`flyline key list <key>`).

## v1.4.0 (2026-07-26)
- **Inline viewport smooth height**: Viewport height pre-allocates to available space down to the bottom of the screen without scrolling up, eliminating viewport resize flicker when opening popups.
- **Third-party integration**: Enhanced support and terminal state synchronization for third-party tools (Atuin, FZF).
- **Customizable PS2**: Added support for customizable PS2 multi-line prompt rendering.
- **Packaging & build systems**: Added Nix flake packaging, Arch Linux build fixes, and `SOURCE_DATE_EPOCH` support for reproducible build timestamps.
- **Settings & config**: Exposed Flycomp settings in Flyline and added options to disable easter eggs.
- **Bug fixes & stability**: Resolved PATH scan lock contention, zero-width terminal suggestion popup panics, and unterminated quote auto-newline insertion.

## v1.5.0 (2026-08-03)
- **Termina backend**: Switched terminal rendering backend to `termina` for enhanced event handling and precise UI rendering.
- **Enhanced mouse selection & UX**: Added triple-click line selection, quad-click buffer selection, click-and-drag suggestion selection, and isolated scrolling movements.
- **Platform & packaging support**: Added Android/Termux installation support, a declarative NixOS module, and Homebrew installation documentation.
- **Binary size & build optimization**: Reduced binary size by ~1.3MB by switching to `regex-lite` and improved Arch Linux LTO build options.
- **Agent & subprocess stability**: Fixed `SIGCHLD` signal handler reset behavior when spawning agent command substitutions to prevent process reaping errors (`ECHILD`).
- **Parsing & completion fixes**: Improved square bracket autoclosing, unterminated function acceptance, `autocd` directory path command recognition, quote space-suffix handling, and resolved `extglob` parsing issues.

## v1.6.0 (2026-08-09)
- **Synchronized rendering (mode 2026)**: Added ANSI Mode 2026 support for tear-free, flicker-free rendering in GPU terminals (Ghostty, Alacritty, Kitty, WezTerm).
- **Terminal resize & prompt fixes**: Resolved Ghostty scrollback line erasure on window shrink and fixed prompt position escape code resending on Ctrl+L screen clear.
- **Performance & stability**: Accelerated newline insertion performance, added thread-safe shell exit code handling, and resolved background thread cleanup crashes.
- **Platform support**: Fixed Android/Termux build dependencies and updated installer scripts.

## v1.6.1 (2026-08-10)
- **Viewport resize & wrapping**: Improve terminal emulator detection for auto-resize strategies and disable ratatui auto-wrapping to prevent line shifts on viewport resize.

## v1.6.2 (2026-08-12)
- **Terminal info & heredoc parsing**: Query terminal emulator info using DA2 device attributes, improve global mouse state cleanup, and fix heredoc delimiter parsing logic.
"#;

pub(crate) fn pretty_changelog() -> String {
    let palette = crate::palette::Palette::default();
    let text = crate::agent_mode::markdown_to_text(CHANGELOG, &palette);
    crate::content_utils::text_to_ansi(&text)
}
