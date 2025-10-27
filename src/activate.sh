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

# JOBU_SESSION_KEY="$RANDOM$RANDOM$RANDOM$RANDOM$RANDOM"; # Random generates a number b/w 0 - 32767
# JOBU_SESSION_KEY="${JOBU_SESSION_KEY}0000000000000000" # Pad it to 16+ chars.
# export JOBU_SESSION_KEY=${JOBU_SESSION_KEY:0:16}; # Trim to 16-digits if excess.

bind 'set enable-bracketed-paste off'

PS1="MYPROMPT> "

jobu_start_of_prompt() {
    # printf "\033[?1049l"

    # see test_8.sh for explanation
    JOBU_ORIGINAL_PS0=$PS0
    PS0="\033[?1049l"
    # PS0=""
    export PS0
    export PS1
    
    # Create a secure temporary file
    tmpfile=$(mktemp "/dev/shm/jobu.${JOBU_SESSION_KEY}.XXXXXX")
    chmod 600 "$tmpfile"
    "$JOBU_EXEC_PATH" get-command 2> "$tmpfile"
    ret=$?
    JOBU_COMMAND=$(<"$tmpfile")
    rm -f "$tmpfile"

    # if [ $ret -ne 0 ]; then
    #     echo "jobu get-command failed with exit code $ret" >&2
    #     sleep 1
    # fi

    # If the command is empty or only whitespace, return early
    # if [[ -z "$JOBU_COMMAND" || "${JOBU_COMMAND:0:1}" = "#" ]]; then
    #     return
    # fi
    # echo "finished here"
    bind -x '"a": jobu_end_of_prompt'
    bind '"\e[0n": "a\C-J"'
    printf "\033[5n"
}

# Append to existing PROMPT_COMMAND instead of overwriting
if [[ -n "$PROMPT_COMMAND" ]]; then
    PROMPT_COMMAND="$PROMPT_COMMAND; jobu_start_of_prompt"
else
    PROMPT_COMMAND='jobu_start_of_prompt'
fi

jobu_end_of_prompt() {
    echo "in end of prompt"
    READLINE_LINE=${JOBU_COMMAND};
    READLINE_POINT=${#READLINE_LINE};
    printf "\033[?1049h"
}
