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

bind '"q": "z\C-J"'
bind '"z": "exit"'

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
    PS1=$(echo "$output" | rg -o 'PS1: (.*)' -r '$1')

    bind -x '"j": jobu_end_of_prompt'
    bind '"\e[0n": "j\C-J"'
    # bind -x '"\e[0n": echo "received"'
    echo -en "\033[5n"
}

PROMPT_COMMAND='jobu_start_of_prompt'


jobu_end_of_prompt() {
    # bash will always print the prompt and the READLINE_LINE after this function returns
    # the prompt is printed because are just in the middle of executing the PROMPT_COMMAND
    # and bash prints the prompt after PROMPT_COMMAND completes
    # and it seems to print READLINE_LINE just before executing the command as well
    READLINE_LINE=${JOBU_COMMAND};
    READLINE_POINT=${#READLINE_LINE};
    bind '"j": self-insert'
    bind -r '\e[0n'
}