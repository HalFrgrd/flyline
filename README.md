# Flyline

<div align="center">

[![CI](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml/badge.svg)](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/blob/master/LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/releases)

**A Bash plugin for modern command line editing.**


![Demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_overview.gif)

</div>

When you write a command in Bash, a library called [readline](https://www.gnu.org/software/bash/manual/html_node/Command-Line-Editing.html) handles your keystrokes. Readline lacks many features users have to come expect. Flyline is a readline replacement that provides an enhanced line editing experience with:
- Undo and redo support
- [Agent assisted command writing](#agent-mode)
- Fuzzy history suggestions
- Fuzzy autocompletions
- Integration with Bash autocomplete
- [Mouse support](#mouse-support)
- [Improvements on Bash's tab completion](#tab-completion-improvements)
- Tooltips
- Auto close brackets and quotes
- Syntax highlighting
- Runs in the same process as Bash
- Cursor animations

Flyline is similar to [ble.sh](https://github.com/akinomyoga/ble.sh) but is written in Rust and uses [ratatui.rs](https://ratatui.rs/) to more easily draw complex user interfaces.

# Installation

To install flyline, you need to:
1. Acquire `libflyline.so`
2. Run `enable -f /path/to/libflyline.so flyline` (preferably in your `.bashrc`)

From easiest to hardest:

### Run `install.sh`

Run the following command to automatically download and set your `.bashrc` to run the latest flyline version:
```bash
curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/master/install.sh | sh
```


### Download from releases

Download the latest `libflyline.so` for your system from [the releases page](https://github.com/HalFrgrd/flyline/releases). If you are on Linux, you probably want the `gnu` variant unless you are on a `musl` based Linux distro (e.g. Alpine, Chimera).
Then, in your `.bashrc` (or in your current Bash session):
```bash
enable -f /path/to/libflyline.so flyline
flyline --tutorial-mode
```


### Build from source

Clone the repo and run:
```bash
cargo build
enable -f /path/to/flyline_checkout/target/debug/libflyline.so flyline
```

### Notes

Disable flyline with `enable -d flyline`.

<details>
<summary><strong>On newer versions of Bash</strong></summary>
Taken from https://www.gnu.org/software/bash/manual/bash.html:

> The -f option means to load the new builtin command name from shared object filename, on systems that support dynamic loading. If filename does not contain a slash, Bash will use the value of the BASH_LOADABLES_PATH variable as a colon-separated list of directories in which to search for filename. The default for BASH_LOADABLES_PATH is system-dependent, and may include "." to force a search of the current directory.

Bash 4.4 introduced `BASH_LOADABLES_PATH`
Bash 5.2-alpha added a default value for `BASH_LOADABLES_PATH`.
Check your Bash version with: `bash --version`

So on Bash at least as recent as 5.2, if you install flyline to one of:
- /opt/local/lib/bash
- /opt/pkg/lib/bash
- /usr/lib/bash
- /usr/local/lib/bash
- /usr/pkg/lib/bash

Then you can simply run `enable flyline`.

</details>

# Rich prompts

Flyline supports dynamic content in `PS1`, `RPS1` / `RPROMPT`, and `PS1_FILL`.

## PS1
The `PS1` environment variable sets the left prompt just like normal. See [Bash prompt documentation](https://www.gnu.org/software/bash/manual/html_node/Controlling-the-Prompt.html), [Arch Linux wiki](https://wiki.archlinux.org/title/Bash/Prompt_customization), or [Starship integration](#starship-integration) for more information.
![PS1 demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_prompts_ps1.gif)
```bash
export PS1='\u@\h:\w$ '
export PS1='\u@\h:\w\n$ '
export PS1='\e[01;32m\u@\h\e[00m:\e[01;34m\w\e[00m\n$ '
```

## RPS1 / RPROMPT
The `RPS1` / `RPROMPT` variable sets the right prompt similarly to Zsh.
![RPS1 demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_prompts_rps1.gif)
```bash
export RPS1='\t'
export RPS1='\t\n<'
export RPS1='\e[01;33m\t\n<\e[00m'
```

## PS1_FILL
`PS1_FILL` fills the gap between the `PS1` and `RPS1` lines.
![PS1_FILL demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_prompts_ps1_fill.gif)
```bash
export PS1_FILL='-'
export PS1_FILL='🯁🯂🯃🮲🮳' # finger pointing to running man
export PS1_FILL='🯁🯂🯃🮲🮳 \D{%.3f}'
```

## Dynamic time in prompts

Flyline recognises the standard Bash time escape sequences and re-evaluates them on every prompt draw, so the time shown is always current:

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
export RPS1='\e[01;32m\t\e[0m'

# Right prompt showing 12-hour am/pm time
export RPS1='\e[01;34m\@\e[0m'
```

### Custom time format with `\D{format}`

Use `\D{format}` with any [Chrono format string](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) to display the time exactly how you want it. This is similar to `\D{format}` in the [Bash prompt documentation](https://www.gnu.org/software/bash/manual/html_node/Controlling-the-Prompt.html), but the format string is interpreted by Chrono rather than strftime.

```bash
# Show date and time
export RPS1='\e[01;32m\D{%Y-%m-%d %H:%M:%S}\e[0m'

# Show only hours and minutes
export RPS1='\D{%H:%M}'
```

## Custom animations

Create your own animations with `flyline create-anim --name [your animation name here]`.
Flyline will replace strings in the prompt matching the animation name with the animation:

![Custom animation demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_custom_animation.gif)

More examples can be found in [examples/animations.sh](examples/animations.sh).

The block below is auto-generated from `flyline create-anim --help`:

<!-- FLYLINE_CREATE_ANIM_HELP_START -->
```
Create a custom prompt animation.

Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
with the current animation frame on every render.  Frames may include
ANSI colour sequences written as `\e` (e.g. `\e[33m`).

Examples:
  flyline create-anim --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽
  flyline create-anim --name "john" --ping-pong --fps 5  '\e[33m\u' '\e[31m\u' '\e[35m\u' '\e[36m\u'

Usage: flyline create-anim [OPTIONS] --name <NAME> [FRAMES]...

Arguments:
  [FRAMES]...
          One or more animation frames (positional).  Use `\e` for the ESC character

Options:
      --name <NAME>
          Name to embed in prompt strings as the animation placeholder

      --fps <FPS>
          Playback speed in frames per second (default: 10)
          
          [default: 10]

      --ping-pong
          Reverse direction at each end instead of wrapping (ping-pong / bounce mode)

  -h, --help
          Print help (see a summary with '-h')
```
<!-- FLYLINE_CREATE_ANIM_HELP_END -->

## Starship integration
TODO:
Starship provides customizable prompts for any shell. The git metrics prompt part is very useful but can slow down the time it takes to generate the prompt. Because Flyline can redraw the prompt, it can asynchronously load the slower widgets in the background to keep the shell feeling snappy.


# Agent mode
Flyline can call an agent of your choice with the current command buffer as a prompt.
This allows you to write a command in plain English and your agent will convert it into a Bash command:

![Agent mode demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_agent_mode.gif)

[See the examples on how to set this up.](examples/agent_mode.sh)
The agent should return a simple JSON array of commands as described by the example system prompt.

# Tab completion improvements
Flyline extends Bash's tab completion feature in many ways.
Note that you will need to have [set up completions in normal Bash first](https://github.com/scop/bash-completion).

### Fuzzy tab completions
When you're presented with suggestions, you can type to fuzzily search through the list:

![Fuzzy suggestions demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_fuzzy_suggestions.gif)

### Alias expansion
Aliases are expanded before tab completion so that Bash calls the desired completion function.
For instance, if `gc` aliases to `git commit`, `gc --verbo<Tab>` will work as expected.

### Nested command
Tab completions inside subshell, command substitution, and process substitution expressions.
For instance, `ls $(grep --<Tab>)` calls `grep`'s tab completion logic if it's set up.

### Mid-word tab completions
When your cursor is midway through a word and you press tab (e.g. `grep --i<Tab>nvrte`) the left hand side will be used in the programmable completion function but the suggestions will be fuzzily searched using the entire word.

### `LS_COLORS` styling
Flyline styles your filename tab completion results according to `$LS_COLORS`:

![LS_COLORS demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_ls_colors.gif)

# Mouse support

Move your cursor, select suggestions, hover for tooltips with your mouse.
Flyline must capture mouse events for the entire terminal which isn't always desirable.
For instance, you might want to select text above the current prompt with your mouse.

Flyline offers three mouse modes:
- disabled: Never capture mouse events
- simple:   Mouse capture is on by default; toggled when Escape is pressed
- smart:    Mouse capture is on by default with automatic management: disabled on scroll or when the user clicks above the viewport, re-enabled on any keypress or when focus is regained

`flyline --mouse-mode smart` is the default.

# Command history

**Fuzzy history search:**
Flyline offers a fuzzy history search similar to fzf or skim accessed with `Ctrl+R`:

![Fuzzy history demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_fuzzy_history.gif)


**Inline suggestions:**
Inline suggestions appear as you type based on the most recent matching history entry. Accept them with `Right`/`End`.

**Scroll through prefix matches:**
Pressing `Up` will scroll through history entries that are a prefix match with the current command.

**Zsh history entries:**
Optionally read Zsh history entries to make migrating to Bash easier.

# Terminal emulator notes
## VS Code:
Recommended settings
- [`terminal.integrated.minimumContrastRatio = 1`](vscode://settings/terminal.integrated.minimumContrastRatio) to prevent the cell's foreground colour changing when it's under the cursor.
- You may want to set [`terminal.integrated.macOptionIsMeta`](vscode://settings/terminal.integrated.macOptionIsMeta) so `Option+<KEY>` shortcuts are properly recognised.
- Enable [`terminal.integrated.enableKittyKeyboardProtocol`](vscode://settings/terminal.integrated.enableKittyKeyboardProtocol) so that the integrated terminal [correctly forwards keystrokes to flyline](https://code.visualstudio.com/updates/v1_109#_new-vt-features). You will need to set [`workbench.settings.alwaysShowAdvancedSettings = 1`](vscode://settings/workbench.settings.alwaysShowAdvancedSettings) to find this setting.
- Enable [`terminal.integrated.textBlinking`](vscode://terminal.integrated.textBlinking). Few terminal emulators support this neat text style option so enjoy it!
- If keybindings are not working properly, you can debug by [Toggling Keyboard Shortcuts Troubleshooting](https://code.visualstudio.com/docs/configure/keybindings#_troubleshooting-keyboard-shortcuts).

## macOS
> [!NOTE]
> These notes are for when the terminal emulator is running on macOS and flyline is running within a remote Linux shell

`Command+<KEY>` shortcuts are often captured by the terminal emulator and not forwarded to the shell.
Two possible fixes are:
- Map `Command+<KEY>` to `Control+<KEY>` in your terminal emulator settings.
- Use a terminal emulator that supports [Kitty's extended keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). This allows flyline to receive `Command+<KEY>` events.

## Shell integration
Flyline prints [OSC 133](https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers) and [OSC 633](https://code.visualstudio.com/docs/terminal/shell-integration#_supported-escape-sequences) escape codes to integrate the shell with the terminal. These are on by default and can be disabled with `flyline --send-shell-integration-codes false`.

# Settings

Configure flyline by running `flyline [OPTIONS]` in your `.bashrc` (after the `enable` call) or in Bash session.
Run `flyline --help` to see all available options.
You could set these options in your current session but then they wouldn't persist between sessions.
[Examples can be found here.](examples/settings.sh)

The block below is auto-generated from `flyline --help`:

<!-- FLYLINE_HELP_START -->
```
Usage: flyline [OPTIONS] [COMMAND]

Commands:
  agent-mode   Configure AI agent mode.
  create-anim  Create a custom prompt animation.
  set-color    Configure the colour palette.
  key          Manage keybindings.
  help         Print this message or the help of the given subcommand(s)

Options:
      --version
          Show version information

      --dump-logs [<PATH>]
          Dump in-memory logs to file. Optionally specify a PATH; if omitted, a timestamped file is created in the current directory

      --stream-logs <PATH>
          Dump current logs to PATH and append new logs. Use `stderr` to stream to standard error

      --log-level <LEVEL>
          Set the logging level
          
          [possible values: error, warn, info, debug, trace]

      --load-zsh-history [<PATH>]
          Load Zsh history in addition to Bash history. Optionally specify a PATH to the Zsh history file; if omitted, defaults to $HOME/.zsh_history

      --tutorial-mode [<TUTORIAL_MODE>]
          Enable or disable tutorial mode with hints for first-time users. Use `--tutorial-mode false` to disable
          
          [possible values: true, false]

      --show-animations [<SHOW_ANIMATIONS>]
          Show animations
          
          [possible values: true, false]

      --show-inline-history [<SHOW_INLINE_HISTORY>]
          Show inline history suggestions
          
          [possible values: true, false]

      --auto-close-chars [<AUTO_CLOSE_CHARS>]
          Enable automatic closing character insertion (e.g. insert `)` after `(`)
          
          [possible values: true, false]

      --use-term-emulator-cursor [<USE_TERM_EMULATOR_CURSOR>]
          Use the terminal emulator's cursor instead of rendering a custom cursor
          
          [possible values: true, false]

      --matrix-animation [<MATRIX_ANIMATION>]
          Run matrix animation in the terminal background
          
          [possible values: true, false]

      --frame-rate <FPS>
          Render frame rate in frames per second (1–120, default 30)

      --mouse-mode <MODE>
          Mouse capture mode (disabled, simple, smart). Default is smart

          Possible values:
          - disabled: Never capture mouse events
          - simple:   Mouse capture is on by default; toggled when Escape is pressed
          - smart:    Mouse capture is on by default with automatic management: disabled on scroll or when the user clicks above the viewport, re-enabled on any keypress or when focus is regained

      --send-shell-integration-codes [<SEND_SHELL_INTEGRATION_CODES>]
          Send shell integration escape codes (OSC 133 / OSC 633)
          
          [possible values: true, false]

  -h, --help
          Print help (see a summary with '-h')

Read more at https://github.com/HalFrgrd/flyline
```
<!-- FLYLINE_HELP_END -->

When flyline loads, it automatically sets up its own tab completion
so you can type `flyline --<Tab>` in your shell to interactively browse and configure settings.

## Colour palette

Flyline ships with two built-in colour presets (dark and light) and lets you override individual colours.

### Presets

```bash
flyline set-color --default-theme dark   # original palette, optimised for dark terminals
flyline set-color --default-theme light  # preset optimised for light terminals
flyline set-color --default-theme auto   # detect dark/light from the terminal background colour
```

### Custom colours

Style strings follow the [rich](https://rich.readthedocs.io/en/stable/style.html) syntax: a
space-separated list of attributes and colours.

Supported attributes: `bold`, `dim`, `italic`, `underline`, `blink`, `reverse`, `strike`.

Colours can be specified by name (`red`, `green`, `blue`, `magenta`, `cyan`, `yellow`,
`white`, `black`, `bright_red`, …), as a 256-colour index (`color(196)`), or as an RGB
hex code (`#ff5500`) or `rgb(r,g,b)` form.

```bash
flyline set-color --inline-suggestion "dim italic"
flyline set-color --default-theme light --matching-char "bold blue"
flyline set-color --recognised-command "green" --unrecognised-command "bold red"
flyline set-color --secondary-text "dim" --tutorial-hint "bold italic"overrides
```
