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

PS1=""

jobu_start_of_prompt() {

    # Run get-command directly in current shell, not in subshell
    local temp_output=$(mktemp)
    # Execute directly in current shell and redirect stderr to temp file
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

    stty -echo
    printf "\033[5n"
}

PROMPT_COMMAND='jobu_start_of_prompt'

jobu_end_of_prompt() {
    # bash will always print the prompt and the READLINE_LINE after this function returns
    # the prompt is printed because are just in the middle of executing the PROMPT_COMMAND
    # and bash prints the prompt after PROMPT_COMMAND completes
    # and it seems to print READLINE_LINE just before executing the command as well
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
    stty echo
}

trap 'jobu_restore_output' DEBUG