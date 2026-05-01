# Flyline

<div align="center">

[![CI](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml/badge.svg)](https://github.com/HalFrgrd/flyline/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/blob/master/LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/HalFrgrd/flyline)](https://github.com/HalFrgrd/flyline/releases)

**A Bash plugin for modern command line editing.**


![Demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_overview.gif)

</div>

When Bash prompts you for a command, a library called [readline](https://www.gnu.org/software/bash/manual/html_node/Command-Line-Editing.html) handles your keystrokes. Readline lacks many features users have come to expect. Flyline is a readline replacement that provides an enhanced line editing experience with:
- Undo and redo support
- [Agent assisted command writing](#agent-mode)
- [Rich prompt customizations, (asynchronous widgets), and animations](#rich-prompts)
- [Fuzzy history searching](#command-history)
- [Mouse support](#mouse-support)
- [Improvements to Bash's tab completion](#tab-completion-improvements)
- Tooltips
- Auto close brackets and quotes
- Syntax highlighting
- Runs in the same process as Bash
- Cursor animations and styles

Flyline is similar to [ble.sh](https://github.com/akinomyoga/ble.sh) but is written in Rust and uses [ratatui.rs](https://ratatui.rs/) to more easily draw complex user interfaces.

### Who is it for?
1. You want an out-of-the-box great shell experience without the hassle of setting up half a dozen plugins, plugin managers, keyboard shortcuts, and startup scripts (any one of which might phone home).
2. You're a terminal power user who wants to fine-tune their shell experience by writing in a modern language like Rust. Flyline can be the starting platform for you; contributions welcome!

# Installation

### Quick install: run `install.sh`

> [!TIP]
> Quick install:
> run the following command to automatically download flyline and update your `.bashrc` to load the latest version:
```bash
curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/master/install.sh | sh
```

> [!IMPORTANT]
> On macOS you must first install a version of bash that supports custom builtins: `brew install bash`

### Download from releases

Download the latest `libflyline.so` for your system from [the releases page](https://github.com/HalFrgrd/flyline/releases). If you are on Linux, you probably want the `gnu` variant unless you know you are on a `musl` based Linux distro (e.g. Alpine, Chimera).
Then, in your `.bashrc` (or in your current Bash session):
```bash
enable -f /path/to/libflyline.so flyline
flyline run-tutorial
```


### Build from source

Clone the repo and run:
```bash
cargo build
enable -f /path/to/flyline_checkout/target/debug/libflyline.so flyline
flyline run-tutorial
```


<details>
<summary><strong>Installation notes</strong></summary>

Disable flyline with `enable -d flyline`.

#### BASH_LOADABLES_PATH

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

# Configuration

Flyline sets up its own tab completion
so you can type `flyline <Tab>` in your shell to interactively browse and configure settings. Copy the commands into your `.bashrc` so they persist.

Explore this readme and [examples](examples/) for what you can configure.

# Rich prompts

Flyline supports dynamic content in `PS1`, `RPS1` / `RPROMPT`, and `PS1_FILL`.

## PS1
The `PS1` environment variable sets the left prompt just like normal. See [Bash prompt documentation](https://www.gnu.org/software/bash/manual/html_node/Controlling-the-Prompt.html), [Arch Linux wiki](https://wiki.archlinux.org/title/Bash/Prompt_customization), or [Starship](https://starship.rs/) for more information.
![PS1 demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_prompts_ps1.gif)
```bash
PS1='\u@\h:\w$ '
PS1='\u@\h:\w\n$ '
PS1='\e[01;32m\u@\h\e[00m:\e[01;34m\w\e[00m\n$ '
```

> [!TIP]
> Do git metrics slow down your prompt loading time? See [custom widget](#custom-command-widget) or [example widgets](examples/widgets.sh) for a solution.

## RPS1 / RPROMPT
The `RPS1` / `RPROMPT` variable sets the right prompt similarly to Zsh.
![RPS1 demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_prompts_rps1.gif)
```bash
RPS1='\t'
RPS1='\t\n<'
RPS1='\e[01;33m\t\n<\e[00m'
```

## PS1_FILL
`PS1_FILL` fills the gap between the `PS1` and `RPS1` lines.
![PS1_FILL demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_prompts_ps1_fill.gif)
```bash
PS1_FILL='-'
PS1_FILL='🯁🯂🯃🮲🮳' # finger pointing to running man
PS1_FILL='🯁🯂🯃🮲🮳 \D{%.3f}'
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
RPS1='\e[01;32m\t\e[0m'

# Right prompt showing 12-hour am/pm time
RPS1='\e[01;34m\@\e[0m'
```

### Custom time format with `\D{format}`

Use `\D{format}` with any [Chrono format string](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) to display the time exactly how you want it. This is similar to `\D{format}` in the [Bash prompt documentation](https://www.gnu.org/software/bash/manual/html_node/Controlling-the-Prompt.html), but the format string is interpreted by Chrono rather than strftime.

```bash
# Show date and time
RPS1='\e[01;32m\D{%Y-%m-%d %H:%M:%S}\e[0m'

# Show only hours and minutes
RPS1='\D{%H:%M}'
```



## Custom prompt widgets

Create custom prompt widgets with `flyline create-prompt-widget`.
Flyline will replace strings in the prompt matching the widget name with the widget's output.
The available widget types are `animation`, `mouse-mode`, `copy-buffer`, `custom`, and `last-command-duration`.

### Animations

Create your own animations with `flyline create-prompt-widget animation --name [your animation name here] [FRAMES]`.
Flyline will replace strings in the prompt matching the animation name with the animation:

![Custom animation demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_custom_animation.gif)

More examples can be found in [examples/animations.sh](examples/animations.sh).

The block below is auto-generated from `flyline create-prompt-widget animation --help`:

<!-- FLYLINE_CREATE_PROMPT_WIDGET_ANIMATION_HELP_START -->
```
Create a custom prompt animation that cycles through frames.

Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
with the current animation frame on every render.  Frames may include
ANSI colour sequences written as `\e` (e.g. `\e[33m`).

Examples:
  flyline create-prompt-widget animation --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽
  flyline create-prompt-widget animation --name "john" --ping-pong --fps 5  '\e[33m\u' '\e[31m\u' '\e[35m\u' '\e[36m\u'

See https://github.com/HalFrgrd/flyline/blob/master/examples/animations.sh for more details and example usage.

Usage: flyline create-prompt-widget animation [OPTIONS] --name <NAME> [FRAMES]...

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
<!-- FLYLINE_CREATE_PROMPT_WIDGET_ANIMATION_HELP_END -->

### Mouse-mode widget

The block below is auto-generated from `flyline create-prompt-widget mouse-mode --help`:

<!-- FLYLINE_CREATE_PROMPT_WIDGET_MOUSE_MODE_HELP_START -->
```
Show different text depending on whether mouse capture is enabled.

Instances of NAME in prompt strings (PS1, RPS1, PS1_FILL) are replaced
with ENABLED_TEXT when mouse capture is on, and DISABLED_TEXT when off.

Examples:
  flyline create-prompt-widget mouse-mode '🖱️' '🔴'
  # Now use FLYLINE_MOUSE_MODE in your prompt:
  PS1='\u@\h:\w [FLYLINE_MOUSE_MODE] $ '

  flyline create-prompt-widget mouse-mode --name MOUSE_MODE "on " "off"

Usage: flyline create-prompt-widget mouse-mode [OPTIONS] <ENABLED_TEXT> <DISABLED_TEXT>

Arguments:
  <ENABLED_TEXT>
          Text to display when mouse capture is enabled

  <DISABLED_TEXT>
          Text to display when mouse capture is disabled

Options:
      --name <NAME>
          Name to embed in prompt strings as the widget placeholder. Defaults to `FLYLINE_MOUSE_MODE`
          
          [default: FLYLINE_MOUSE_MODE]

  -h, --help
          Print help (see a summary with '-h')
```
<!-- FLYLINE_CREATE_PROMPT_WIDGET_MOUSE_MODE_HELP_END -->

### Copy-buffer widget

Render clickable text in your prompt that copies the current command buffer to the clipboard via OSC 52.

```bash
flyline create-prompt-widget copy-buffer '[copy]'
# Now use FLYLINE_COPY_BUFFER in your prompt:
RPS1=' FLYLINE_COPY_BUFFER'
```

### Custom command widget

The block below is auto-generated from `flyline create-prompt-widget custom --help`:

<!-- FLYLINE_CREATE_PROMPT_WIDGET_CUSTOM_HELP_START -->
```
Run a shell command and display its output in the prompt.

The output is passed through Bash's decode_prompt_string so Bash prompt
escape sequences (e.g. \u, \w, ANSI colour codes) are fully supported.

Examples:
  # Non-blocking (default): runs in the background; shows the previous output
  # while the command is running (empty on the first render).
  flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_slow_git_metrics.sh'
  # PS1 usage:
  PS1='\u@\h:\w [CUSTOM_WIDGET1] $ '

  # Non-blocking with a 10-space placeholder while the new output is being computed.
  flyline create-prompt-widget custom --name CUSTOM_WIDGET1 --command 'run_slow_git_metrics.sh' --placeholder 10

  # Blocking: waits for the command to finish before showing the prompt.
  flyline create-prompt-widget custom --name CUSTOM_WIDGET2 --command 'run_something.sh' --block

  # Blocking with a 500 ms timeout; falls back to placeholder if slower.
  flyline create-prompt-widget custom --name CUSTOM_WIDGET3 --command 'run_slow.sh --flag' --block 500 --placeholder prev

Usage: flyline create-prompt-widget custom [OPTIONS] --name <NAME> --command <COMMAND>

Options:
      --name <NAME>
          Name to embed in prompt strings as the widget placeholder

      --command <COMMAND>
          Command string to run; include any flags in the same string, e.g. --command './widget.sh --someflag'

      --block [<MS>]
          Block until the command finishes, optionally with a timeout in milliseconds. With no value, polls indefinitely (i32::MAX ms ≈ 24.8 days).  If the timeout expires the command continues running in the background and subsequent renders will pick up its output

      --placeholder <PLACEHOLDER>
          What to show while the command is running.  Either a number (spaces) or 'prev' (use the previous output of the command)

  -h, --help
          Print help (see a summary with '-h')
```
<!-- FLYLINE_CREATE_PROMPT_WIDGET_CUSTOM_HELP_END -->

### Last-command-duration widget

Show how long ago the flyline app last closed. By default it uses the compact `five-chars` format, and you can switch to a Chrono format string with `--format`.

```bash
flyline create-prompt-widget last-command-duration
# Now use FLYLINE_LAST_COMMAND_DURATION in your prompt:
RPS1=' FLYLINE_LAST_COMMAND_DURATION'

flyline create-prompt-widget last-command-duration --format "%H:%M:%S"
```


# Agent mode
Flyline can interact with your AI agent to suggest commands.
This allows you to write a command in plain English and your agent will convert it into a Bash command:

![Agent mode demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_agent_mode.gif)

After setting up your agent with flyline, you can pass the buffer to your agent with Alt+Enter or simply Enter when your command starts with your trigger prefix (e.g. `ai: list files older than three days`).

[See the examples for how to set this up.](examples/agent_mode.sh). If agent mode is not configured yet, pressing Alt+Enter will prompt flyline to help configure it.

Flyline will syntax highlight the suggested commands and render markdown output.

# Mouse support

Click to move your cursor, select suggestions, and hover for tooltips.
Flyline must capture mouse events for the entire terminal which isn't always desirable.
For instance, you might want to select text above the current prompt with your mouse.

Flyline offers three mouse modes:
- `disabled`: Never capture mouse events
- `simple`: Mouse capture is on by default; toggled when Escape is pressed
- `smart` (default): Mouse capture is on by default with automatic management: disabled on scroll or when the user clicks above the viewport, re-enabled on any keypress or when focus is regained

I'd recommend [setting up a mouse mode widget](#mouse-mode-widget) to know when mouse capture is enabled.

# Tab completion improvements
Flyline extends Bash's tab completion feature in many ways.
Note that you will need to have [set up completions in normal Bash first](https://github.com/scop/bash-completion).

### Fuzzy tab completion search
When you're presented with suggestions, you can type to fuzzily search through the list:

![Fuzzy suggestions demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_fuzzy_suggestions.gif)

### Fuzzy path completion
The last path segment will be fuzzily matched on the directory contents:

![Fuzzy path suggestions demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_fuzzy_path_suggestions.gif)


### Alias expansion
Aliases are expanded before attempting tab completion so that Bash calls the desired completion function.
For instance, if `gc` aliases to `git commit`, `gc --verbo<Tab>` will work as expected.

### Nested command contexts
Flyline supports tab completions inside subshell, command substitution, and process substitution expressions.
For instance, `ls $(grep --<Tab>)` calls `grep`'s tab completion logic if it's set up.

### Mid-word tab completions
When your cursor is midway through a word and you press tab (e.g. `grep --i<Tab>nvrte`), the left-hand side will be used in the programmable completion function but the suggestions will be fuzzily searched using the entire word.

### Dynamic descriptions
If a suggestion contains a tab character, flyline displays the contents after the tab as a description. If there are multiple tab characters, flyline will animate each tab-delimited frame at 24fps. Try `flyline set-cursor --interpolate-easing <Tab>` for an example.
<!-- This demo requires a smooth frame which the github runner recording it does not deliver. Hiding it for now since it deserves better.-->
<!-- ![Tab completion easing demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_tab_completion_easing.gif) -->

ANSI styling is supported in descriptions: any ANSI colour/style escape codes embedded in the tab-separated description text will be rendered as ratatui styled spans.

Descriptions for files are the time since last modified.

### Automatically complete based on `--help`
Coming soon: Automatically generate a completion spec for commands without one.
For now, you can manually generate a Bash completion script with `flyline comp-spec-synthesis your_command`.

### `LS_COLORS` styling
Flyline styles your filename tab completion results according to `$LS_COLORS`:

![LS_COLORS demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_ls_colors.gif)


# Command history

**Fuzzy history search:**
Flyline offers a fuzzy history search similar to fzf or skim accessed with `Ctrl+R`:

![Fuzzy history demo](https://github.com/HalFrgrd/flyline/releases/download/assets/demo_fuzzy_history.gif)

**Inline suggestions:**
Inline suggestions appear as you type based on the most recent matching history entry. Accept them by moving your cursor to the end of the line and pressing `Right`/`End`.

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

I find that Copilot can't interact with the terminal if flyline runs with certain settings. If you run into this problem, add this to the end of your `.bashrc`:
```bash
if [[ -n "${COPILOT_TERMINAL:-}" ]]; then
    RPS1=''
    flyline set-cursor --backend terminal --interpolate none
    flyline editor --show-inline-history false
fi
``` 
and set this in your `settings.json`:
```json
  "chat.tools.terminal.terminalProfile.linux": {
    "env": {
      "COPILOT_TERMINAL": "1"
    },
    "path": "bash",
  }
```

## macOS

`Command+<KEY>` shortcuts are often captured by the terminal emulator and not forwarded to the shell.
Two possible fixes are:
- Map `Command+<KEY>` to `Control+<KEY>` in your terminal emulator settings.
- Use a terminal emulator that supports [Kitty's extended keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). This allows flyline to receive `Command+<KEY>` events.

## Shell integration
Flyline prints [OSC 133](https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers) and [OSC 633](https://code.visualstudio.com/docs/terminal/shell-integration#_supported-escape-sequences) escape codes to integrate the shell with the terminal. These are on by default and can be disabled with `flyline --send-shell-integration-codes none`.

# Settings

The block below is auto-generated from `flyline --help`:

<!-- FLYLINE_HELP_START -->
```
Usage: flyline [OPTIONS] [COMMAND]

Commands:
  set-agent-mode        Configure AI agent mode.
  create-prompt-widget  Create a custom prompt widget.
  set-style             Configure the colour palette.
  set-cursor            Configure the cursor appearance and animation.
  key                   Manage keybindings.
  log                   Logging commands: dump, configure level, or stream logs.
  run-tutorial          Run the interactive tutorial for first-time users.
  editor                Configure the inline editor.
  comp-spec-synthesis   Run a command with --help, parse the output, and print a Bash completion
                        script to stdout.
  help                  Print this message or the help of the given subcommand(s)

Options:
      --version
          Show version information

      --load-zsh-history [<PATH>]
          Load Zsh history in addition to Bash history. Optionally specify a PATH to the Zsh history file

      --show-animations [<SHOW_ANIMATIONS>]
          Show animations
          
          [possible values: true, false]

      --matrix-animation [<MATRIX_ANIMATION>]
          Run matrix animation in the terminal background. Use `on` to always show it, `off` to disable it, or an integer number of seconds to show it after that many seconds of inactivity (no keypress or mouse event). Defaults to `off`; passing the flag without a value is equivalent to `on`

      --frame-rate <FPS>
          Render frame rate in frames per second (1–120, default 24)

      --mouse-mode <MODE>
          Mouse capture mode (disabled, simple, smart). Default is smart

          Possible values:
          - disabled: Never capture mouse events
          - simple:   Mouse capture is on by default; toggled when Escape is pressed
          - smart:    Mouse capture is on by default with automatic management: disabled on scroll or when the user clicks above the viewport, re-enabled on any keypress or when focus is regained

      --send-shell-integration-codes [<SEND_SHELL_INTEGRATION_CODES>]
          Send shell integration escape codes (OSC 133 / OSC 633): none, only-prompt-pos, or full

          Possible values:
          - none:            Send no shell integration codes
          - only-prompt-pos: Only send the escape codes that report prompt start/end positions
          - full:            Send the full set of shell integration codes: prompt positions, execution start/end codes, and cursor-position reporting

      --enable-extended-key-codes [<ENABLE_EXTENDED_KEY_CODES>]
          Whether to request the use of extended (kitty-protocol) keyboard codes during startup. Enabled by default; pass `--enable-extended-key-codes false` to disable it on terminals that misbehave when the request is sent
          
          [possible values: true, false]

  -h, --help
          Print help (see a summary with '-h')

Read more at https://github.com/HalFrgrd/flyline
```
<!-- FLYLINE_HELP_END -->


## Colour palette

Flyline ships with two built-in colour presets (dark and light) and lets you override individual colours.

### Presets

```bash
flyline set-style --default-theme dark   # original palette, optimised for dark terminals
flyline set-style --default-theme light  # preset optimised for light terminals
```

### Custom colours

Style strings follow the [rich](https://rich.readthedocs.io/en/stable/style.html) syntax: a
space-separated list of attributes and colours.

Supported attributes: `bold`, `dim`, `italic`, `underline`, `blink`, `reverse`, `strike`.

Colours can be specified by name (`red`, `green`, `blue`, `magenta`, `cyan`, `yellow`,
`white`, `black`, `bright_red`, …), as a 256-colour index (`color(196)`), or as an RGB
hex code (`#ff5500`) or `rgb(r,g,b)` form.

```bash
flyline set-style inline-suggestion="dim italic"
flyline set-style --default-theme light matching-char="bold blue"
flyline set-style recognised-command="green" unrecognised-command="bold red"
flyline set-style secondary-text="dim" tutorial-hint="bold italic"
flyline set-style bash-reserved="bold yellow"
```

## Keybindings

List all keybindings with `flyline key list`.
Flyline allows configurable keybindings with the `flyline key bind [KEY SEQUENCE] [CONTEXT_EXPR]=[ACTION]` subcommand.
The context expression is a `+`-separated chain of camelCase context variables (each optionally prefixed with `!` to negate).
A binding only fires when its context expression evaluates to true.
This allows the same key sequence to trigger different actions under different circumstances.

For instance:
```bash
flyline key bind Enter always=submitOrNewline
flyline key bind Enter tabCompletionAvailable=tabCompletionAcceptEntry  # defined last -> higher priority
```
When you press `Enter`, flyline will accept the tab completion entry if `tabCompletionAvailable` is true (i.e. you are currently browsing tab completion suggestions).
If `tabCompletionAvailable` is false, then it will try the next keybinding for `Enter` and run that action if its context expression evaluates to true.
The `always` context variable is always true.

A context expression may combine multiple variables with `+`:
```bash
flyline key bind Tab inlineSuggestionAvailable+cursorAtEnd=acceptInlineSuggestion
```
`||` and parentheses are not supported.  Use `!` in front of a variable to negate it (e.g. `!textSelected`).

It is possible to remap keys entirely with:
```bash
flyline key remap Alt Ctrl       # Pressing Alt now acts like pressing Ctrl
flyline key remap Ctrl Alt       # With the above command, Alt and Ctrl are effectively swapped.
```

Tab completions exist for both key sequences and context/action arguments to make it easier to write keybindings.
