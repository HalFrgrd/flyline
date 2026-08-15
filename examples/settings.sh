# These would go in your .bashrc after enabling flyline. See flyline --help for more details.
flyline --load-zsh-history
flyline set-cursor --backend terminal
flyline mouse --mode disabled
flyline editor --show-inline-history false
flyline editor --select-with-mouse false

# These are example settings that I use:

flyline --set-frame-rate 24
flyline set-agent-mode \
    --system-prompt "Be concise. Answer with a JSON array of at most 3 items with objects containing: command and description. Command will be a Bash command. " \
    --trigger-prefix ": " \
    --command 'agy --model "Gemini 3.5 Flash (Low)" --prompt'

flyline key bind Ctrl+A always=selectAll  


# ANSI escape introducer and text style constants for terminal output.
ANSI_ESC='\033['

ANSI_RESET='\033[0m'
ANSI_BOLD='\033[1m'
ANSI_DIM='\033[2m'
ANSI_ITALIC='\033[3m'
ANSI_UNDERLINE='\033[4m'

ANSI_BLACK='\033[30m'
ANSI_RED='\033[31m'
ANSI_GREEN='\033[32m'
ANSI_YELLOW='\033[33m'
ANSI_BLUE='\033[34m'
ANSI_MAGENTA='\033[35m'
ANSI_CYAN='\033[36m'
ANSI_WHITE='\033[37m'

ANSI_BRIGHT_BLACK='\033[90m'
ANSI_BRIGHT_RED='\033[91m'
ANSI_BRIGHT_GREEN='\033[92m'
ANSI_BRIGHT_YELLOW='\033[93m'
ANSI_BRIGHT_BLUE='\033[94m'
ANSI_BRIGHT_MAGENTA='\033[95m'
ANSI_BRIGHT_CYAN='\033[96m'
ANSI_BRIGHT_WHITE='\033[97m'

# Semantic aliases for status messaging.
ANSI_INFO="$ANSI_CYAN"
ANSI_WARN="$ANSI_YELLOW"
ANSI_ERROR="$ANSI_RED"
ANSI_SUCCESS="$ANSI_GREEN"

flyline create-prompt-widget mouse-mode --name MOUSE_MODE "${ANSI_GREEN}M" "${ANSI_BRIGHT_RED}X"
flyline create-prompt-widget last-command-duration --name FLYLINE_LAST_COMMAND_DUR
flyline create-prompt-widget copy-buffer --name FLYLINE_COPY_BUFFER '> '
flyline create-prompt-widget custom --name FLYLINE_ASYNC_GIT_WIDGET --command "$__DOTFILES_ROOT/helpers/prompt_git_widget.sh" --placeholder prev

# From https://github.com/rcaloras/bash-preexec
source "bash-preexec.sh"

set_ps1() {
    local -a pipe_status=("${PIPESTATUS[@]}")
    local pipe_ok=1
    local status
    for status in "${pipe_status[@]}"; do
        if [ "$status" -ne 0 ]; then
            pipe_ok=0
            break
        fi
    done

    local c_reset="\\[${ANSI_RESET}\\]"
    local c_green="\\[${ANSI_SUCCESS}\\]"
    local c_red="\\[${ANSI_ERROR}\\]"
    local c_yellow="\\[${ANSI_WARN}\\]"
    local c_dark_green="\\[${ANSI_GREEN}\\]"
    local c_cyan="\\[${ANSI_INFO}\\]"
    local pipe_color="$c_green"
    if [ "$pipe_ok" -ne 1 ]; then
        pipe_color="$c_red"
    fi

    local pipe_text="${pipe_status[*]}"
    if [ -z "$pipe_text" ]; then
        pipe_text="0"
    fi

    local cwd_widget="${PWD/#$HOME/~}"
    local term_cols="${COLUMNS:-80}"
    local max_cwd_len=$((term_cols / 2))
    local min_cwd_len=12
    if [ "$max_cwd_len" -gt 20 ]; then
        max_cwd_len=20
    fi
    if [ "$max_cwd_len" -lt "$min_cwd_len" ]; then
        max_cwd_len=$min_cwd_len
    fi

    if [ "${#cwd_widget}" -gt "$max_cwd_len" ]; then
        cwd_widget="…${cwd_widget: -$((max_cwd_len - 1))}"
    fi

    PS1="(${pipe_color}${pipe_text}${c_reset} ${c_yellow}FLYLINE_LAST_COMMAND_DUR${c_reset}) ${c_dark_green}\\h${c_reset} ${c_cyan}${cwd_widget} ${c_reset}FLYLINE_ASYNC_GIT_WIDGET\n${pipe_color}FLYLINE_COPY_BUFFER${c_reset}"
}

precmd_functions+=(set_ps1)
RPS1=" ${ANSI_YELLOW}\t${ANSI_RESET}"
PS1_FILL="MOUSE_MODE"
PROMPT_RULER="─"

PS1_FINAL="Started at ${ANSI_YELLOW}\D{%Y-%m-%d %H:%M:%S}> ${ANSI_RESET}"
RPS1_FINAL=''
PS1_FILL_FINAL=''
PROMPT_RULER_FINAL=''
