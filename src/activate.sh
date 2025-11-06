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

run_jobu_bash_server(){
    # Jobu will communicate with this process during cle for stuff like 
    # autocompletions and `which` lookups.

    local request_pipe="/tmp/jobu_request"
    local response_pipe="/tmp/jobu_response"

    # Redirect stdin to request pipe and stdout to response pipe
    exec 0< "$request_pipe"
    exec 1> "$response_pipe"
    # Keep stderr redirected to /dev/null
    exec 2>/dev/null

    jobu_log "SERVER" "Jobu bash server started, waiting for requests..."

    while read -r query; do
        # Handle queries with full access to parent environment
        jobu_log "SERVER" "Received query: $query"
        case "$query" in
            "get-var PATH") echo "$PATH" ;;
            "WHICH "*)
                cmd="${query#WHICH }"
                jobu_log "SERVER" "Looking up command: $cmd"
                cmd_path=$(command -v "$cmd")
                result=$?
                jobu_log "SERVER" "Command found (exit code $result): $cmd_path"
                echo "$cmd_path"
                ;;
            "COMPLETE "*)
                partial="${query#COMPLETE }"
                # Simple completion logic (can be improved)
                comp_results=$(compgen -c "$partial")
                echo "$comp_results"
                ;;
            *) 
                echo "Unknown query: $query" ;;
        esac
    done
}

jobu_start_of_prompt() {

    history -a  # Append to history file so jobu can read latest commands

    export PS1


    # Create named pipes for jobu communication
    # TODO improve this
    mkfifo "/tmp/jobu_request" 2>/dev/null || true
    mkfifo "/tmp/jobu_response" 2>/dev/null || true

    # Start the jobu bash server in background
    run_jobu_bash_server &
    local child_pid=$!

    # Create a secure temporary file
    tmpfile=$(mktemp "/dev/shm/jobu.XXXXXX")
    chmod 600 "$tmpfile"
    "$JOBU_EXEC_PATH" get-command "/tmp/jobu_request" "/tmp/jobu_response" 2> "$tmpfile"
    ret=$?
    JOBU_COMMAND=$(<"$tmpfile")
    rm -f "$tmpfile"
    # Kill the jobu bash server process silently
    set +m  # Disable job control to prevent "Terminated" message
    kill "$child_pid" 2>/dev/null || true
    wait "$child_pid" 2>/dev/null || true  # Wait for process to actually exit
    set -m  # Re-enable job control
    jobu_log "MAIN" "killed jobu server with exit code $ret"

    # This approach is based on test_3.sh
    JOBU_SHOULD_RESTORE=1
    JOBU_BACKUP_PS1=$PS1
    JOBU_BACKUP_STTY=$(stty -g)
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

