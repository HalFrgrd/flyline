echo 'hello from jobu'
PROMPT_COMMAND='echo $LINENO > $JOBU_FIFO_PATH 2>/dev/null || true'


# edit-and-execute-command annoyingly echos the command before executing it.
# so i need to delete that line from the terminal in debug trap.
JOBU_SHOULD_DELETE_ECHOED_COMMAND=0
dumb_editor() {
    f="$1"
    # echo hello
    [ -z "$f" ] && return 1
    [ ! -e "$f" ] && : > "$f"
    content=$(sed -n '1p' -- "$f" 2>/dev/null || printf '')
    if [ "$content" = "exit" ]; then
        return 0
    fi
    printf '%s' 'ls | head -n 2' > "$f"
    JOBU_SHOULD_DELETE_ECHOED_COMMAND=1

    return 0
}

get_rid_of_echoed_command() {
    # echo "in get_rid_of_echoed_command JOBU_SHOULD_DELETE_ECHOED_COMMAND=$JOBU_SHOULD_DELETE_ECHOED_COMMAND"
    # Clear the last line in the terminal
    [ "${JOBU_SHOULD_DELETE_ECHOED_COMMAND:-0}" -ne 1 ] && return
    tput cuu1  # Move cursor up one line
    tput el    # Clear to the end of the line
    JOBU_SHOULD_DELETE_ECHOED_COMMAND=0
}

trap 'get_rid_of_echoed_command' DEBUG

export VISUAL=dumb_editor
bind '"\C-m": edit-and-execute-command'
