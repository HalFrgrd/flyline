# Flyline

<div align="center">

[![CI](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml/badge.svg)](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/blob/main/LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/releases)

![Demo](assets/demo.gif)

</div>

A bash plugin for modern command line editing. Flyline replaces readline to provide a code-editor-like experience and other features:
- Undo and redo support
- Cursor animations
- Fuzzy history suggestions
- Fuzzy autocompletions
- Integration with bash autocomplete
- Mouse support:
    - Click to move cursor
    - Hover over words for tooltips
- [Improvements on bash's tab completion](#tab-completion-improvements)
- Tooltips
- Auto close brackets and quotes
- Syntax highlighting
- Runs in the same process as bash


# Installation
Download the latest `libflyline.so`.
In your `.bashrc` (or in your current bash session):
```bash
enable -f /path/to/libflyline.so flyline
flyline --tutorial-mode
```


# Integrations
## VS Code:
- I'd recommend setting `terminal.integrated.minimumContrastRatio = 1` to prevent the cell's foreground colour changing when it's under the cursor.
- You may want to set `terminal.integrated.macOptionIsMeta` so `Option+<KEY>` shortcuts are properly recognised.
- Shell integration WIP (https://github.com/HalFrgrd/flyline/issues/52)

## macOS
`Command+<KEY>` shortcuts are often captured by the terminal emulator and not forwarded to the shell.
Two possible fixes are:
- Map `Command+<KEY>` to`Control+<KEY>` in your terminal emulator settings.
- Use a terminal emulator that supports [Kitty's exteneded keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). This allows flyline to receive `Command+<KEY>` events.

# Rich prompts

Flyline supports dynamic content in `PS1`, `RPS1` / `RPROMPT`, and `PS1_FILL`.

## Setting your prompt
#### PS1
The `PS1` environment variable sets the left prompt just like normal. See [bash prompt documentation](https://www.gnu.org/software/bash/manual/html_node/Controlling-the-Prompt.html) for more information or [starship integration](#starship-integration).

![PS1 demo](assets/demo_prompts_ps1.gif)

#### RPS1 / RPROMPT
`RPS1` / `RPROMPT` Either variable sets the right prompt similarly to zsh.

![RPS1 demo](assets/demo_prompts_rps1.gif)

#### PS1_FILL
- `PS1_FILL` fills the gap between the `PS1` and `RPS1` lines.

![PS1_FILL demo](assets/demo_prompts_ps1_fill.gif)

For instance:
![Prompt demo](assets/demo_prompts.gif)
```bash
export PS1='\u@\h:\w$ '
export RPS1='\t'
export PS1='\u@\h:\w\n$ '
export PS1_FILL='-'
export PS1='\[\033[01;32m\]\u@\h\[\033[00m\]:\[\033[01;34m\]\w\[\033[00m\]\n$ '
export PS1_FILL='🯁🯂🯃🮲🮳' # finger pointing to running man
```

## Starship integration
TODO:
Starship provides customizable prompts for any shell. The git metrics prompt part is very useful but can slow down the time it takes to generate the prompt. Because Flyline can redraw the prompt, it can asynchronously load the slower widgets in the background to keep the shell feeling snappy 

## Dynamic time in prompts

Flyline recognises the standard bash time escape sequences and re-evaluates them on every prompt draw, so the time shown is always current:

| Sequence       | Output                          |
|----------------|---------------------------------|
| `\t`           | 24-hour time — `HH:MM:SS`       |
| `\T`           | 12-hour time — `HH:MM:SS`       |
| `\@`           | 12-hour time with am/pm         |
| `\A`           | 24-hour time — `HH:MM`          |
| `\D{format}`   | Custom format (see below)       |

These can be placed in any of the supported prompt variables:

```bash
# Right prompt showing 24-hour time in green
export RPS1='\[\033[01;32m\]\t\[\033[0m\]'

# Right prompt showing 12-hour am/pm time
export RPS1='\[\033[01;34m\]\@\[\033[0m\]'
```

### Custom time format with `\D{format}`

Use `\D{format}` with any [Chrono format string](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) to display the time exactly how you want it. This is similar to `\D{format}` in the [bash prompt documentation](https://www.gnu.org/software/bash/manual/html_node/Controlling-the-Prompt.html), but the format string is interpreted by Chrono rather than strftime.

```bash
# Show date and time
export RPS1='\[\033[01;32m\]\D{%Y-%m-%d %H:%M:%S}\[\033[0m\]'

# Show only hours and minutes
export RPS1='\D{%H:%M}'
```

# Tab completion improvements
Flyline extends bash's tab completion feature in many ways: 

Fuzzy tab completions: when you're presented with suggestions, you can type to fuzzily search through the list:

![Fuzzy suggestions demo](assets/demo_fuzzy_suggestions.gif)

Alias commands: e.g. if `gc` aliases to `git commit`, `gc --verbo<TAB>` works as expected

Tab completions inside subshell, command substitution, and process substitution expressions: TODO: check this doesn't work in bash normally

Mid-word tab completions: when your cursor is mid way through a word and you press tab (e.g. `grep --i<TAB>nvrte`) the left hand side will be used in the programmable completion function but the suggestions will be fuzzily searched using the entire word.

# Command history

## Fuzzy history search
Flyline offers a fuzzy history search similar to fzf or skim accessed with `Ctrl+R`. The fuzzy search algorithm is aeine from skim which is robust to letters-out-of-order typos.

## Inline suggestion
Inline suggestions appear as you type based on the most recent matching history entry. Accept them with `Right`/`End`.

## Scroll through prefix matches
Pressing `Up` will scroll through history entries that are a prefix match with the current command.

## Zsh history entries
Optionally read zsh history entries to make migrating to bash easier. 