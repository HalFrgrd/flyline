# AI Agent Developer Guide: `flyline`

This document provides a simplified developer guide for [flyline](.), a Bash plugin replacing standard GNU readline with a modern, Rust-based line editor.

## Key Files
- **[src/lib.rs](src/lib.rs)**: C FFI bindings loaded directly into the host Bash process (e.g. `flyline_get_char`).
- **[src/app/mod.rs](src/app/mod.rs)**: The main TUI application loop, redraw coordination, and frame rendering.
- **[src/app/actions.rs](src/app/actions.rs)**: Handles keystrokes, keybindings, modes, and command actions.
- **[src/bash_funcs.rs](src/bash_funcs.rs)**: Bridges Rust code with the host Bash shell (variable retrieval, path resolution, and calling Bash functions/hooks).
- **[src/bash_symbols.rs](src/bash_symbols.rs)**: C-compatible definitions of GNU Bash internal types, structures, and global variables.
- **[src/prompt_manager.rs](src/prompt_manager.rs)**: Asynchronous shell prompt widgets, PS1 configurations, and terminal animations.
- **[src/text_buffer.rs](src/text_buffer.rs)**: Text state management, cursor movements, and undo/redo stacks.

## Useful Commands
```bash
# Build the loadable builtin library (target/debug/libflyline.so)
cargo build

# Load the plugin in the current Bash session
enable -f target/debug/libflyline.so flyline

# Unload the plugin and restore default readline
enable -d flyline

# Run unit tests only (avoids slow different-bash-version integration tests)
cargo test --lib

# To run flycomp unit tests specifically
cargo test -p flycomp

# Format the codebase after making changes
cargo fmt
```

> [!TIP]
> Avoid running the full `cargo test` suite locally. The integration tests (`tests/docker_integration_tests.rs`) spawn Docker containers testing multiple versions of Bash, which is extremely slow. Prefer running `cargo test --lib` or testing specific packages.

## Guidelines
1. **Safety & Stability**: `flyline` runs inside the active shell process. Avoid unwinding panics across the C FFI boundary; wrap entry points in `catch_unwind_safe` to prevent shell crashes. Never create an `App` instance in library unit tests, as `App` depends on global FFI symbols (like `history_list` or `current_readline_prompt`) that are only resolved dynamically when loaded inside Bash, causing linker failures in library test targets.
2. **Terminal Rendering**: Uses `ratatui` to draw suggestions, widgets, and tooltips. Ensure components handle narrow or resizing terminal viewports gracefully.
3. **Interactive UI via Tagged Cells**: To map terminal mouse coordinates to actions, `flyline` uses `TaggedCell` ([src/content_builder.rs](src/content_builder.rs#L193)) in its rendering buffer `Contents` ([src/content_builder.rs](src/content_builder.rs#L214)).
   * Each cell associates a `ratatui::buffer::Cell` with a `Tag` enum (e.g., `Tag::Command`, `Tag::Suggestion`, `Tag::TutorialNext`).
   * Use `get_tagged_cell` in [src/app/mod.rs](src/app/mod.rs#L355) to map mouse events (`column`, `row`) to interactive components.
   * When drawing clickable elements or widgets, ensure they are written using tagged methods (like `write_tagged_span` or `write_tagged_line`).

