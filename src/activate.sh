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

jobu_log_file="$HOME/jobu_bash.logs"

jobu_log(){
    echo "$(date '+%Y-%m-%d %H:%M:%S') [$1]: $2" >> "$jobu_log_file"
}


JOBU_SESSION_ID=$(uuidgen)


request_pipe="/tmp/jobu_request_$JOBU_SESSION_ID"
response_pipe="/tmp/jobu_response_$JOBU_SESSION_ID"

run_jobu_bash_server(){
    # Jobu will communicate with this process during cle for stuff like 
    # autocompletions and `which` lookups.

    jobu_log "SERVER" "Jobu bash server started, waiting for requests..."

    exec 0< "$request_pipe"
    exec 1> "$response_pipe"
    # Keep stderr redirected to /dev/null
    exec 2>/dev/null

    while IFS= read -r query; do
        # Handle queries with full access to parent environment
        jobu_log "SERVER" "Received query: $query"
        case "$query" in
            "get-var PATH") echo "$PATH" ;;
            "WHICH "*)
                cmd="${query#WHICH }"
                cmd_path=$(command -v "$cmd")
                jobu_log "SERVER" "cmd= $cmd Command found: $cmd_path"
                printf "%s" "$cmd_path"
                ;;
            "COMPLETE "*)
                partial="${query#COMPLETE }"
                # Simple completion logic (can be improved)
                comp_results=$(compgen -c "$partial")
                printf "%s" "$comp_results"
                ;;
            "PING")
                printf "PONG"
                ;;
            "EXIT") 
                jobu_log "SERVER" "Received EXIT command, shutting down server."
                printf "exiting"
                break
                ;;
            *) 
                printf "Unknown query: %s" "$query" ;;
        esac
        printf "\0"
    done
}

jobu_start_of_prompt() {
    history -a  # Append to history file so jobu can read latest commands

    export PS1

    # Create named pipes for jobu communication
    mkfifo "$request_pipe" 2>/dev/null || true
    mkfifo "$response_pipe" 2>/dev/null || true

    JOBU_BACKUP_STTY=$(stty -g)

    jobu_log "MAIN" "Starting jobu bash server..."

    run_jobu_bash_server &

    "$JOBU_EXEC_PATH" get-command "$request_pipe" "$response_pipe"
    sleep 1
    
    # This approach is based on test_3.sh
    JOBU_SHOULD_RESTORE=1
    JOBU_BACKUP_PS1=$PS1
    stty -echo
    PS1=""

    bind -x '"a": jobu_end_of_prompt'
    bind '"\e[0n": "a\C-J"'
    printf "\033[5n"
}


jobu_end_of_prompt() {
    # echo "in end of prompt"
    READLINE_LINE=${JOBU_COMMAND};
    READLINE_POINT=${#READLINE_LINE};
}

jobu_pre_exec() {
    if [[ -n "${JOBU_SHOULD_RESTORE:-}" ]]; then
        PS1="$JOBU_BACKUP_PS1"
        stty "$JOBU_BACKUP_STTY"
        unset JOBU_SHOULD_RESTORE
    fi
}


precmd_functions+=(jobu_start_of_prompt)
preexec_functions+=(jobu_pre_exec)

