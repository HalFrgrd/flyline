printf 'hello from jobu\n\r'

bind '"q": "z\C-J"'
bind '"z": "exit"'

# PS1_ORIGINAL="$PS1"
PS1="my prompt: "

jobu_start_of_prompt() {
    # echo "starting prompt"
    # PS1

    printf "${PS1}"


    # Run get-command directly in current shell, not in subshell
    local temp_output=$(mktemp)
    
    # Execute directly in current shell and redirect stderr to temp file
    $JOBU_EXEC_PATH get-command 2> "$temp_output"
    
    # Read the output from temp file (stderr output)
    local output
    output=$(cat "$temp_output")
    rm -f "$temp_output"
    
    # Extract the FORBASH command
    JOBU_COMMAND=$(echo "$output" | rg -o 'FORBASH: (.*)' -r '$1')
    # JOBU_COMMAND="${JOBU_COMMAND}"
    # printf "\n"
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
    # bind -r '\e[0n'
    # printf '\n\r'
}