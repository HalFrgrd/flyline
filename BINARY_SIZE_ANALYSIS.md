# Binary Size Analysis: `libflyline.so`

This document reports the binary size contribution of each non-core dependency in `libflyline.so`
and explains *why* each one adds the size it does.

## Methodology

For each crate the project was compiled from a fixed baseline with that crate removed (or replaced
with a minimal stub) and the resulting `.so` size compared to baseline.  Crates that were too
deeply integrated to stub cleanly (chrono, itertools) were analysed via `nm --size-sort`.

**Baseline**: 4 928 648 bytes (4.70 MB) – release build on host, no profile tweaks.

---

## Results (sorted by size impact)

| # | Crate | Approx. saving | Disposition |
|---|-------|---------------|-------------|
| 1 | `skim` | **~559 KB** | Keep – but replacement would help |
| 2 | `pulldown-cmark` | **~440 KB** | Keep – but large for an optional feature |
| 3 | `clap` + `clap_complete` | **~431 KB** | Core CLI – keep |
| 4 | `lscolors` | **~386 KB** | Keep – but brings heavy deps |
| 5 | `serde` + `serde_json` | **~70 KB** | ✅ **Removed** (hand-rolled JSON parser) |
| 6 | `glob` | **~59 KB** | Keep |
| 7 | `parse-style` | **~37 KB** | Keep |
| 8 | `rand` | **~19 KB** | ✅ **Removed** (inline LCG PRNG) |
| 9 | `chrono` | **~9 KB** | Keep |
| 10 | `ansi-to-tui` (direct dep) | **~9 KB** | Keep |
| 11 | `itertools` | **~2 KB** | Keep |
| 12 | `timeago` | **< 1 KB** | ✅ **Removed** (inline formatter) |
| 13 | `color-eyre` | **~0 KB** | ✅ **Removed** (never used) |

---

## Detailed findings

### 1. `skim` (~559 KB) — **Largest contributor**

`skim` is a full fuzzy-finder TUI application (an `fzf` clone in Rust).  flyline only uses it for
one thing: the `ArinaeMatcher` fuzzy-scoring algorithm.  But even with
`default-features = false, features = []`, skim drags in its entire dependency tree:

| Unique dep | Why it's large |
|---|---|
| `tui-term` + `vt100` + `vte` | Full VT100/xterm terminal emulator (~150 KB) |
| `shell-quote` + `bstr` + `regex-automata` | Text processing for shell integration |
| `tempfile` | Temporary file management (spawning external processes) |
| `tokio-util` | Async I/O utilities needed by the TUI event loop |
| `ansi-to-tui v8.0.1` | A *second* copy (skim's own copy vs. flyline's fork) |
| `serde` + `serde_derive` | Skim serialises state to/from JSON |

**Recommendation**: Replace with a standalone fuzzy-matching crate (`fuzzy-matcher`,
`nucleo-matcher`, or a simple hand-rolled scorer).  All three call sites use the same 3 lines of
API (`ArinaeMatcher::new`, `.fuzzy_match`, `.fuzzy_indices`).  This alone would save ~559 KB.

---

### 2. `pulldown-cmark` (~440 KB)

`pulldown-cmark` is a CommonMark-compliant Markdown parser.  flyline uses it exclusively in
**agent mode** (`src/agent_mode.rs`) to render the prose that the AI model returns alongside
its JSON suggestions.

Why is it so large even with `default-features = false`?

* It includes full Unicode-aware scanners for every CommonMark construct (tables, footnotes,
  math, HTML, ...).
* Its parser is generated as a large DFA / jump-table; the compiled form occupies
  significant `.rodata` and `.text` space.

**Recommendation**: Implement a lightweight Markdown renderer that handles only the subset
flyline needs (bold, italic, headings, inline code, code blocks, lists, tables).  The current
`markdown_to_text` function in `agent_mode.rs` consumes about 150 lines; a self-contained
parser targeting only those constructs could be written in ~300 lines with zero external deps.

---

### 3. `clap` + `clap_complete` (~431 KB)

`clap` is a comprehensive argument-parsing library.  flyline uses it heavily: the entire
`flyline set-*` subcommand family is driven by clap's `#[derive(Parser)]` macro.  The
`clap_complete` crate generates shell completion scripts for bash/zsh/fish.

~431 KB of named symbols are linked from `clap_builder`.  Clap is "core" CLI infrastructure
and replacing it with a lighter parser (e.g. `pico-args`, `argh`) would require a significant
rewrite of `src/lib.rs` and `src/settings.rs`.

---

### 4. `lscolors` (~386 KB)

`lscolors` parses the `LS_COLORS` environment variable and returns a ratatui `Style` for a
given filesystem path.  It is only called in `bash_funcs.rs`.

The large size comes from its two unique dependencies:

| Dep | Why it's large |
|---|---|
| `nu-ansi-term v0.50.3` | Full ANSI terminal styling library (2 900 lines of source) |
| `aho-corasick v1.1.4` | Multi-pattern string matcher used to parse `LS_COLORS` tokens efficiently |

Note: `aho-corasick` is *shared* with `flash` and `skim`; it would not be eliminated by
removing only `lscolors`.  The ~386 KB saving comes primarily from `nu-ansi-term` plus the
`lscolors`-specific monomorphisation of path-matching code.

**Recommendation**: Implement a custom `LS_COLORS` parser (~100 lines) that handles the common
`di=01;34:fi=0:*.rs=01;32:...` format directly.  No external deps needed.

---

### 5. `serde` + `serde_json` (~70 KB) — ✅ Removed

These were used in exactly one place: parsing the JSON array that the AI agent writes to stdout
(`src/agent_mode.rs`).  The schema is simple: `[{"command": "...", "description": "..."}]`.

A hand-rolled recursive-descent parser (~80 lines) replaces serde/serde_json entirely with no
loss of functionality.  All 12 agent_mode tests continue to pass.

---

### 6. `glob` (~59 KB)

`glob` provides pattern matching for tab-completion (`src/app/tab_completion.rs`).  The crate
itself is small (< 1 000 lines), but it forces the Rust linker to instantiate several layers of
`std::fs::read_dir` / `DirEntry` / `PathBuf` iterator machinery that might otherwise be
dead-eliminated.  This explains the disproportionate 59 KB saving when it is removed.

---

### 7. `parse-style` (~37 KB)

`parse-style` is used in `src/palette.rs` to parse user-supplied style strings like `"bold red"`.
The savings come from parsing machinery (nom-style combinator or similar) inside the crate.

**Recommendation**: Implement a simple hand-rolled parser.  The format is well-defined:
space-separated tokens from a fixed vocabulary (colour names + attribute keywords).  ~50 lines.

---

### 8. `rand` (~19 KB) — ✅ Removed

`rand` was used exclusively in `src/content_builder.rs` to drive the matrix-rain animation.
It was replaced with a simple 64-bit LCG (linear congruential generator) implemented inline in
8 lines of code.  The visual result is indistinguishable.

---

### 9. `chrono` (~9 KB)

`chrono` provides timezone-aware date/time and strftime-style formatting for bash prompt
time-escapes (`\t`, `\T`, `\@`, `\A`, `\D{format}`).  Direct symbol contribution is small
(~9 KB).  Its unique deps (`iana-time-zone`, `num-traits`) add a further few KB.  The
functionality is not easily replaced with `std::time` because `strftime`-style formatting
requires `chrono`'s format engine.

---

### 10. `ansi-to-tui` (direct dep) (~9 KB)

The HalFrgrd fork of `ansi-to-tui` (via git dependency) converts ANSI escape sequences from
decoded bash prompt output into ratatui `Text`.  It is called once in `src/prompt_manager.rs`.
The `nom` parser combinator library it depends on is already pulled in, so the net cost is low.

---

### 11. `itertools` (~2 KB)

Direct symbol contribution is tiny.  `itertools` provides convenience methods (`.zip_longest()`,
`.join()`, `.merge()`, `.chunk_by()`, `.tuple_windows()`) that are inlined at their call sites.
The actual code lives inside the monomorphised callers, not in named `itertools::` symbols.
Removing it would require reimplementing these methods locally across 7 files.

---

### 12. `timeago` (< 1 KB) — ✅ Removed

A single call: `timeago::format_5chars(duration)`.  Replaced with a 20-line inline formatter
that produces identical output format.

---

### 13. `color-eyre` (~0 KB) — ✅ Removed

The crate was listed in `Cargo.toml` but never imported or used anywhere in the source.
The linker already dead-eliminated it, so binary size did not change — but removing it
simplifies the dependency tree and speeds up `cargo check`.

---

## Changes applied in this PR

The following real improvements were applied (all 320 library tests still pass):

1. **Removed `color-eyre`** — unused dependency.
2. **Removed `rand`** — replaced with inline LCG in `content_builder.rs`.
3. **Removed `timeago`** — replaced with inline duration formatter in `app/mod.rs`.
4. **Removed `serde` + `serde_json`** — replaced with hand-rolled JSON parser in `agent_mode.rs`.
5. **Fixed `skim` 3.7.0 API** — `ArinaeMatcher::new` now takes 3 arguments; updated all 3 call sites.

**Net savings: ~84 KB** (4 928 648 → 4 843 072 bytes).

## Largest remaining opportunities

| Opportunity | Estimated saving | Effort |
|---|---|---|
| Replace `skim` fuzzy-matcher with standalone crate | ~559 KB | Medium |
| Replace `pulldown-cmark` with custom Markdown renderer | ~440 KB | Medium |
| Replace `lscolors` with custom `LS_COLORS` parser | ~386 KB | Low |
| Replace `parse-style` with inline parser | ~37 KB | Low |
| Replace `glob` with `std::fs::read_dir` + custom glob | ~59 KB | Medium |
