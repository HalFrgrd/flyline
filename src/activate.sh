# printf 'hello from jobu\n\r'

# Print colorful JOBU logo
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

jobu_start_of_prompt() {

    # Run get-command directly in current shell, not in subshell
    local temp_output=$(mktemp)
    # Execute directly in current shell and redirect stderr to temp file
    # ensure PS1 is exported so the jobu binary inherits it
    export PS1
    $JOBU_EXEC_PATH get-command 2> "$temp_output"
    # Read the output from temp file (stderr output)
    local output
    output=$(cat "$temp_output")
    rm -f "$temp_output"
    

    JOBU_COMMAND=$(echo "$output" | rg -o 'COMMAND: (.*)' -r '$1')
    printf "\n"

    JOBU_NEED_RESTORE_OUTPUT=1
    bind -x '"a": jobu_end_of_prompt'
    bind '"\e[0n": "a\C-J"'

    # When we use a `bind -x`, readline hands over to bash for execution
    # then it assumes the prompt is dirty so it tries its best to redraw it.
    # we dont want anything to be drawn over what jobu has draw to the terminal
    # so we temporarily clear the PS1 variable
    JOBU_ORIGINAL_PS1=$PS1
    PS1=""

    # https://unix.stackexchange.com/questions/797263/tty-line-discipline-echoing-feature
    # see behaviours/strace_with_-echo.txt
    # readline will "echo" the READLINE_LINE after running jobu_end_of_prompt
    # because it thinks bash might have made the prompt / command dirty
    stty -echo
    printf "\033[5n"
    printf "\033[5n"
    
}

PROMPT_COMMAND='jobu_start_of_prompt'

jobu_end_of_prompt() {
    READLINE_LINE=${JOBU_COMMAND};
    READLINE_POINT=${#READLINE_LINE};

    bind '"a": self-insert'
    bind -r '\e[0n'
}


jobu_restore_output() {
    if [ -z "$JOBU_NEED_RESTORE_OUTPUT" ]; then
        return
    fi
    unset JOBU_NEED_RESTORE_OUTPUT
    PS1=$JOBU_ORIGINAL_PS1
    stty echo
}

trap 'jobu_restore_output' DEBUG