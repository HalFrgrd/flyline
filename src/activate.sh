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

    jobu_log "SERVER" "Jobu bash server started, waiting for requests..."

    while read -r query; do
        # Handle queries with full access to parent environment
        jobu_log "SERVER" "Received query: $query"
        case "$query" in
            "WHICH "*)
                cmd="${query#WHICH }"
                cmd_path=$(command -v "$cmd")
                result=$?
                response_len=${#cmd_path}
                jobu_log "SERVER" "Response length: $response_len bytes"
                jobu_log "SERVER" "Command found (exit code $result): $cmd_path"
                echo "RESP_LEN=$response_len"
                echo "RESP_BODY=$cmd_path"
                ;;
            "COMPLETE "*)
                partial="${query#COMPLETE }"
                # Simple completion logic (can be improved)
                comp_results=$(compgen -c "$partial")
                result=$?
                response_len=${#comp_results}
                jobu_log "SERVER" "Completion results length: $response_len bytes"
                jobu_log "SERVER" "Completion results for (exit code $result) '$partial': $comp_results"
                echo "RESP_LEN=$response_len"
                echo "RESP_BODY=$comp_results"
                ;;
            "SETCMD "*)
                cmd_to_set="${query#SETCMD }"
                export JOBU_COMMAND="$cmd_to_set"
                jobu_log "SERVER" "Set JOBU_COMMAND to: $JOBU_COMMAND"
                echo "RESP_LEN=4"
                echo "RESP_BODY=done"
                ;;
            "PING")
                echo "PONG"
                ;;
            *)
                echo "Unknown query: $query" ;;
        esac
    done
}

jobu_start_of_prompt() {
    history -a  # Append to history file so jobu can read latest commands

    export PS1

    JOBU_COMMAND="unbound"
    JOBU_BACKUP_STTY=$(stty -g)

    coproc JOBUINSTANCE {
        jobu_log "MAIN" "Starting Jobu instance..."
        "$JOBU_EXEC_PATH" get-command;
        jobu_log "MAIN" "Finished Jobu instance."

        }

    run_jobu_bash_server <&$"${JOBUINSTANCE[0]}" >&$"${JOBUINSTANCE[1]}"
    jobu_log "MAIN" "finished running jobu server"

    wait "${JOBUINSTANCE_PID}"


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

