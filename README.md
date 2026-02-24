# Flyline

<div align="center">

[![CI](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml/badge.svg)](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/blob/main/LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/releases)

</div>

A modern `readline` alternative for bash.

Features:
- Undo and redo support
- Cursor animations
- Fuzzy history suggestions
- Fuzzy autocompletions
- Integration with bash autocomplete
- Mouse support:
    - Click to move cursor in buffer
    - Hover over command for tooltips
- Tab completions when writing subshells, command substitutions, process substitutions
- Tab completions for aliases (e.g. if `gc` aliases to `git commit`, `gc --verbo<TAB>` works as expected )

# Installation
Download the latest `libflyline.so`.
In your `.bashrc` (or in your current bash session): `enable -f /path/to/libflyline.so flyline`.


# Integrations
## VsCode:
- I'd recommend setting `terminal.integrated.minimumContrastRatio = 1` to prevent the cell's foreground colour changing it's under the cursor.
- You may want to set `terminal.integrated.macOptionIsMeta` so `Option+key` shortcuts are properly recognised.
- Shell integration WIP

## MacOs
`Command+<KEY>` shortcuts are often captured by the terminal emulator and not forwarded to the shell.
Two fixes are:
- Map `Command+<KEY>` to`Control+<KEY>` in your termianl emulator settings.
- Use a terminal emulator that supports [Kitty's exteneded keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). This allows flyline to receive `Command+<KEY>` events.


