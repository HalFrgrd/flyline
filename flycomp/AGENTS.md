# AI Agent Developer Guide: `flycomp`

This document provides context, architecture, build/test instructions, and coding conventions for AI agents working on the [flycomp](.) crate.

## Overview
`flycomp` is a helper crate for `flyline` that parses CLI documentation (command-line `--help` output and `man` pages) to dynamically generate shell completion scripts (for Bash, Zsh, Fish, Elvish, and PowerShell) or extract command configurations in JSON format.

It consists of:
- **CLI parser detectors** to identify option schemas (Clap-style, Argparse-style, etc.) and construct a unified [Command](src/lib.rs#L41-L53) tree.
- **Man page parser** to scrape information from unix `man` formats.
- **Clap converter** to map parsed definitions into a dynamic `clap::Command` and generate completion scripts via `clap_complete`.

---

## Workspace Integration
`flycomp` is a workspace member package of `flyline` configured in the root [Cargo.toml](../Cargo.toml#L7).

- **Library crate name**: `flycomp`
- **Binary crate name**: `flycomp`

---

## Directory & File Tour
- **[Cargo.toml](Cargo.toml)**: Defines the crate dependencies (e.g., `clap`, `clap_complete`, `serde_json`, `regex`).
- **[src/lib.rs](src/lib.rs)**:
  - Defines the public data models [Command](src/lib.rs#L41-L53) and [Arg](src/lib.rs#L25-L38).
  - Implements the [to_clap_command](src/lib.rs#L78) mapper which builds the dynamic Clap CLI definition.
  - Implements the completion synthesis runner [synthesize_completion](src/lib.rs#L208).
- **[src/main.rs](src/main.rs)**: The entrypoint for the standalone `flycomp` binary, parsing options like `--output` and `--strategy`.
- **[src/parse_help.rs](src/parse_help.rs)**: Contains the main help parsing logic, recognizing:
  - Clap style outputs (e.g., standard Rust CLI output).
  - Python's `argparse` module outputs.
  - Generic formatting fallback.
- **[src/parse_man.rs](src/parse_man.rs)**: Scrapes manual pages using groff/mandoc cleaning regex patterns.
- **[tests/man_pages/](../tests/man_pages/)** (located in the workspace root): Contains raw/gzipped manual pages used by the `parse_man.rs` unit tests (loaded using paths like `../tests/man_pages/...`).

---

## Development Workflow

### Build Commands
To compile only the `flycomp` package:
```bash
cargo build -p flycomp
```

To compile the entire `flyline` workspace:
```bash
cargo build
```

### Run Commands
To run the CLI completion generator on a specific target command (e.g., `ls` or `git`):
```bash
# Generate Bash script (default)
cargo run -p flycomp -- ls

# Generate JSON metadata
cargo run -p flycomp -- ls --output json

# Generate Zsh script using man pages strategy only
cargo run -p flycomp -- ls --output zsh --strategy man-page
```

### Test Commands
To run the unit tests inside `flycomp`:
```bash
cargo test -p flycomp
```

To run all workspace tests:
```bash
cargo test
```

---

## Architectural & Coding Guidelines

### 1. Owned Strings Support in Clap 4.x
To avoid leaking memory, `flycomp` enables the `"string"` feature for `clap` in its [Cargo.toml](Cargo.toml).
This allows passing owned `String` parameters directly to `clap::Command::new()`, `clap::Arg::new()`, `.about()`, `.help()`, `.long()`, and `.value_name()` (which all accept types implementing `Into<Str>`). 

> [!TIP]
> Do not introduce manual pointer leaks or `Box::leak` workarounds when writing dynamic command generator code. Simply pass owned `String` structures or clone them.

### 2. Execution of Commands (`RunHelp` Strategy)
The `RunHelp` strategy executes the target binary with `--help` using [std::process::Command](src/lib.rs#L393-L397).
> [!CAUTION]
> Running untrusted binaries is dangerous. Be careful when invoking synthesis on random binary paths. Make sure you don't execute side-effect-heavy or malicious scripts.

### 3. Adding Support for New Parser Formats
If you need to add support for another CLI output format (e.g. Go flag package, Node commander, etc.):
1. Update `HelpFormat` enum in [src/parse_help.rs](src/parse_help.rs).
2. Enhance `detect_format` to recognize the output.
3. Write parser logic and a corresponding test suite validating the command metadata extraction.

### 4. Writing & Maintaining Tests
- If you edit the man-page parser, add unit tests in [src/parse_man.rs](src/parse_man.rs#L1042) using existing mock man files in `tests/man_pages/` or adding new mock files there if needed.
- Ensure all tests compile and pass before declaring a task complete.
