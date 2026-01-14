# printf 'hello from flyline\n\r'

# Print colorful FLYLINE logo
echo ""
echo -e "\033[38;5;196m     ██╗ \033[38;5;208m ██████╗ \033[38;5;220m██████╗\033[38;5;46m██╗   ██╗\033[0m"
echo -e "\033[38;5;196m     ██║\033[38;5;208m██╔═══██╗\033[38;5;220m██╔══██╗\033[38;5;46m██║   ██║\033[0m"
echo -e "\033[38;5;196m     ██║\033[38;5;208m██║   ██║\033[38;5;220m██████╔╝\033[38;5;46m██║   ██║\033[0m"
echo -e "\033[38;5;196m██   ██║\033[38;5;208m██║   ██║\033[38;5;220m██╔══██╗\033[38;5;46m██║   ██║\033[0m"
echo -e "\033[38;5;196m╚█████╔╝\033[38;5;208m╚██████╔╝\033[38;5;220m██████╔╝\033[38;5;46m╚██████╔╝\033[0m"
echo -e "\033[38;5;196m ╚════╝ \033[38;5;208m ╚═════╝ \033[38;5;220m╚═════╝ \033[38;5;46m ╚═════╝ \033[0m"
echo -e "\033[38;5;129m            ~ A big hook into bash ~\033[0m"
echo ""

# bind '"q": "z\C-J"'
# bind '"z": "exit"'

bind 'set enable-bracketed-paste off'

PS1="MYPROMPT> "

flyline_start_of_prompt() {

    # Run get-command directly in current shell, not in subshell
    local temp_output=$(mktemp)
    # Execute directly in current shell and redirect stderr to temp file
    # ensure PS1 is exported so the flyline binary inherits it
    export PS1
    $FLYLINE_EXEC_PATH get-command 2> "$temp_output"
    # Read the output from temp file (stderr output)
    local output
    output=$(cat "$temp_output")
    rm -f "$temp_output"
    

    FLYLINE_COMMAND=$(echo "$output" | rg -o 'COMMAND: (.*)' -r '$1')
    printf "\n"

    FLYLINE_NEED_RESTORE_OUTPUT=1
    bind -x '"a": flyline_end_of_prompt'
    bind '"\e[0n": "a\C-J"'

    # When we use a `bind -x`, readline hands over to bash for execution
    # then it assumes the prompt is dirty so it tries its best to redraw it.
    # we dont want anything to be drawn over what flyline has draw to the terminal
    # so we temporarily clear the PS1 variable
    FLYLINE_ORIGINAL_PS1=$PS1
    PS1=""

    # https://unix.stackexchange.com/questions/797263/tty-line-discipline-echoing-feature
    # see behaviours/strace_with_-echo.txt
    # readline will "echo" the READLINE_LINE after running flyline_end_of_prompt
    # because it thinks bash might have made the prompt / command dirty
    stty -echo
    printf "\033[5n"
}

PROMPT_COMMAND='flyline_start_of_prompt'

flyline_end_of_prompt() {
    READLINE_LINE=${FLYLINE_COMMAND};
    READLINE_POINT=${#READLINE_LINE};

    bind '"a": self-insert'
    bind -r '\e[0n'
}


flyline_restore_output() {
    if [ -z "$FLYLINE_NEED_RESTORE_OUTPUT" ]; then
        return
    fi
    unset FLYLINE_NEED_RESTORE_OUTPUT
    PS1=$FLYLINE_ORIGINAL_PS1
    stty echo
}

trap 'flyline_restore_output' DEBUG