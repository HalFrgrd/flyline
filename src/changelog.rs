pub(crate) const CHANGELOG: &str = r#"# Changelog

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
